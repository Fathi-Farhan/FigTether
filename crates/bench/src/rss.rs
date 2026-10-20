//! Resident set size sampling.
//!
//! N04 asks for a resident set size sample every second, taken from outside
//! the measured path. This module reads the resident set size of the current
//! process. When the platform does not expose it, the sampler returns `None`
//! and the report prints `NOT_MEASURED`; it never substitutes a guess.

/// Current resident set size of this process in bytes.
///
/// Returns `None` when the platform does not expose the value.
pub fn current_rss_bytes() -> Option<u64> {
    imp::rss_bytes()
}

/// Whether the platform can report a resident set size.
pub fn is_supported() -> bool {
    imp::SUPPORTED
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;

    /// Layout of `PROCESS_MEMORY_COUNTERS` from the Windows API.
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn K32GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCounters,
            size: u32,
        ) -> i32;
        fn GetCurrentProcess() -> *mut c_void;
    }

    /// The Windows implementation is always available.
    pub const SUPPORTED: bool = true;

    /// Read the working set of the current process.
    pub fn rss_bytes() -> Option<u64> {
        let mut counters = ProcessMemoryCounters {
            cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
            page_fault_count: 0,
            peak_working_set_size: 0,
            working_set_size: 0,
            quota_peak_paged_pool_usage: 0,
            quota_paged_pool_usage: 0,
            quota_peak_non_paged_pool_usage: 0,
            quota_non_paged_pool_usage: 0,
            pagefile_usage: 0,
            peak_pagefile_usage: 0,
        };
        let status =
            unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
        if status == 0 {
            return None;
        }
        Some(counters.working_set_size as u64)
    }
}

#[cfg(target_os = "linux")]
mod imp {
    /// Page size of the Linux reference profile.
    const PAGE_SIZE: u64 = 4096;

    /// The Linux implementation reads the resident pages of the process.
    pub const SUPPORTED: bool = true;

    /// Read the resident set size from `/proc/self/statm`.
    pub fn rss_bytes() -> Option<u64> {
        let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
        let resident: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
        Some(resident * PAGE_SIZE)
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    /// Other platforms report nothing rather than a guess.
    pub const SUPPORTED: bool = false;

    /// No resident set size is available on this platform.
    pub fn rss_bytes() -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sampler_never_invents_a_value() {
        match current_rss_bytes() {
            Some(value) => assert!(value > 0),
            None => assert!(!is_supported()),
        }
    }
}
