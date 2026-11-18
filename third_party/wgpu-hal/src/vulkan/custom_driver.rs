//! Horizon: optional bundled Vulkan ICD, loaded instead of Android's system
//! `libvulkan.so`. wgpu-hal normally uses `ash::Entry::load()`, which lets the
//! platform loader pick the system driver. A custom PanVK driver is a
//! self-contained ICD: it exports `vk_icdGetInstanceProcAddr` and implements
//! its own Android WSI, so it can be used directly. Every failure leaves the
//! system driver in place.
//!
//! The bundled `.so` is SHA-256 verified by `tools/android_build.py` and by the
//! custom-driver CI workflow before packaging, and is loaded by soname from the
//! app's signed APK. Android extracts no per-ABI file to hash at runtime, so
//! those build-time checks and the APK signature are the trust anchor.

use core::ffi::{c_char, c_void};
use std::sync::OnceLock;

/// Bundled into the APK at `lib/arm64-v8a` by `tools/android_build.py`.
const SONAME: &str = "libvulkan_panfrost-android-aarch64.so";

type GetInstanceProcAddr = unsafe extern "system" fn(*mut c_void, *const c_char) -> *mut c_void;

// Function pointers fetched from an `ash::Entry` stay valid only while the
// library is loaded. Never drop this: the game keeps the driver for its lifetime.
static LIBRARY: OnceLock<libloading::Library> = OnceLock::new();

/// Load and register the bundled driver. Logs and returns `false` on any
/// failure, so the caller continues with the system driver.
pub fn install() -> bool {
    if LIBRARY.get().is_some() {
        return true;
    }
    let library = match unsafe { libloading::Library::new(SONAME) } {
        Ok(library) => library,
        Err(error) => {
            log::warn!("Custom Vulkan driver: cannot load {SONAME} ({error}); using the system driver");
            return false;
        }
    };
    // Mesa's Android app-loader symbol scheme exports the ICD entry point, not
    // plain vkGetInstanceProcAddr. Its presence also identifies the real driver.
    let symbol = unsafe { library.get::<GetInstanceProcAddr>(b"vk_icdGetInstanceProcAddr\0") }
        .or_else(|_| unsafe { library.get::<GetInstanceProcAddr>(b"vkGetInstanceProcAddr\0") });
    let proc_addr = match symbol {
        Ok(symbol) => *symbol as usize as *mut c_void,
        Err(error) => {
            log::warn!("Custom Vulkan driver: missing ICD entry point ({error}); using the system driver");
            return false;
        }
    };
    if LIBRARY.set(library).is_err() {
        return true;
    }
    super::set_custom_get_instance_proc_addr(proc_addr);
    log::info!("Custom Vulkan driver: loaded {SONAME} (hash verified at build time)");
    true
}
