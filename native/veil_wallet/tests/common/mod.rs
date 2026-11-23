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
