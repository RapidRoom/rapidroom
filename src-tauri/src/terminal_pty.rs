use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State, WebviewWindow, ipc::Channel};

const MAX_SESSIONS: usize = 16;
const MAX_INPUT: usize = 65_536;
const HISTORY_LIMIT: usize = 262_144;
const IN_FLIGHT_LIMIT: usize = 262_144;

#[derive(Clone, Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum TerminalEvent {
    Output { sequence: u64, data: Vec<u8> },
    Exit { code: Option<u32> },
}

#[derive(Default)]
struct Output {
    history: VecDeque<u8>,
    subscriber: Option<(String, Channel<TerminalEvent>)>,
    sequence: u64,
    pending: VecDeque<(u64, usize)>,
    pending_bytes: usize,
    closed: bool,
    exited: Option<Option<u32>>,
}

struct Session {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    output: Mutex<Output>,
    ready: Condvar,
    #[cfg(unix)]
    pid: Option<u32>,
    #[cfg(windows)]
    killer: Mutex<Box<dyn portable_pty::ChildKiller + Send + Sync>>,
}

impl Session {
    fn send(output: &mut Output, data: Vec<u8>) {
        if data.is_empty() || output.subscriber.is_none() {
            return;
        }
        output.sequence += 1;
        let sequence = output.sequence;
        output.pending_bytes += data.len();
        output.pending.push_back((sequence, data.len()));
        if output
            .subscriber
            .as_ref()
            .unwrap()
            .1
            .send(TerminalEvent::Output { sequence, data })
            .is_err()
        {
            output.subscriber = None;
            output.pending.clear();
            output.pending_bytes = 0;
        }
    }

    fn push(&self, data: Vec<u8>) {
        let mut output = self.output.lock().unwrap();
        while !output.closed
            && output.subscriber.is_some()
            && output.pending_bytes + data.len() > IN_FLIGHT_LIMIT
        {
            output = self.ready.wait(output).unwrap();
        }
        if output.closed {
            return;
        }
        output.history.extend(data.iter().copied());
        let excess = output.history.len().saturating_sub(HISTORY_LIMIT);
        output.history.drain(..excess);
        Self::send(&mut output, data);
        self.ready.notify_all();
    }

    fn attach(&self, attachment: String, channel: Channel<TerminalEvent>) {
        let mut output = self.output.lock().unwrap();
        output.subscriber = Some((attachment, channel));
        output.pending.clear();
        output.pending_bytes = 0;
        let history = output.history.iter().copied().collect();
        Self::send(&mut output, history);
        if let Some(code) = output.exited
            && let Some((_, channel)) = output.subscriber.as_ref()
        {
            let _ = channel.send(TerminalEvent::Exit { code });
        }
        self.ready.notify_all();
    }

    fn detach(&self, attachment: &str) {
        let mut output = self.output.lock().unwrap();
        if output
            .subscriber
            .as_ref()
            .is_some_and(|(id, _)| id == attachment)
        {
            output.subscriber = None;
            output.pending.clear();
            output.pending_bytes = 0;
            self.ready.notify_all();
        }
    }

    fn acknowledge(&self, attachment: &str, sequence: u64) {
        let mut output = self.output.lock().unwrap();
        if !output
            .subscriber
            .as_ref()
            .is_some_and(|(id, _)| id == attachment)
        {
            return;
        }
        if sequence > output.sequence {
            return;
        }
        while output
            .pending
            .front()
            .is_some_and(|(id, _)| *id <= sequence)
        {
            let (_, bytes) = output.pending.pop_front().unwrap();
            output.pending_bytes -= bytes;
        }
        self.ready.notify_all();
    }

    fn close(&self) {
        {
            let mut output = self.output.lock().unwrap();
            output.closed = true;
            output.subscriber = None;
            self.ready.notify_all();
        }
        #[cfg(target_os = "linux")]
        if let Some(session_id) = self.pid
            && let Ok(processes) = std::fs::read_dir("/proc")
        {
            for entry in processes.flatten() {
                if let Some(pid) = entry
                    .file_name()
                    .to_str()
                    .and_then(|s| s.parse::<i32>().ok())
                    && unsafe { libc::getsid(pid) } == session_id as i32
                {
                    unsafe {
                        libc::kill(pid, libc::SIGHUP);
                        libc::kill(pid, libc::SIGKILL);
                    }
                }
            }
        }
        #[cfg(target_os = "macos")]
        if let Some(pid) = self.pid {
            let foreground = self.master.lock().unwrap().process_group_leader();
            for group in [Some(pid as i32), foreground].into_iter().flatten() {
                if unsafe { libc::getsid(group) } == pid as i32 {
                    unsafe {
                        libc::kill(-group, libc::SIGHUP);
                        libc::kill(-group, libc::SIGKILL);
                    }
                }
            }
        }
        #[cfg(windows)]
        let _ = self.killer.lock().unwrap().kill();
    }
}

