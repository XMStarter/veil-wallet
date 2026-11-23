//! Running the `veil` program from tests.

#![allow(dead_code)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

/// True when the wallet library is available. Without it the calling test
/// is skipped, unless `VEIL_REQUIRE_WALLET_LIB` is set, which makes that a
/// failure.
pub fn have_wallet_library() -> bool {
    if std::env::var_os("VEIL_WALLET_LIB").is_some() {
        return true;
    }
    assert!(
        std::env::var_os("VEIL_REQUIRE_WALLET_LIB").is_none(),
        "VEIL_WALLET_LIB is not set but the wallet library is required"
    );
    eprintln!("skipped: set VEIL_WALLET_LIB to the wallet library to run this test");
    false
}

/// A fresh directory, removed when dropped.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(name: &str) -> TempDir {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let unique = NEXT.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("veil-cli-{name}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        TempDir(dir)
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What a run of `veil` printed and how it ended.
pub struct Run {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }

    /// The lines of standard output, without surrounding spaces.
    pub fn lines(&self) -> Vec<&str> {
        self.stdout.lines().map(str::trim).collect()
    }
}

/// The `veil` program built alongside this test.
///
/// Found from where the test itself is, not from the path the build knew:
/// a test built on Linux for Windows runs on another machine's view of the
/// file system, where the build's path means nothing.
fn veil_program() -> PathBuf {
    let test = std::env::current_exe().expect("the test program has a path");
    // target/<profile>/deps/<test> is next to target/<profile>/veil.
    let profile_dir = test
        .parent()
        .and_then(Path::parent)
        .expect("the test program is in target/<profile>/deps");
    profile_dir.join(format!("veil{}", std::env::consts::EXE_SUFFIX))
}

/// Runs `veil --data-dir <data_dir> <arguments>`, answering its questions
/// with `answers`, one per line.
pub fn veil(data_dir: &Path, arguments: &[&str], answers: &[&str]) -> Run {
    let mut child = Command::new(veil_program())
        .arg("--data-dir")
        .arg(data_dir)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("veil starts");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        for answer in answers {
            // The program may exit before reading everything.
            let _ = writeln!(stdin, "{answer}");
        }
    }
    let Output {
        status,
        stdout,
        stderr,
    } = child.wait_with_output().expect("veil finishes");
    Run {
        code: status.code(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    }
}

/// The recovery phrase among the lines `veil create` printed.
pub fn phrase_in(run: &Run) -> String {
    run.lines()
        .into_iter()
        .find(|line| line.split(' ').count() == 16)
        .expect("a sixteen-word phrase")
        .to_owned()
}

/// The stagenet address among the lines a command printed.
pub fn address_in(run: &Run) -> String {
    run.lines()
        .into_iter()
        .find(|line| line.len() == 95 && line.starts_with('7'))
        .unwrap_or_else(|| panic!("no stagenet subaddress in: {}", run.stdout))
        .to_owned()
}
