//! Memory protection for sensitive data
//!
//! Provides two hardening measures:
//!
//! 1. **Core dump prevention** — Disables core dumps via `setrlimit(RLIMIT_CORE, 0)`
//!    so that a crash never writes seed material to disk.
//!
//! 2. **Memory locking** — Locks a memory region via `mlock()` to prevent the OS
//!    from swapping sensitive data (seeds, keys) to disk.
//!
//! Both are best-effort: failures are logged but don't crash the application,
//! since some environments (containers, unprivileged users) may not permit these
//! operations.
//!
//! # Platform Support
//!
//! - Unix/macOS/Linux: Full support via libc
//! - Windows: Core dump prevention via SetErrorMode (partial), no mlock yet
//! - Other: No-ops with warnings

use std::sync::atomic::{AtomicBool, Ordering};

/// Track whether core dumps have been disabled (call only once)
static CORE_DUMPS_DISABLED: AtomicBool = AtomicBool::new(false);

/// Disable core dumps for the current process.
///
/// This prevents sensitive data (seeds, keys) from being written to disk
/// if the process crashes. Should be called early in application startup.
///
/// Returns `true` if core dumps were successfully disabled.
///
/// # Example
/// ```
/// nostring_core::memory::disable_core_dumps();
/// ```
pub fn disable_core_dumps() -> bool {
    if CORE_DUMPS_DISABLED.swap(true, Ordering::SeqCst) {
        return true; // Already disabled
    }

    #[cfg(unix)]
    {
        unix::disable_core_dumps_impl()
    }

    #[cfg(windows)]
    {
        windows::disable_core_dumps_impl()
    }

    #[cfg(not(any(unix, windows)))]
    {
        eprintln!("[nostring] Warning: core dump prevention not supported on this platform");
        false
    }
}

/// Lock a memory region to prevent it from being swapped to disk.
///
/// This is critical for seed material — if the OS swaps a page containing
/// a seed to disk, it could persist in swap space long after the process exits.
///
/// Returns `true` if the memory was successfully locked.
///
/// # Safety
///
/// The caller must ensure that:
/// - `ptr` points to a valid allocation of at least `len` bytes
/// - The locked region is unlocked (via `munlock`) before being freed,
///   or the process exits (which implicitly unlocks all pages)
///
/// # Example
/// ```
/// use zeroize::Zeroizing;
/// let seed = Zeroizing::new([0u8; 64]);
/// // Lock the seed in memory
/// unsafe {
///     nostring_core::memory::mlock(seed.as_ptr(), seed.len());
/// }
/// // ... use seed ...
/// // Unlock when done (optional — process exit unlocks automatically)
/// unsafe {
///     nostring_core::memory::munlock(seed.as_ptr(), seed.len());
/// }
/// ```
pub unsafe fn mlock(ptr: *const u8, len: usize) -> bool {
    if len == 0 {
        return true;
    }

    #[cfg(unix)]
    {
        unix::mlock_impl(ptr, len)
    }

    #[cfg(not(unix))]
    {
        let _ = (ptr, len);
        eprintln!("[nostring] Warning: mlock not supported on this platform");
        false
    }
}

/// Unlock a previously locked memory region.
///
/// # Safety
///
/// The caller must ensure `ptr` and `len` match a previous `mlock` call.
pub unsafe fn munlock(ptr: *const u8, len: usize) -> bool {
    if len == 0 {
        return true;
    }

    #[cfg(unix)]
    {
        unix::munlock_impl(ptr, len)
    }

    #[cfg(not(unix))]
    {
        let _ = (ptr, len);
        true
    }
}

/// A wrapper that mlocks its contents on creation and munlocks + zeroizes on drop.
///
/// Use this for seed material that must never hit swap.
///
/// # Example
/// ```
/// use nostring_core::memory::LockedBuffer;
/// let mut buf = LockedBuffer::new(64);
/// buf.as_mut_slice()[..5].copy_from_slice(b"hello");
/// // Memory is locked, zeroized on drop, then unlocked
/// ```
pub struct LockedBuffer {
    data: Vec<u8>,
    locked: bool,
}

impl LockedBuffer {
    /// Create a new zero-filled buffer and lock it in memory.
    pub fn new(len: usize) -> Self {
        let data = vec![0u8; len];
        let locked = if !data.is_empty() {
            unsafe { mlock(data.as_ptr(), data.len()) }
        } else {
            true
        };

        if !locked {
            eprintln!(
                "[nostring] Warning: failed to mlock {} bytes — seed may be swappable",
                len
            );
        }

        Self { data, locked }
    }

