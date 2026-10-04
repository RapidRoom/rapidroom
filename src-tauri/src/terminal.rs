use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

use tauri::AppHandle;

#[derive(Debug, PartialEq, Eq)]
struct Launch {
    program: String,
    args: Vec<String>,
}

fn launch(program: &str, args: &[&str]) -> Launch {
    Launch {
        program: program.to_string(),
        args: args.iter().map(|a| a.to_string()).collect(),
    }
}

// Most terminals inherit the working directory, but single-instance ones (Ghostty, kitty,
// GNOME's) can open the window in another process, so pass it explicitly where we know how.
fn known_terminal(program: &str, dir: &str) -> Option<Launch> {
    let name = Path::new(program).file_name()?.to_str()?;
    let wd = format!("--working-directory={dir}");
    Some(match name {
        "ghostty" | "foot" | "gnome-terminal" | "kgx" | "xfce4-terminal" | "tilix" => {
            launch(program, &[&wd])
        }
        "ptyxis" => launch(program, &["--new-window", &wd]),
        "alacritty" => launch(program, &["--working-directory", dir]),
        "kitty" => launch(program, &["--directory", dir]),
        "wezterm" => launch(program, &["start", "--cwd", dir]),
        "konsole" => launch(program, &["--workdir", dir]),
        "xdg-terminal-exec" | "x-terminal-emulator" | "xterm" => launch(program, &[]),
        _ => return None,
    })
}

const LINUX_TERMINALS: &[&str] = &[
    "xdg-terminal-exec",
    "ghostty",
    "foot",
    "kitty",
    "alacritty",
    "wezterm",
    "ptyxis",
    "gnome-terminal",
    "kgx",
    "konsole",
    "xfce4-terminal",
    "tilix",
    "x-terminal-emulator",
    "xterm",
];

fn linux_candidates(dir: &str, terminal_env: Option<&str>) -> Vec<Launch> {
    let mut candidates = Vec::new();
    if let Some(term) = terminal_env.map(str::trim).filter(|t| !t.is_empty()) {
        candidates.push(known_terminal(term, dir).unwrap_or_else(|| launch(term, &[])));
    }
    candidates.extend(
        LINUX_TERMINALS
            .iter()
            .filter_map(|program| known_terminal(program, dir)),
    );
    candidates
}

const APPIMAGE_VARS: &[&str] = &[
    "LD_LIBRARY_PATH",
    "LD_PRELOAD",
    "GDK_PIXBUF_MODULE_FILE",
    "GDK_PIXBUF_MODULEDIR",
    "GIO_MODULE_DIR",
    "GIO_EXTRA_MODULES",
    "GTK_PATH",
    "GTK_EXE_PREFIX",
    "GTK_DATA_PREFIX",
    "GTK_IM_MODULE_FILE",
    "GSETTINGS_SCHEMA_DIR",
    "PYTHONHOME",
    "PYTHONPATH",
    "PERLLIB",
    "QT_PLUGIN_PATH",
    "APPDIR",
    "APPIMAGE",
    "ARGV0",
    "OWD",
];

// An AppImage points library and data paths into its own mount; a terminal (and the shell and
// tools inside it) must not inherit them. Returns the variables to remove and the ones to rewrite.
fn appimage_env_fixes(
    get: impl Fn(&str) -> Option<String>,
) -> (Vec<String>, Vec<(String, String)>) {
    let Some(appdir) = get("APPDIR").filter(|d| !d.is_empty()) else {
        return (Vec::new(), Vec::new());
    };
    let remove = APPIMAGE_VARS
        .iter()
        .filter(|v| get(v).is_some())
        .map(|v| v.to_string())
        .collect();
    let rewrite = ["PATH", "XDG_DATA_DIRS", "XDG_CONFIG_DIRS"]
        .iter()
        .filter_map(|var| {
            let value = get(var)?;
            let kept: Vec<&str> = value
                .split(':')
                .filter(|entry| !entry.is_empty() && !entry.starts_with(&appdir))
                .collect();
            let kept = kept.join(":");
            (kept != value).then(|| (var.to_string(), kept))
        })
        .collect();
    (remove, rewrite)
}

fn prepare(command: &mut Command, dir: &Path, version: &str) {
    command
        .current_dir(dir)
        .env("PWD", dir)
        .env("RAPIDROOM_VERSION", version)
        .env("RAPIDROOM_FOLDER", dir);
    let (remove, rewrite) = appimage_env_fixes(|var| std::env::var(var).ok());
    for var in remove {
        command.env_remove(var);
    }
    for (var, value) in rewrite {
        command.env(var, value);
    }
}

fn spawn_first(candidates: Vec<Launch>, dir: &Path, version: &str) -> Result<String, String> {
    let mut tried = Vec::new();
    for candidate in candidates {
        let mut command = Command::new(&candidate.program);
        command.args(&candidate.args);
        prepare(&mut command, dir, version);
        match command.spawn() {
            Ok(mut child) => {
                std::thread::spawn(move || child.wait());
                return Ok(candidate.program);
            }
            Err(e) if e.kind() == ErrorKind::NotFound => tried.push(candidate.program),
            Err(e) => return Err(format!("{}: {}", candidate.program, e)),
        }
    }
    Err(format!(
        "No terminal found (tried {}). Set $TERMINAL to your terminal.",
        tried.join(", ")
    ))
}

