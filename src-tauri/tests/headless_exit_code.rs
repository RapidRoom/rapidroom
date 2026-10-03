// Runs the real binary in headless export mode. Both cases fail before any
// image is decoded, so no GPU is needed; Linux still needs a display (xvfb-run).
// Always pass valid `export` arguments: unknown arguments open the GUI.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(120);

fn run_export(home: &Path, source: &Path, output: &Path) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_RapidRAW"))
        .arg("export")
        .arg(source)
        .arg("--output")
        .arg(output)
        .args(["--format", "jpeg"])
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("APPDATA", home.join("appdata"))
        .env("LOCALAPPDATA", home.join("localappdata"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start RapidRAW");

    let stdout = drain(child.stdout.take().unwrap());
    let stderr = drain(child.stderr.take().unwrap());
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("failed to poll RapidRAW") {
            break status;
        }
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            panic!("RapidRAW did not exit within {:?}", TIMEOUT);
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    Output {
        status,
        stdout: stdout.join().unwrap(),
        stderr: stderr.join().unwrap(),
    }
}

fn drain(mut pipe: impl Read + Send + 'static) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf);
        buf
    })
}

fn display_available() -> bool {
    if !cfg!(target_os = "linux") {
        return true;
    }
    let available =
        std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some();
    if !available {
        eprintln!("Skipping: no display. Run under xvfb-run.");
    }
    available
}

fn assert_failed(output: &Output, expected: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "export should fail, got {:?}\nstderr:\n{}",
        output.status,
        stderr
    );
    assert_eq!(output.status.code(), Some(1), "stderr:\n{}", stderr);
    assert!(
        stderr.contains("Headless export failed") && stderr.contains(expected),
        "stderr should explain the failure ({expected:?}), got:\n{}",
        stderr
    );
}

#[test]
fn missing_input_exits_non_zero() {
    if !display_available() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let output = run_export(
        dir.path(),
        &dir.path().join("missing.ARW"),
        &dir.path().join("out.jpg"),
    );
    assert_failed(&output, "Source path does not exist");
}

#[test]
fn unwritable_output_exits_non_zero() {
    if !display_available() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("input.ARW");
    std::fs::write(&source, b"not a raw file").unwrap();
    // A regular file can't be a parent directory, on every platform.
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"").unwrap();
    let output = run_export(dir.path(), &source, &blocker.join("sub").join("out.jpg"));
    assert_failed(&output, "Failed to create output parent directory");
}
