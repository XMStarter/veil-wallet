//! C interface, called from the app through FFI.
//!
//! Every function is safe to call from any thread. None of them blocks on
//! the network. A null handle is accepted everywhere and treated as "Tor is
//! not running". Panics never cross this boundary.

use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::ptr;

use crate::service::{Purpose, State, TorConfig, TorService};

/// Opaque handle to a running Tor service.
pub struct VeilTor {
    service: TorService,
}

/// `veil_tor_state` result: Tor is connecting.
pub const VEIL_TOR_BOOTSTRAPPING: i32 = State::Bootstrapping as i32;
/// `veil_tor_state` result: Tor can carry traffic.
pub const VEIL_TOR_READY: i32 = State::Ready as i32;
/// `veil_tor_state` result: Tor failed to start, or the handle is null.
pub const VEIL_TOR_FAILED: i32 = State::Failed as i32;

/// `veil_tor_socks_port` purpose: wallet synchronisation.
pub const VEIL_TOR_PURPOSE_SYNC: i32 = Purpose::Sync as i32;
/// `veil_tor_socks_port` purpose: transaction broadcast.
pub const VEIL_TOR_PURPOSE_BROADCAST: i32 = Purpose::Broadcast as i32;

/// Starts Tor. Both arguments are UTF-8 paths to directories private to the
/// app. Returns null on failure. The handle must be released with
/// `veil_tor_stop`.
///
/// # Safety
///
/// Both pointers must be null or point to NUL-terminated strings that stay
/// valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn veil_tor_start(
    state_dir: *const c_char,
    cache_dir: *const c_char,
) -> *mut VeilTor {
    let (Some(state_dir), Some(cache_dir)) = (path_arg(state_dir), path_arg(cache_dir)) else {
        return ptr::null_mut();
    };
    let started = catch_unwind(|| {
        TorService::start(TorConfig {
            state_dir,
            cache_dir,
        })
    });
    match started {
        Ok(Ok(service)) => Box::into_raw(Box::new(VeilTor { service })),
        _ => ptr::null_mut(),
    }
}

/// Returns one of the `VEIL_TOR_*` state constants.
///
/// # Safety
///
/// `handle` must be null or a live handle from `veil_tor_start`.
#[no_mangle]
pub unsafe extern "C" fn veil_tor_state(handle: *const VeilTor) -> i32 {
    with_service(handle, VEIL_TOR_FAILED, |service| {
        service.status().state as i32
    })
}

/// Returns bootstrap progress from 0 to 100.
///
/// # Safety
///
/// `handle` must be null or a live handle from `veil_tor_start`.
#[no_mangle]
pub unsafe extern "C" fn veil_tor_bootstrap_percent(handle: *const VeilTor) -> i32 {
    with_service(handle, 0, |service| i32::from(service.status().percent))
}

/// Returns the loopback port of the SOCKS listener for `purpose`, or 0 if
/// the handle is null or the purpose is unknown.
///
/// # Safety
///
/// `handle` must be null or a live handle from `veil_tor_start`.
#[no_mangle]
pub unsafe extern "C" fn veil_tor_socks_port(handle: *const VeilTor, purpose: i32) -> i32 {
    let purpose = match purpose {
        VEIL_TOR_PURPOSE_SYNC => Purpose::Sync,
        VEIL_TOR_PURPOSE_BROADCAST => Purpose::Broadcast,
        _ => return 0,
    };
    with_service(handle, 0, |service| i32::from(service.socks_port(purpose)))
}

/// Returns a message explaining why Tor is stuck or failed, or null if there
/// is nothing to report. The caller frees it with `veil_tor_string_free`.
///
/// # Safety
///
/// `handle` must be null or a live handle from `veil_tor_start`.
#[no_mangle]
pub unsafe extern "C" fn veil_tor_message(handle: *const VeilTor) -> *mut c_char {
    with_service(handle, ptr::null_mut(), |service| {
        service
            .status()
            .message
            .and_then(|message| CString::new(message.replace('\0', " ")).ok())
            .map_or(ptr::null_mut(), CString::into_raw)
    })
}