#[derive(Clone, Default)]
pub struct Sessions(Arc<Mutex<Registry>>);

#[derive(Default)]
struct Registry {
    active: HashMap<String, Arc<Session>>,
    closed: HashSet<String>,
    shutdown: bool,
}

impl Sessions {
    fn get(&self, id: &str) -> Result<Arc<Session>, String> {
        self.0
            .lock()
            .unwrap()
            .active
            .get(id)
            .cloned()
            .ok_or_else(|| "Terminal tab is closed".into())
    }

    fn open(
        &self,
        id: String,
        command: CommandBuilder,
        size: PtySize,
        attachment: String,
        channel: Channel<TerminalEvent>,
    ) -> Result<String, String> {
        uuid::Uuid::parse_str(&id).map_err(|_| "Invalid terminal tab".to_string())?;
        let mut sessions = self.0.lock().unwrap();
        if sessions.shutdown || sessions.closed.contains(&id) {
            return Err("Terminal tab is closed".into());
        }
        if let Some(session) = sessions.active.get(&id) {
            session.attach(attachment, channel);
            return Ok(id);
        }
        if sessions.active.len() >= MAX_SESSIONS {
            return Err("Close a terminal tab before opening another".into());
        }
        let pair = native_pty_system()
            .openpty(size)
            .map_err(|e| e.to_string())?;
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| e.to_string())?;
        drop(pair.slave);
        let session = Arc::new(Session {
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            output: Mutex::new(Output::default()),
            ready: Condvar::new(),
            #[cfg(unix)]
            pid: child.process_id(),
            #[cfg(windows)]
            killer: Mutex::new(child.clone_killer()),
        });
        session.attach(attachment, channel);
        sessions.active.insert(id.clone(), Arc::clone(&session));
        let reading = Arc::clone(&session);
        std::thread::spawn(move || {
            let mut buffer = [0; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(length) => reading.push(buffer[..length].to_vec()),
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        });
        std::thread::spawn(move || {
            let code = child.wait().ok().map(|status| status.exit_code());
            let mut output = session.output.lock().unwrap();
            output.exited = Some(code);
            if let Some((_, channel)) = output.subscriber.as_ref() {
                let _ = channel.send(TerminalEvent::Exit { code });
            }
            session.ready.notify_all();
        });
        Ok(id)
    }

    pub fn close_all(&self) {
        let sessions = {
            let mut registry = self.0.lock().unwrap();
            registry.shutdown = true;
            std::mem::take(&mut registry.active)
        };
        for session in sessions.into_values() {
            session.close();
        }
    }
}

pub(crate) fn caller(window: &WebviewWindow) -> Result<(), String> {
    let url = window.url().map_err(|e| e.to_string())?;
    let production = url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && ((url.scheme() == "tauri" && url.host_str() == Some("localhost"))
            || (matches!(url.scheme(), "http" | "https")
                && url.host_str() == Some("tauri.localhost")));
    let development = cfg!(debug_assertions)
        && window
            .app_handle()
            .config()
            .build
            .dev_url
            .as_ref()
            .is_some_and(|dev| dev.origin() == url.origin());
    if window.label() != "main" || !(production || development) {
        return Err("Terminal commands are available only inside RapidRoom".into());
    }
    Ok(())
}

fn size(cols: u16, rows: u16) -> Result<PtySize, String> {
    if cols == 0 || rows == 0 || cols > 2048 || rows > 512 {
        return Err("Invalid terminal size".into());
    }
    Ok(PtySize {
        cols,
        rows,
        pixel_width: 0,
        pixel_height: 0,
    })
}