#[tauri::command]
pub fn open_terminal_here(path: String, app_handle: AppHandle) -> Result<String, String> {
    let dir = Path::new(&path);
    if !dir.is_absolute() || !dir.is_dir() {
        return Err(format!("Not a folder: {path}"));
    }
    let version = app_handle.package_info().version.to_string();
    let dir_str = dir.to_string_lossy();

    if std::env::var_os("FLATPAK_ID").is_some() {
        return Err(
            "The Flatpak sandbox cannot open a terminal on the host. Open one in this folder yourself."
                .into(),
        );
    }

    let candidates = if cfg!(target_os = "linux") {
        linux_candidates(&dir_str, std::env::var("TERMINAL").ok().as_deref())
    } else if cfg!(target_os = "macos") {
        vec![launch("open", &["-a", "Terminal", &dir_str])]
    } else if cfg!(target_os = "windows") {
        vec![
            launch("wt.exe", &["-d", &dir_str]),
            launch("cmd.exe", &["/C", "start", "cmd.exe"]),
        ]
    } else {
        return Err("Opening a terminal is not supported on this platform.".into());
    };

    spawn_first(candidates, dir, &version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn programs(candidates: &[Launch]) -> Vec<&str> {
        candidates.iter().map(|c| c.program.as_str()).collect()
    }

    #[test]
    fn terminal_env_comes_first_with_its_directory_flag() {
        let candidates = linux_candidates("/photos/My Trip", Some("/usr/bin/ghostty"));
        assert_eq!(
            candidates[0],
            launch("/usr/bin/ghostty", &["--working-directory=/photos/My Trip"])
        );
        assert_eq!(candidates[1].program, "xdg-terminal-exec");
    }

    #[test]
    fn unknown_terminal_env_is_launched_bare_in_the_folder() {
        let candidates = linux_candidates("/photos", Some("my-term"));
        assert_eq!(candidates[0], launch("my-term", &[]));
    }

    #[test]
    fn blank_terminal_env_is_ignored() {
        let candidates = linux_candidates("/photos", Some("  "));
        assert_eq!(programs(&candidates)[0], "xdg-terminal-exec");
        assert_eq!(candidates.len(), LINUX_TERMINALS.len());
    }

    #[test]
    fn directory_flags_keep_the_path_as_one_argument() {
        let dir = "/photos/a b";
        assert_eq!(
            known_terminal("kitty", dir),
            Some(launch("kitty", &["--directory", dir]))
        );
        assert_eq!(
            known_terminal("wezterm", dir),
            Some(launch("wezterm", &["start", "--cwd", dir]))
        );
        assert_eq!(
            known_terminal("konsole", dir),
            Some(launch("konsole", &["--workdir", dir]))
        );
        assert_eq!(known_terminal("bash", dir), None);
    }

    #[test]
    fn appimage_paths_are_removed_from_the_terminal_environment() {
        let env: HashMap<&str, &str> = HashMap::from([
            ("APPDIR", "/tmp/.mount_RapidXY"),
            ("APPIMAGE", "/home/me/RapidRoom.AppImage"),
            ("LD_LIBRARY_PATH", "/tmp/.mount_RapidXY/usr/lib"),
            (
                "PATH",
                "/tmp/.mount_RapidXY/usr/bin:/usr/local/bin:/usr/bin",
            ),
            (
                "XDG_DATA_DIRS",
                "/tmp/.mount_RapidXY/usr/share:/usr/local/share:/usr/share",
            ),
            ("XDG_CONFIG_DIRS", "/etc/xdg"),
        ]);
        let (remove, rewrite) = appimage_env_fixes(|v| env.get(v).map(|s| s.to_string()));
        assert!(remove.contains(&"LD_LIBRARY_PATH".to_string()));
        assert!(remove.contains(&"APPDIR".to_string()));
        assert!(!remove.contains(&"GTK_PATH".to_string()));
        assert_eq!(
            rewrite,
            vec![
                ("PATH".to_string(), "/usr/local/bin:/usr/bin".to_string()),
                (
                    "XDG_DATA_DIRS".to_string(),
                    "/usr/local/share:/usr/share".to_string()
                ),
            ]
        );
    }

    #[test]
    fn environment_is_untouched_outside_an_appimage() {
        let (remove, rewrite) =
            appimage_env_fixes(|v| (v == "LD_LIBRARY_PATH").then(|| "/opt/lib".into()));
        assert!(remove.is_empty());
        assert!(rewrite.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn first_installed_terminal_starts_in_the_folder_with_rapidroom_variables() {
        let dir = std::env::temp_dir().join(format!("rapidroom-terminal-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let out = dir.join("out.txt");
        let script = format!(
            "printf '%s|%s|%s' \"$(pwd)\" \"$RAPIDROOM_VERSION\" \"$RAPIDROOM_FOLDER\" > '{}'",
            out.display()
        );
        let candidates = vec![
            launch("rapidroom-no-such-terminal", &[]),
            launch("/bin/sh", &["-c", &script]),
        ];

        assert_eq!(spawn_first(candidates, &dir, "9.9.9").unwrap(), "/bin/sh");
        let mut written = String::new();
        for _ in 0..100 {
            written = std::fs::read_to_string(&out).unwrap_or_default();
            if !written.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let expected = format!("{0}|9.9.9|{0}", dir.display());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(written, expected);
    }

    #[test]
    fn missing_terminals_give_a_hint() {
        let err = spawn_first(
            vec![launch("rapidroom-no-such-terminal", &[])],
            Path::new("."),
            "1",
        )
        .unwrap_err();
        assert!(err.contains("rapidroom-no-such-terminal"));
        assert!(err.contains("$TERMINAL"));
    }
}
