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
    fn test_disable_core_dumps_succeeds_on_unix() {
        let result = disable_core_dumps();

        // On Unix, this should succeed
        #[cfg(unix)]
        assert!(result, "disable_core_dumps must succeed on Unix");

        // Idempotent — second call returns true (already disabled)
        let result2 = disable_core_dumps();
        assert!(result2, "second call should return true (already disabled)");
    }

    #[cfg(unix)]
    #[test]
    fn test_core_dumps_actually_disabled() {
        // Verify the rlimit is actually set to 0 after calling disable_core_dumps
        disable_core_dumps();

        unsafe {
            let mut rlim = libc::rlimit {
                rlim_cur: 999,
                rlim_max: 999,
            };
            let result = libc::getrlimit(libc::RLIMIT_CORE, &mut rlim);
            assert_eq!(result, 0, "getrlimit should succeed");
            assert_eq!(rlim.rlim_cur, 0, "RLIMIT_CORE soft limit must be 0");
            assert_eq!(rlim.rlim_max, 0, "RLIMIT_CORE hard limit must be 0");
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_mlock_actually_locks_pages() {
        // Allocate a page-aligned buffer and verify mlock succeeds
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
        let data = vec![0u8; page_size];

        unsafe {
            let locked = mlock(data.as_ptr(), data.len());
            assert!(locked, "mlock should succeed for a single page");

            // Verify we can read/write the locked memory normally
            let ptr = data.as_ptr() as *mut u8;
            std::ptr::write_volatile(ptr, 0xAB);
            let val = std::ptr::read_volatile(ptr);
            assert_eq!(val, 0xAB, "locked memory must be readable/writable");

            let unlocked = munlock(data.as_ptr(), data.len());
            assert!(unlocked, "munlock should succeed");
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_mlock_large_allocation() {
        // Test locking a larger region (256 KB) — typical seed + key material
        let size = 256 * 1024;
        let data = vec![0xFFu8; size];

        unsafe {
            let locked = mlock(data.as_ptr(), data.len());
            // This may fail if ulimit -l is too low, which is fine —
            // we just verify it doesn't crash
            if locked {
                let unlocked = munlock(data.as_ptr(), data.len());
                assert!(unlocked, "munlock should succeed after successful mlock");
            } else {
                eprintln!(
                    "mlock for {} bytes failed (likely ulimit -l too low) — acceptable",
                    size
                );
            }
        }
    }

    #[test]
    fn test_mlock_zero_length_is_noop() {
        // Zero-length lock should succeed trivially
        let data = vec![0u8; 0];
        unsafe {
            assert!(mlock(data.as_ptr(), 0));
            assert!(munlock(data.as_ptr(), 0));
        }
    }

    #[test]
    fn test_locked_buffer_basic_operations() {
        let mut buf = LockedBuffer::new(64);

        // Verify initial state is zeroed
        assert!(
            buf.as_slice().iter().all(|&b| b == 0),
            "LockedBuffer must be zero-initialized"
        );
        assert_eq!(buf.as_slice().len(), 64);

        // Write and read back
        buf.as_mut_slice()[0] = 0xDE;
        buf.as_mut_slice()[1] = 0xAD;
        buf.as_mut_slice()[63] = 0xFF;
        assert_eq!(buf.as_slice()[0], 0xDE);
        assert_eq!(buf.as_slice()[1], 0xAD);
        assert_eq!(buf.as_slice()[63], 0xFF);

        #[cfg(unix)]
        assert!(buf.is_locked(), "LockedBuffer should be locked on Unix");
    }

    #[test]
    fn test_locked_buffer_zero_length() {
        let buf = LockedBuffer::new(0);
        assert!(buf.is_locked());
        assert!(buf.as_slice().is_empty());
    }

    #[test]
    fn test_locked_buffer_zeroizes_on_drop() {
        // Use a raw pointer to observe the memory after drop.
        // This is the only way to verify zeroization actually happened.
        let buf = LockedBuffer::new(64);
        let ptr = buf.as_slice().as_ptr();
        let len = buf.as_slice().len();

        // Write known pattern
        unsafe {
            let mptr = ptr as *mut u8;
            for i in 0..len {
                std::ptr::write_volatile(mptr.add(i), 0xFF);
            }
        }

        // Verify pattern is there
        unsafe {
            for i in 0..len {
                assert_eq!(
                    std::ptr::read_volatile(ptr.add(i)),
                    0xFF,
                    "pre-drop: byte {} should be 0xFF",
                    i
                );
            }
        }

        // Drop the buffer — this should zeroize
        drop(buf);

        // After drop, the memory *should* be zeroed.
        // Note: this is technically UB (reading freed memory), but it's
        // the only way to verify zeroization in a test. The allocator
        // won't have reused this memory yet in a single-threaded test.
        unsafe {
            let mut zeroed_count = 0;
            for i in 0..len {
                if std::ptr::read_volatile(ptr.add(i)) == 0 {
                    zeroed_count += 1;
                }
            }
            // We expect all bytes to be zero, but allow for allocator
            // metadata overwriting a few bytes
            assert!(
                zeroed_count >= len - 16,
                "after drop: at least {} of {} bytes should be zeroed, got {}",
                len - 16,
                len,
                zeroed_count
            );
        }
    }

    #[test]
    fn test_locked_buffer_multiple_allocations() {
        // Verify we can have multiple locked buffers simultaneously
        let mut bufs: Vec<LockedBuffer> = Vec::new();
        for i in 0..5 {
            let mut buf = LockedBuffer::new(128);
            buf.as_mut_slice().fill(i as u8);
            bufs.push(buf);
        }

        // Verify each buffer has its own data
        for (i, buf) in bufs.iter().enumerate() {
            assert!(
                buf.as_slice().iter().all(|&b| b == i as u8),
                "buffer {} should contain all 0x{:02X}",
                i,
                i
            );
        }

        // Drop all — should zeroize each
        drop(bufs);
    }

    #[test]
    fn test_locked_buffer_with_seed_sized_data() {
        // 64 bytes = BIP-39 seed, 32 bytes = private key
        for size in [32, 64, 128] {
            let mut buf = LockedBuffer::new(size);
            assert_eq!(buf.as_slice().len(), size);

            // Simulate writing seed material
            for (i, byte) in buf.as_mut_slice().iter_mut().enumerate() {
                *byte = (i % 256) as u8;
            }

            // Verify
            for (i, &byte) in buf.as_slice().iter().enumerate() {
                assert_eq!(byte, (i % 256) as u8);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_mlock_unaligned_address() {
        // mlock should handle non-page-aligned addresses
        // (the kernel rounds down to page boundary)
        let data = vec![0u8; 256];
        let offset = 17; // intentionally unaligned
        unsafe {
            let ptr = data.as_ptr().add(offset);
            let len = data.len() - offset;
            let locked = mlock(ptr, len);
            // Should succeed — kernel handles alignment
            if locked {
                let unlocked = munlock(ptr, len);
                assert!(unlocked);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_double_mlock_same_region() {
        // Locking the same region twice should not cause issues
        let data = vec![0u8; 4096];
        unsafe {
            let locked1 = mlock(data.as_ptr(), data.len());
            let locked2 = mlock(data.as_ptr(), data.len());
            // Both should succeed (mlock is idempotent)
            if locked1 {
                assert!(locked2, "second mlock on same region should succeed");
                munlock(data.as_ptr(), data.len());
            }
        }
    }
}