fn shell(path: &Path, version: &str, custom: Option<&str>) -> Result<CommandBuilder, String> {
    if !path.is_absolute() || !path.is_dir() {
        return Err("Terminal folder does not exist".into());
    }
    let mut command = CommandBuilder::new_default_prog();
    if let Some(custom) = custom.filter(|s| !s.trim().is_empty()) {
        if custom.len() > 4096 || !Path::new(custom).is_absolute() || !Path::new(custom).is_file() {
            return Err("Choose an existing absolute shell path".into());
        }
        #[cfg(unix)]
        {
            let name = std::ffi::CString::new(custom).map_err(|_| "Invalid shell path")?;
            if unsafe { libc::access(name.as_ptr(), libc::X_OK) } != 0 {
                return Err("Selected shell is not executable".into());
            }
            command.env("SHELL", custom);
        }
        #[cfg(windows)]
        {
            command = CommandBuilder::new(custom);
        }
    }
    command.cwd(path);
    command.env("PWD", path);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("RAPIDROOM_FOLDER", path);
    command.env("RAPIDROOM_VERSION", version);
    let (remove, rewrite) = crate::terminal::appimage_env_fixes(|name| std::env::var(name).ok());
    for name in remove {
        command.env_remove(name);
    }
    for (name, value) in rewrite {
        command.env(name, value);
    }
    Ok(command)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRequest {
    id: String,
    path: String,
    cols: u16,
    rows: u16,
    attachment: String,
}

#[tauri::command]
pub async fn pty_open(
    window: WebviewWindow,
    app: AppHandle,
    sessions: State<'_, Sessions>,
    request: OpenRequest,
    on_event: Channel<TerminalEvent>,
) -> Result<String, String> {
    caller(&window)?;
    if std::env::var_os("FLATPAK_ID").is_some() {
        return Err("The built-in terminal is unavailable in Flatpak".into());
    }
    let size = size(request.cols, request.rows)?;
    let settings_path = crate::app_settings::get_settings_path(&app)?;
    let settings = match std::fs::read(settings_path) {
        Ok(data) => Some(
            serde_json::from_slice::<serde_json::Value>(&data)
                .map_err(|_| "Saved terminal settings are unreadable".to_string())?,
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
    };
    let custom = settings
        .as_ref()
        .and_then(|s| s.get("terminalSettings"))
        .and_then(|s| s.get("shell"))
        .and_then(|s| s.as_str());
    let command = shell(
        Path::new(&request.path),
        &app.package_info().version.to_string(),
        custom,
    )?;
    let sessions = sessions.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        sessions.open(request.id, command, size, request.attachment, on_event)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn pty_write(
    window: WebviewWindow,
    sessions: State<'_, Sessions>,
    id: String,
    data: Vec<u8>,
) -> Result<(), String> {
    caller(&window)?;
    if data.len() > MAX_INPUT {
        return Err("Terminal input is too large".into());
    }
    let session = sessions.get(&id)?;
    if session.output.lock().unwrap().exited.is_some() {
        return Err("Terminal session ended".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut writer = session.writer.lock().unwrap();
        writer
            .write_all(&data)
            .and_then(|()| writer.flush())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn pty_resize(
    window: WebviewWindow,
    sessions: State<'_, Sessions>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    caller(&window)?;
    sessions
        .get(&id)?
        .master
        .lock()
        .unwrap()
        .resize(size(cols, rows)?)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pty_ack(
    window: WebviewWindow,
    sessions: State<'_, Sessions>,
    id: String,
    attachment: String,
    sequence: u64,
) -> Result<(), String> {
    caller(&window)?;
    sessions.get(&id)?.acknowledge(&attachment, sequence);
    Ok(())
}

#[tauri::command]
pub fn pty_detach(
    window: WebviewWindow,
    sessions: State<'_, Sessions>,
    id: String,
    attachment: String,
) -> Result<(), String> {
    caller(&window)?;
    if let Ok(session) = sessions.get(&id) {
        session.detach(&attachment);
    }
    Ok(())
}

#[tauri::command]
pub fn pty_close(
    window: WebviewWindow,
    sessions: State<'_, Sessions>,
    id: String,
) -> Result<(), String> {
    caller(&window)?;
    let session = {
        let mut registry = sessions.0.lock().unwrap();
        registry.closed.insert(id.clone());
        registry.active.remove(&id)
    };
    if let Some(session) = session {
        session.close();
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn channel() -> Channel<TerminalEvent> {
        Channel::new(|_| Ok(()))
    }

    fn wait_until(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !predicate() {
            assert!(Instant::now() < deadline, "PTY condition timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn real_pty_preserves_cwd_environment_resize_and_cleans_background_job() {
        let folder = tempfile::tempdir().unwrap();
        let sessions = Sessions::default();
        let id = uuid::Uuid::new_v4().to_string();
        let mut command = CommandBuilder::new("/bin/sh");
        command.cwd(folder.path());
        command.env("RAPIDROOM_FOLDER", folder.path());
        command.arg("-c");
        command.arg("printf 'FOLDER=%s\\n' \"$RAPIDROOM_FOLDER\"; pwd; stty size; sleep 120 & printf 'BACKGROUND=%s\\n' \"$!\"; read answer; stty size; wait");
        sessions
            .open(
                id.clone(),
                command,
                size(80, 24).unwrap(),
                "test".into(),
                channel(),
            )
            .unwrap();
        let session = sessions.get(&id).unwrap();
        session.detach("test");
        let history = || {
            String::from_utf8_lossy(
                &session
                    .output
                    .lock()
                    .unwrap()
                    .history
                    .iter()
                    .copied()
                    .collect::<Vec<_>>(),
            )
            .into_owned()
        };
        wait_until(|| history().contains("BACKGROUND="));
        let before = history();
        assert!(before.contains(folder.path().to_str().unwrap()));
        assert!(before.contains("24 80"));
        let background: i32 = before
            .lines()
            .find_map(|line| line.strip_prefix("BACKGROUND="))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(
            unsafe { libc::getsid(background) },
            session.pid.unwrap() as i32
        );
        session
            .master
            .lock()
            .unwrap()
            .resize(size(120, 40).unwrap())
            .unwrap();
        session
            .writer
            .lock()
            .unwrap()
            .write_all(b"continue\n")
            .unwrap();
        wait_until(|| history().contains("40 120"));
        sessions.close_all();
        wait_until(|| {
            let stat = std::fs::read_to_string(format!("/proc/{background}/stat"));
            stat.is_err()
                || stat
                    .unwrap()
                    .rsplit_once(") ")
                    .is_some_and(|(_, rest)| rest.starts_with('Z'))
        });
        assert!(session.output.lock().unwrap().closed);
    }

    #[test]
    fn bounded_history_and_attachment_generations_control_backpressure() {
        let pair = native_pty_system().openpty(size(80, 24).unwrap()).unwrap();
        let writer = pair.master.take_writer().unwrap();
        let session = Arc::new(Session {
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            output: Mutex::new(Output::default()),
            ready: Condvar::new(),
            pid: None,
        });
        session.push(vec![1; HISTORY_LIMIT + 64]);
        assert_eq!(session.output.lock().unwrap().history.len(), HISTORY_LIMIT);
        session.attach("current".into(), channel());
        let sequence = session.output.lock().unwrap().sequence;
        let sender = Arc::clone(&session);
        let completed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done = Arc::clone(&completed);
        let thread = std::thread::spawn(move || {
            sender.push(vec![2; 8192]);
            done.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        session.acknowledge("old", sequence);
        session.acknowledge("current", sequence + 10);
        assert_eq!(
            session.output.lock().unwrap().pending_bytes,
            IN_FLIGHT_LIMIT
        );
        session.acknowledge("current", sequence);
        wait_until(|| completed.load(std::sync::atomic::Ordering::SeqCst));
        thread.join().unwrap();
        session.detach("old");
        assert!(session.output.lock().unwrap().subscriber.is_some());
        session.detach("current");
        assert_eq!(session.output.lock().unwrap().pending_bytes, 0);
        session.close();
    }

    #[test]
    fn closed_tabs_and_shutdown_reject_delayed_open_without_spawning() {
        let sessions = Sessions::default();
        let id = uuid::Uuid::new_v4().to_string();
        sessions.0.lock().unwrap().closed.insert(id.clone());
        assert!(
            sessions
                .open(
                    id,
                    CommandBuilder::new("/bin/sh"),
                    size(80, 24).unwrap(),
                    "test".into(),
                    channel()
                )
                .is_err()
        );
        sessions.close_all();
        assert!(
            sessions
                .open(
                    uuid::Uuid::new_v4().to_string(),
                    CommandBuilder::new("/bin/sh"),
                    size(80, 24).unwrap(),
                    "test".into(),
                    channel()
                )
                .is_err()
        );
        assert!(sessions.0.lock().unwrap().active.is_empty());
        assert!(size(0, 24).is_err());
        assert!(shell(Path::new("relative"), "test", None).is_err());
    }
}
