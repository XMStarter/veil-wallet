//! Shared helpers for tests that need the real wallet library.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use veil_wallet::WalletLibrary;

/// Loads the wallet library named by `VEIL_WALLET_LIB`.
///
/// Without that variable the calling test is skipped, so that `cargo test`
/// works on a machine that has not built the library. Setting
/// `VEIL_REQUIRE_WALLET_LIB=1` turns a missing library into a failure instead
/// of a silent skip.
pub fn wallet_library() -> Option<WalletLibrary> {
    match std::env::var_os("VEIL_WALLET_LIB") {
        Some(path) => Some(WalletLibrary::load(path).expect("wallet library loads")),
        None => {
            assert!(
                std::env::var_os("VEIL_REQUIRE_WALLET_LIB").is_none(),
                "VEIL_WALLET_LIB is not set but the wallet library is required"
            );
            eprintln!("skipped: set VEIL_WALLET_LIB to the wallet library to run this test");
            None
        }
    }
}

/// A fresh directory for one test's wallet files, removed when dropped.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(name: &str) -> TempDir {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let unique = NEXT.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "veil-wallet-{name}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        TempDir(dir)
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Where the wallet library would make its "shared ring database" for the
/// user who runs the tests, if nothing kept it from doing so: next to the
/// user's home on Linux, and on Windows in the folder all users of the
/// machine share. A test cannot move the Windows one out of the way.
pub fn shared_ring_database() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("ProgramData").unwrap_or_else(|| r"C:\ProgramData".into())
    } else {
        std::env::var_os("HOME").unwrap_or_else(|| "/".into())
    };
    PathBuf::from(base).join(".shared-ringdb")
}

/// Everything under `dir`, folders included: the path from `dir` on, the
/// size, and when it was last written. Sorted; empty for a folder that is
/// not there.
pub fn everything_under(
    dir: &std::path::Path,
) -> Vec<(String, u64, Option<std::time::SystemTime>)> {
    fn walk(
        dir: &std::path::Path,
        prefix: &str,
        found: &mut Vec<(String, u64, Option<std::time::SystemTime>)>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
            let (size, written, is_dir) = match entry.metadata() {
                Ok(data) => (data.len(), data.modified().ok(), data.is_dir()),
                Err(_) => (0, None, false),
            };
            if is_dir {
                walk(&entry.path(), &format!("{name}/"), found);
                // A folder's own size says nothing.
                found.push((name, 0, written));
            } else {
                found.push((name, size, written));
            }
        }
    }
    let mut found = Vec::new();
    walk(dir, "", &mut found);
    found.sort();
    found
}

/// The names of [`everything_under`] alone.
pub fn names_under(dir: &std::path::Path) -> Vec<String> {
    everything_under(dir)
        .into_iter()
        .map(|(name, _, _)| name)
        .collect()
}

/// What lies where the library's shared ring database would be, looked at
/// before a test so that it can be compared afterwards.
pub struct RingDatabaseBefore {
    place: PathBuf,
    before: Vec<(String, u64, Option<std::time::SystemTime>)>,
    was_there: bool,
}

impl RingDatabaseBefore {
    pub fn look() -> RingDatabaseBefore {
        let place = shared_ring_database();
        RingDatabaseBefore {
            was_there: place.exists(),
            before: everything_under(&place),
            place,
        }
    }

    /// Fails if the wallets used since [`RingDatabaseBefore::look`] made
    /// that database or wrote to it.
    ///
    /// On a machine where it was never made this is certain: it must still
    /// not be there. Where an older library, or another Monero wallet, has
    /// left one, all that can be seen is that no file in it has changed in
    /// name, size or time of writing; which of the two it was is printed.
    pub fn assert_untouched(self) {
        let place = self.place.display();
        if self.was_there {
            println!(
                "{place} was there before this test (left by an older wallet library or by \
                 another Monero wallet), so the test can only see that it was not written to"
            );
        } else {
            assert!(
                !self.place.exists(),
                "the wallet library made {place}, outside the wallet's folder"
            );
        }
        assert_eq!(
            everything_under(&self.place),
            self.before,
            "the wallet library wrote to {place}, outside the wallet's folder"
        );
    }
}
