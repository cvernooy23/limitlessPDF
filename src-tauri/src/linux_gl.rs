//! Linux WebKitGTK GPU-compatibility shim.
//!
//! On some Linux systems — VMs without 3D acceleration, minimal installs
//! missing Mesa, or headless / software-rendering setups — WebKitGTK's
//! hardware-accelerated renderer cannot create an EGL display and aborts the
//! entire process at webview-creation time with:
//!
//! ```text
//! Could not create default EGL display: EGL_BAD_PARAMETER. Aborting...
//! ```
//!
//! That abort happens deep inside WebKitGTK and cannot be caught after the
//! fact. Instead we run a lightweight EGL probe at startup and, if EGL is
//! unusable, switch WebKitGTK onto its software-rendering path *before* the
//! webview is built by exporting the relevant environment variables. Healthy
//! systems are left completely untouched, and an explicit override from the
//! user or their environment always wins.

use std::os::raw::{c_char, c_int, c_void};

type EglGetDisplay = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type EglInitialize = unsafe extern "C" fn(*mut c_void, *mut c_int, *mut c_int) -> u32;
type EglTerminate = unsafe extern "C" fn(*mut c_void) -> u32;

/// If EGL can't stand up a display on this machine, force WebKitGTK onto its
/// software-rendering path so the app degrades to a working (if slower) window
/// instead of aborting. Best-effort: any uncertainty leaves the system as-is.
pub fn ensure_webkit_compat() {
    // Respect an explicit choice already present in the environment.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some()
        || std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_some()
    {
        return;
    }

    if egl_display_works() {
        return;
    }

    // No usable EGL. These are read by WebKitGTK / Mesa when the webview spins
    // up its GPU process, which happens after this runs, so setting them
    // in-process here is early enough — no relaunch required.
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
    eprintln!(
        "limitlessPDF: no usable EGL display detected; enabling WebKitGTK \
         software-rendering fallback (WEBKIT_DISABLE_DMABUF_RENDERER=1)."
    );
}

/// Try to obtain and initialize the default EGL display, mirroring the path
/// WebKitGTK takes. Returns `false` if libEGL is missing or the display can't
/// be initialized — the exact condition that makes WebKitGTK abort.
fn egl_display_works() -> bool {
    unsafe {
        let handle = dlopen_first(&[b"libEGL.so.1\0", b"libEGL.so\0"]);
        if handle.is_null() {
            // No EGL library at all: WebKitGTK would fail the same way.
            return false;
        }

        let get_display: EglGetDisplay = match dlsym_fn(handle, b"eglGetDisplay\0") {
            Some(f) => std::mem::transmute::<*mut c_void, EglGetDisplay>(f),
            None => {
                libc::dlclose(handle);
                return false;
            }
        };
        let initialize: EglInitialize = match dlsym_fn(handle, b"eglInitialize\0") {
            Some(f) => std::mem::transmute::<*mut c_void, EglInitialize>(f),
            None => {
                libc::dlclose(handle);
                return false;
            }
        };
        let terminate: Option<EglTerminate> = dlsym_fn(handle, b"eglTerminate\0")
            .map(|f| std::mem::transmute::<*mut c_void, EglTerminate>(f));

        // EGL_DEFAULT_DISPLAY is a null native-display handle.
        let display = get_display(std::ptr::null_mut());
        if display.is_null() {
            libc::dlclose(handle);
            return false;
        }

        let mut major: c_int = 0;
        let mut minor: c_int = 0;
        let ok = initialize(display, &mut major, &mut minor) != 0;

        if ok {
            if let Some(term) = terminate {
                term(display);
            }
        }

        libc::dlclose(handle);
        ok
    }
}

/// dlopen the first of several candidate library names that succeeds.
unsafe fn dlopen_first(names: &[&[u8]]) -> *mut c_void {
    for name in names {
        let handle = libc::dlopen(name.as_ptr() as *const c_char, libc::RTLD_NOW);
        if !handle.is_null() {
            return handle;
        }
    }
    std::ptr::null_mut()
}

/// dlsym a symbol, returning `None` when it is not present.
unsafe fn dlsym_fn(handle: *mut c_void, name: &[u8]) -> Option<*mut c_void> {
    let sym = libc::dlsym(handle, name.as_ptr() as *const c_char);
    if sym.is_null() {
        None
    } else {
        Some(sym)
    }
}
