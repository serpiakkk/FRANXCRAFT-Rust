//! Horizon: optional bundled Vulkan ICD, loaded instead of Android's system
//! `libvulkan.so`. wgpu-hal normally uses `ash::Entry::load()`, which lets the
//! platform loader pick the system driver. A custom PanVK driver is a
//! self-contained ICD: it exports `vk_icdGetInstanceProcAddr` and implements
//! its own Android WSI, so it can be used directly. Every failure leaves the
//! system driver in place.

use core::ffi::{c_char, c_void};
use std::{fs, path::PathBuf, string::String, sync::OnceLock};

use sha2::{Digest, Sha256};

/// Bundled into the APK at `lib/arm64-v8a` by `tools/android_build.py`.
const SONAME: &str = "libvulkan_panfrost-android-aarch64.so";
/// SHA-256 of the bundled driver. Replace the placeholder before shipping.
const EXPECTED_SHA256: &str = include_str!("../../../vulkan/driver.sha256");

type GetInstanceProcAddr = unsafe extern "system" fn(*mut c_void, *const c_char) -> *mut c_void;

// Function pointers fetched from an `ash::Entry` stay valid only while the
// library is loaded. Never drop this: the game keeps the driver for its lifetime.
static LIBRARY: OnceLock<libloading::Library> = OnceLock::new();

fn expected_hash() -> &'static str {
    EXPECTED_SHA256
        .split_whitespace()
        .next()
        .unwrap_or_default()
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(64);
    for byte in bytes.as_ref() {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// The mapped path of a loaded library, from `/proc/self/maps`.
fn path_in_maps(maps: &str) -> Option<&str> {
    maps.lines()
        .filter_map(|line| line.split_whitespace().last())
        .find(|path| {
            path.starts_with('/') && path.contains("/lib/arm64-v8a/") && path.ends_with(SONAME)
        })
}

fn library_path() -> Option<PathBuf> {
    path_in_maps(&fs::read_to_string("/proc/self/maps").ok()?).map(PathBuf::from)
}

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
    let path = match library_path() {
        Some(path) => path,
        None => {
            log::warn!("Custom Vulkan driver: cannot locate {SONAME} in /proc/self/maps; using the system driver");
            return false;
        }
    };
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            log::warn!("Custom Vulkan driver: cannot read {} ({error}); using the system driver", path.display());
            return false;
        }
    };
    let digest = hex(Sha256::digest(&bytes));
    if !digest.eq_ignore_ascii_case(expected_hash()) {
        log::warn!(
            "Custom Vulkan driver: hash mismatch for {} (expected {}, got {digest}); using the system driver",
            path.display(),
            expected_hash()
        );
        return false;
    }
    // Mesa's Android app-loader symbol scheme exports the ICD entry point, not
    // plain vkGetInstanceProcAddr.
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
    log::info!("Custom Vulkan driver: {} sha256={digest}", path.display());
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_driver_in_a_proc_maps_excerpt() {
        let maps = "7f000000-7f001000 r--p 00000000 fd:01 99 /system/lib64/libvulkan.so\n\
                    7f002000-7f003000 r-xp 00000000 fd:01 100 /data/app/~~x/lib/arm64-v8a/libvulkan_panfrost-android-aarch64.so\n";
        assert_eq!(
            path_in_maps(maps),
            Some("/data/app/~~x/lib/arm64-v8a/libvulkan_panfrost-android-aarch64.so")
        );
        assert_eq!(path_in_maps("/system/lib64/libvulkan.so\n"), None);
    }

    #[test]
    fn hex_encodes_sha256_digest_bytes() {
        assert_eq!(hex([0u8; 32]), "00".repeat(32));
    }
}
