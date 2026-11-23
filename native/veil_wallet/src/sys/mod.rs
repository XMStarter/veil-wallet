//! Loading the wallet library and the few helpers every call needs.
//!
//! `bindings.rs` is generated from the library's C header (see
//! `native/wallet_lib/generate-bindings.sh`). This module is the only place
//! that knows about raw pointers and C strings.

use std::ffi::{c_char, c_int, CStr, CString};
use std::path::Path;
use std::sync::Arc;

use zeroize::Zeroizing;

use crate::Error;

#[allow(
    non_snake_case,
    non_camel_case_types,
    dead_code,
    clippy::missing_safety_doc,
    clippy::too_many_arguments
)]
mod bindings;

pub(crate) use bindings::MoneroC;

// Values of the enums in the wallet library's C++ interface. The library
// also exports them as data symbols; `WalletLibrary::load` checks that the
// loaded build agrees with these numbers.
pub(crate) const NETWORK_MAINNET: c_int = 0;
pub(crate) const NETWORK_TESTNET: c_int = 1;
pub(crate) const NETWORK_STAGENET: c_int = 2;
pub(crate) const STATUS_OK: c_int = 0;
pub(crate) const PRIORITY_DEFAULT: c_int = 0;
pub(crate) const DIRECTION_IN: c_int = 0;
pub(crate) const DIRECTION_OUT: c_int = 1;
pub(crate) const CONNECTION_DISCONNECTED: c_int = 0;
pub(crate) const CONNECTION_CONNECTED: c_int = 1;

const EXPECTED_CONSTANTS: &[(&[u8], c_int)] = &[
    (b"MONERO_NetworkType_MAINNET\0", NETWORK_MAINNET),
    (b"MONERO_NetworkType_TESTNET\0", NETWORK_TESTNET),
    (b"MONERO_NetworkType_STAGENET\0", NETWORK_STAGENET),
    (b"MONERO_WalletStatus_Ok\0", STATUS_OK),
    (b"MONERO_PendingTransactionStatus_Ok\0", STATUS_OK),
    (b"MONERO_Priority_Default\0", PRIORITY_DEFAULT),
    (b"MONERO_TransactionInfoDirection_In\0", DIRECTION_IN),
    (b"MONERO_TransactionInfoDirection_Out\0", DIRECTION_OUT),
    (
        b"MONERO_WalletConnectionStatus_Disconnected\0",
        CONNECTION_DISCONNECTED,
    ),
    (
        b"MONERO_WalletConnectionStatus_Connected\0",
        CONNECTION_CONNECTED,
    ),
];

/// An error's message followed by the messages of what caused it.
///
/// The loader's own message on Windows is only the name of the call that
/// failed. The reason, such as "access is denied" from an antivirus or "a
/// DLL initialization routine failed", is in the cause.
fn with_causes(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut cause = error.source();
    while let Some(error) = cause {
        text.push_str(": ");
        text.push_str(&error.to_string());
        cause = error.source();
    }
    text
}

/// Loads the shared library at `path`.
///
/// # Safety
///
/// Loading a library runs its initialisers.
#[cfg(not(windows))]
unsafe fn open_library(path: &Path) -> Result<libloading::Library, libloading::Error> {
    libloading::Library::new(path)
}

/// Loads the shared library at `path`.
///
/// Windows looks for the other libraries a library needs in a list of
/// places that by default includes the current folder and everything on
/// `PATH`. A file with the right name dropped into any of them would be
/// loaded into the wallet. Here only the wallet library's own folder and
/// the system folder are searched.
///
/// # Safety
///
/// Loading a library runs its initialisers.
#[cfg(windows)]
unsafe fn open_library(path: &Path) -> Result<libloading::Library, libloading::Error> {
    use libloading::os::windows::{
        Library, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
    };
    // The restricted search needs a full path. If the path cannot be made
    // absolute, the load below fails on it and reports that.
    let full = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    Library::load_with_flags(
        full,
        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
    )
    .map(Into::into)
}

/// The loaded wallet library. Cheap to clone. Once loaded, the library stays
/// loaded until the process ends.
#[derive(Clone)]
pub struct WalletLibrary {
    pub(crate) api: Arc<MoneroC>,
}

