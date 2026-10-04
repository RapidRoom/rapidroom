// Runs the real binary in headless export and bench mode. Every case fails before
// any image is decoded, so no GPU is needed; Linux still needs a display (xvfb-run).
// Always pass valid `export` arguments: unknown arguments open the GUI.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(120);

fn run_export(home: &Path, source: &Path, output: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_RapidRAW"));
    command
        .arg("export")
        .arg(source)
        .arg("--output")
        .arg(output)
        .args(["--format", "jpeg"]);
    run(command, home, false)
}

fn run_bench(home: &Path, args: &[&std::ffi::OsStr], close_stdout: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_RapidRAW"));
    command.arg("bench").args(args);
    run(command, home, close_stdout)
}

// With `close_stdout`, the read end of stdout is closed at once, like `| head`.
fn run(mut command: Command, home: &Path, close_stdout: bool) -> Output {
    let mut child = command
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

    let stdout = child.stdout.take().unwrap();
    let stdout = if close_stdout {
        drop(stdout);
        std::thread::spawn(Vec::new)
    } else {
        drain(stdout)
    };
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

#[test]
fn invalid_bench_arguments_exit_with_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let output = run_bench(dir.path(), &["--iters".as_ref()], false);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{}", stderr);
    assert!(
        stderr.contains("Invalid bench arguments"),
        "stderr should explain the failure, got:\n{}",
        stderr
    );
}

#[test]
fn bench_missing_input_exits_non_zero() {
    if !display_available() {
        return;
    }
    for close_stdout in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.ARW");
        let output = run_bench(
            dir.path(),
            &[missing.as_os_str(), "--iters".as_ref(), "1".as_ref()],
            close_stdout,
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(1),
            "close_stdout={close_stdout}\nstderr:\n{}",
            stderr
        );
        assert!(
            stderr.contains("Bench failed"),
            "stderr should explain the failure, got:\n{}",
            stderr
        );
    }
}