/// Frees a string returned by `veil_tor_message`.
///
/// # Safety
///
/// `message` must be null or a pointer returned by `veil_tor_message` that
/// has not been freed yet.
#[no_mangle]
pub unsafe extern "C" fn veil_tor_string_free(message: *mut c_char) {
    if !message.is_null() {
        // SAFETY: the caller guarantees the pointer came from
        // `CString::into_raw` in `veil_tor_message`.
        drop(unsafe { CString::from_raw(message) });
    }
}

/// Stops Tor and releases the handle. The handle must not be used afterwards.
///
/// # Safety
///
/// `handle` must be null or a live handle from `veil_tor_start`, and no other
/// thread may be using it.
#[no_mangle]
pub unsafe extern "C" fn veil_tor_stop(handle: *mut VeilTor) {
    if handle.is_null() {
        return;
    }
    // SAFETY: the caller guarantees the pointer came from `Box::into_raw` in
    // `veil_tor_start` and is not used again.
    let boxed = unsafe { Box::from_raw(handle) };
    let _ = catch_unwind(AssertUnwindSafe(move || boxed.service.stop()));
}

/// Runs `f` on the service behind `handle`, or returns `fallback` for a null
/// handle or a panic.
unsafe fn with_service<T>(
    handle: *const VeilTor,
    fallback: T,
    f: impl FnOnce(&TorService) -> T,
) -> T {
    // SAFETY: the caller guarantees the handle is null or live.
    let Some(tor) = (unsafe { handle.as_ref() }) else {
        return fallback;
    };
    catch_unwind(AssertUnwindSafe(|| f(&tor.service))).unwrap_or(fallback)
}

unsafe fn path_arg(arg: *const c_char) -> Option<PathBuf> {
    if arg.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees a valid NUL-terminated string.
    let text = unsafe { CStr::from_ptr(arg) }.to_str().ok()?;
    (!text.is_empty()).then(|| PathBuf::from(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_handle_reads_as_not_running() {
        unsafe {
            assert_eq!(veil_tor_state(ptr::null()), VEIL_TOR_FAILED);
            assert_eq!(veil_tor_bootstrap_percent(ptr::null()), 0);
            assert_eq!(veil_tor_socks_port(ptr::null(), VEIL_TOR_PURPOSE_SYNC), 0);
            assert!(veil_tor_message(ptr::null()).is_null());
            veil_tor_string_free(ptr::null_mut());
            veil_tor_stop(ptr::null_mut());
        }
    }

    #[test]
    fn start_rejects_missing_paths() {
        let empty = CString::new("").unwrap();
        unsafe {
            assert!(veil_tor_start(ptr::null(), ptr::null()).is_null());
            assert!(veil_tor_start(empty.as_ptr(), empty.as_ptr()).is_null());
        }
    }

    #[test]
    #[ignore = "starts a Tor client, which tries to reach the Tor network"]
    fn start_poll_stop_round_trip() {
        veil_test_guard::refuse_in_hosted_ci("starting a Tor client");
        let root = std::env::temp_dir().join(format!("veil-tor-ffi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let state = CString::new(root.join("state").to_str().unwrap()).unwrap();
        let cache = CString::new(root.join("cache").to_str().unwrap()).unwrap();

        unsafe {
            let handle = veil_tor_start(state.as_ptr(), cache.as_ptr());
            assert!(!handle.is_null());
            assert_ne!(veil_tor_state(handle), VEIL_TOR_READY);
            let sync = veil_tor_socks_port(handle, VEIL_TOR_PURPOSE_SYNC);
            let broadcast = veil_tor_socks_port(handle, VEIL_TOR_PURPOSE_BROADCAST);
            assert!(sync > 0 && broadcast > 0 && sync != broadcast);
            assert_eq!(veil_tor_socks_port(handle, 99), 0);
            veil_tor_string_free(veil_tor_message(handle));
            veil_tor_stop(handle);
        }
        let _ = std::fs::remove_dir_all(root);
    }
}