impl WalletLibrary {
    /// Loads the wallet library from `path`.
    ///
    /// Fails if the file is missing, is not the library, lacks a function
    /// Veil calls, or disagrees with Veil about the meaning of a constant.
    pub fn load(path: impl AsRef<Path>) -> Result<WalletLibrary, Error> {
        let path = path.as_ref();
        let fail = |reason: String| Error::Library {
            path: path.to_path_buf(),
            reason,
        };

        // SAFETY: loading a shared library runs its initialisers. The path is
        // chosen by the app and points at the library it ships.
        let library = unsafe { open_library(path) }.map_err(|e| fail(with_causes(&e)))?;

        for (symbol, expected) in EXPECTED_CONSTANTS {
            // SAFETY: each symbol is declared in the library's header as
            // `const int`, so reading one `c_int` through it is valid.
            let actual = unsafe {
                let value = library
                    .get::<*const c_int>(symbol)
                    .map_err(|e| fail(e.to_string()))?;
                **value
            };
            if actual != *expected {
                let name = String::from_utf8_lossy(&symbol[..symbol.len() - 1]);
                return Err(fail(format!(
                    "{name} is {actual}, expected {expected}; wrong library version"
                )));
            }
        }

        // SAFETY: `from_library` only looks up symbols; their signatures come
        // from the header the bindings were generated from.
        let api = unsafe { MoneroC::from_library(library) }.map_err(|e| fail(e.to_string()))?;
        // The library logs to stdout by default. Logs can contain addresses
        // and amounts, so they are turned off.
        // SAFETY: plain call with a valid level.
        unsafe { api.MONERO_WalletManagerFactory_setLogLevel(-1) };
        let api = Arc::new(api);
        // The library is never unloaded. Once a wallet has been opened it
        // owns threads that outlive the wallet, and its clean-up joins them.
        // Unloading runs that clean-up while the system holds the lock a
        // thread needs in order to finish, so on Windows the two wait for
        // each other. Keeping one reference for ever leaves the clean-up to
        // the end of the process, where the threads are gone already.
        std::mem::forget(Arc::clone(&api));
        Ok(WalletLibrary { api })
    }

    /// Takes ownership of a string returned by the library and frees it.
    /// A null pointer becomes an empty string.
    ///
    /// # Safety
    ///
    /// `text` must be null or a NUL-terminated string allocated by the
    /// library that has not been freed.
    pub(crate) unsafe fn take_string(&self, text: *const c_char) -> String {
        self.take_secret(text).to_string()
    }

    /// Like [`take_string`](Self::take_string), for text that must be wiped
    /// from memory after use. The library's own copy is overwritten before
    /// it is freed.
    ///
    /// # Safety
    ///
    /// Same as [`take_string`](Self::take_string).
    pub(crate) unsafe fn take_secret(&self, text: *const c_char) -> Zeroizing<String> {
        if text.is_null() {
            return Zeroizing::new(String::new());
        }
        // SAFETY: the caller guarantees a valid NUL-terminated string.
        let bytes = unsafe { CStr::from_ptr(text) }.to_bytes();
        if bytes.is_empty() {
            // For an empty list the library returns a static "" instead of
            // an allocation, and freeing that aborts the process. An empty
            // string is therefore never freed; at worst one byte leaks.
            return Zeroizing::new(String::new());
        }
        let owned = Zeroizing::new(String::from_utf8_lossy(bytes).into_owned());
        // SAFETY: the library handed over ownership of this allocation, which
        // is `bytes.len()` bytes plus the terminator. It is wiped and then
        // released with the library's own allocator.
        unsafe {
            std::ptr::write_bytes(text as *mut u8, 0, bytes.len());
            self.api.MONERO_free(text as *mut _);
        }
        owned
    }
}

/// A C string that is wiped when dropped. Used for passwords and phrases.
pub(crate) struct SecretCString(Option<CString>);

impl SecretCString {
    pub(crate) fn new(text: &str, what: &'static str) -> Result<Self, Error> {
        CString::new(text)
            .map(|c| SecretCString(Some(c)))
            .map_err(|_| Error::InvalidArgument { what })
    }

    pub(crate) fn as_ptr(&self) -> *const c_char {
        self.0.as_ref().map_or(std::ptr::null(), |c| c.as_ptr())
    }
}

impl Drop for SecretCString {
    fn drop(&mut self) {
        if let Some(text) = self.0.take() {
            let mut bytes = text.into_bytes();
            zeroize::Zeroize::zeroize(&mut bytes);
        }
    }
}

/// A C string for text that is not secret.
pub(crate) fn c_string(text: &str, what: &'static str) -> Result<CString, Error> {
    CString::new(text).map_err(|_| Error::InvalidArgument { what })
}