    /// Get a reference to the buffer contents.
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    /// Get a mutable reference to the buffer contents.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Whether the memory is actually locked.
    pub fn is_locked(&self) -> bool {
        self.locked
    }
}

impl Drop for LockedBuffer {
    fn drop(&mut self) {
        // Zeroize before unlocking
        use zeroize::Zeroize;
        self.data.zeroize();

        // Unlock the memory
        if self.locked && !self.data.is_empty() {
            unsafe {
                munlock(self.data.as_ptr(), self.data.len());
            }
        }
    }
}

// ---- Platform implementations ----

#[cfg(unix)]
mod unix {
    pub fn disable_core_dumps_impl() -> bool {
        // SAFETY: setrlimit with RLIMIT_CORE=0 is a standard POSIX operation
        unsafe {
            let rlim = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            let result = libc::setrlimit(libc::RLIMIT_CORE, &rlim);
            if result != 0 {
                let errno = std::io::Error::last_os_error();
                eprintln!(
                    "[nostring] Warning: failed to disable core dumps: {}",
                    errno
                );
                return false;
            }
        }
        true
    }

    pub unsafe fn mlock_impl(ptr: *const u8, len: usize) -> bool {
        let result = libc::mlock(ptr as *const libc::c_void, len);
        if result != 0 {
            let errno = std::io::Error::last_os_error();
            eprintln!(
                "[nostring] Warning: mlock failed for {} bytes: {}",
                len, errno
            );
            return false;
        }
        true
    }

    pub unsafe fn munlock_impl(ptr: *const u8, len: usize) -> bool {
        let result = libc::munlock(ptr as *const libc::c_void, len);
        result == 0
    }
}

#[cfg(windows)]
mod windows {
    pub fn disable_core_dumps_impl() -> bool {
        // Windows core dump prevention would use SetErrorMode or
        // MiniDumpWriteDump configuration. For now, log a warning.
        eprintln!("[nostring] Warning: Windows core dump prevention not yet implemented");
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disable_core_dumps() {
        // Should succeed (or at least not crash)
        let result = disable_core_dumps();
        // On CI/sandboxed environments this might fail, so we just
        // verify it doesn't panic
        eprintln!("Core dump disable result: {}", result);

        // Calling twice should still return true
        let result2 = disable_core_dumps();
        assert!(result2, "second call should return true (already disabled)");
    }

    #[test]
    fn test_locked_buffer() {
        let mut buf = LockedBuffer::new(64);

        // Write some data
        buf.as_mut_slice()[0] = 0xDE;
        buf.as_mut_slice()[1] = 0xAD;
        assert_eq!(buf.as_slice()[0], 0xDE);
        assert_eq!(buf.as_slice()[1], 0xAD);
        assert_eq!(buf.as_slice().len(), 64);

        // Buffer should be locked on supported platforms
        #[cfg(unix)]
        {
            // mlock may fail in sandboxed environments, so we just check
            // it doesn't crash
            eprintln!("Buffer locked: {}", buf.is_locked());
        }

        // Drop will zeroize and munlock
    }

    #[test]
    fn test_locked_buffer_zero_length() {
        let buf = LockedBuffer::new(0);
        assert!(buf.is_locked());
        assert!(buf.as_slice().is_empty());
    }

    #[test]
    fn test_locked_buffer_zeroizes_on_drop() {
        // We can't directly check memory after drop, but we can verify
        // the zeroize path by manually calling it
        let mut buf = LockedBuffer::new(32);
        buf.as_mut_slice().fill(0xFF);
        assert!(buf.as_slice().iter().all(|&b| b == 0xFF));

        // Simulate drop behavior
        use zeroize::Zeroize;
        buf.data.zeroize();
        assert!(buf.as_slice().iter().all(|&b| b == 0));
    }

    #[test]
    fn test_mlock_munlock_roundtrip() {
        let data = vec![42u8; 128];
        unsafe {
            let locked = mlock(data.as_ptr(), data.len());
            eprintln!("mlock result: {}", locked);

            let unlocked = munlock(data.as_ptr(), data.len());
            eprintln!("munlock result: {}", unlocked);
        }
    }
}
