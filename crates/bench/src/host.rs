//! Host context of a benchmark run.
//!
//! The release evidence template asks for the operating system, the processor,
//! and the memory of the machine that produced a number. This module records
//! what the operating system exposes, and reports `None` for everything it
//! cannot observe, so the report can print `NOT_MEASURED` instead of a guess.

use serde::{Deserialize, Serialize};

/// The machine a benchmark run happened on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostContext {
    /// Operating system family, for example `windows`.
    pub os: String,
    /// Architecture, for example `x86_64`.
    pub arch: String,
    /// Number of logical processors the process may use.
    pub logical_cpus: usize,
    /// Processor model, when the platform exposes it.
    pub cpu_model: Option<String>,
    /// Total physical memory in bytes, when the platform exposes it.
    pub total_ram_bytes: Option<u64>,
}

impl HostContext {
    /// Capture the host context of the current process.
    pub fn capture() -> HostContext {
        HostContext {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            logical_cpus: std::thread::available_parallelism()
                .map(|count| count.get())
                .unwrap_or(0),
            cpu_model: None,
            total_ram_bytes: total_ram_bytes(),
        }
    }

    /// Operating system and architecture as one label.
    pub fn os_label(&self) -> String {
        format!("{} {}", self.os, self.arch)
    }

    /// Processor description.
    pub fn cpu_label(&self) -> String {
        match &self.cpu_model {
            Some(model) => format!("{model} ({} logical cores)", self.logical_cpus),
            None => format!(
                "{} logical cores (processor model not captured)",
                self.logical_cpus
            ),
        }
    }

    /// Memory description.
    pub fn ram_label(&self) -> Option<String> {
        self.total_ram_bytes
            .map(|bytes| format!("{} GiB total physical memory", bytes / (1024 * 1024 * 1024)))
    }
}

/// Total physical memory of the machine, when the platform exposes it.
fn total_ram_bytes() -> Option<u64> {
    platform::total_ram_bytes()
}

#[cfg(windows)]
mod platform {
    /// Layout of `MEMORYSTATUSEX` from the Windows API.
    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
    }

    /// Read the total physical memory of the machine.
    pub fn total_ram_bytes() -> Option<u64> {
        let mut status = MemoryStatusEx {
            length: std::mem::size_of::<MemoryStatusEx>() as u32,
            memory_load: 0,
            total_phys: 0,
            avail_phys: 0,
            total_page_file: 0,
            avail_page_file: 0,
            total_virtual: 0,
            avail_virtual: 0,
            avail_extended_virtual: 0,
        };
        let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
        if ok == 0 {
            return None;
        }
        Some(status.total_phys)
    }
}

#[cfg(target_os = "linux")]
mod platform {
    /// Read the total physical memory from `/proc/meminfo`.
    pub fn total_ram_bytes() -> Option<u64> {
        let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
        for line in meminfo.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                let kilobytes: u64 = rest
                    .split_whitespace()
                    .next()
                    .and_then(|value| value.parse().ok())?;
                return Some(kilobytes * 1024);
            }
        }
        None
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    /// Other platforms report nothing rather than a guess.
    pub fn total_ram_bytes() -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_context_is_never_empty() {
        let host = HostContext::capture();
        assert!(!host.os.is_empty());
        assert!(!host.arch.is_empty());
        assert!(!host.cpu_label().is_empty());
    }
}
