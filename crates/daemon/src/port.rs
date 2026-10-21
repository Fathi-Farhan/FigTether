//! Port diagnostics for the loopback bridge.
//!
//! The daemon binds `127.0.0.1` on one fixed port, and the plugin manifest
//! references that port. The module answers two questions: why can this port
//! not be bound, and which port can be bound instead?
//!
//! Windows reserves ranges of TCP ports for the hypervisor (Hyper-V, WSL,
//! Docker Desktop) and reserves them again at every boot. A port inside such
//! a range cannot be bound even when no process listens on it, and the
//! operating system reports the refusal as `WSAEACCES` (os error 10013).
//! The module reads the exclusion table of the operating system, names the
//! range, and suggests a port outside every reserved range.
//!
//! The module never depends on the state of the daemon, so the CLI can use
//! it before the daemon starts.

use std::io::ErrorKind;
use std::net::{Ipv4Addr, TcpListener};

/// Number of ports the search for a free port covers.
///
/// The window starts at the preferred port and reaches `SEARCH_WINDOW` ports
/// above it, so a reserved block of a few hundred ports never hides every
/// usable port.
pub const SEARCH_WINDOW: u16 = 512;

/// Why a port cannot be bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortProblem {
    /// Another process listens on the port.
    InUse,
    /// The operating system reserved a range of ports, and the port is inside
    /// that range.
    ReservedByOs {
        /// First port of the reserved range.
        start: u16,
        /// Last port of the reserved range.
        end: u16,
    },
    /// The operating system or a local policy refused the bind.
    AccessDenied,
}

impl PortProblem {
    /// One sentence that explains the problem.
    ///
    /// The sentence names the reason and nothing else, so a caller can put
    /// it in front of its own next steps.
    pub fn message(&self) -> String {
        match self {
            Self::InUse => "Another process is listening on the port.".to_string(),
            Self::ReservedByOs { start, end } => format!(
                "Windows reserved the range {start}-{end} for the hypervisor, so the port cannot be used even when nothing listens on it."
            ),
            Self::AccessDenied => {
                "The operating system or a local policy refused the bind.".to_string()
            }
        }
    }
}

impl std::fmt::Display for PortProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

/// Diagnose why a port cannot be bound.
///
/// The function binds `127.0.0.1:<port>` and releases the socket again. A
/// free port returns `None`. A port that another process holds returns
/// [`PortProblem::InUse`]. On Windows a port inside a range that the
/// operating system reserved returns [`PortProblem::ReservedByOs`], and a
/// refusal without a known range returns [`PortProblem::AccessDenied`].
pub fn diagnose(port: u16) -> Option<PortProblem> {
    match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
        Ok(listener) => {
            drop(listener);
            None
        }
        Err(err) => match err.kind() {
            ErrorKind::AddrInUse => Some(PortProblem::InUse),
            ErrorKind::PermissionDenied => Some(reserved_problem(port)),
            _ => Some(PortProblem::AccessDenied),
        },
    }
}

/// The first port at or above `preferred` that can be bound.
///
/// The search skips the ranges that the operating system reserved and the
/// ports that another process already holds. It covers at most
/// [`SEARCH_WINDOW`] ports and returns `preferred` when none of them is
/// free, so the caller always receives a port it can report.
pub fn suggest_free_port(preferred: u16) -> u16 {
    let reserved = reserved_ranges();
    let start = first_unreserved(preferred, &reserved).unwrap_or(preferred);
    let mut candidate = u32::from(start);
    for _ in 0..=u32::from(SEARCH_WINDOW) {
        let Ok(port) = u16::try_from(candidate) else {
            break;
        };
        if !is_reserved(&reserved, port) && is_bindable(port) {
            return port;
        }
        candidate = candidate.saturating_add(1);
    }
    preferred
}

/// The port ranges that the operating system reserved.
///
/// The table is the exclusion table of Windows, and it is empty on every
/// other platform.
pub fn reserved_ranges() -> Vec<(u16, u16)> {
    #[cfg(windows)]
    {
        match read_exclusion_table() {
            Ok(text) => parse_reserved_ranges(&text),
            Err(_) => Vec::new(),
        }
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Parse the exclusion table of the operating system.
///
/// The table holds one range per line: the first port, the last port, and
/// optionally a `*` that marks an administered exclusion. The header and
/// the separator lines carry no numbers and are skipped.
pub fn parse_reserved_ranges(text: &str) -> Vec<(u16, u16)> {
    let mut ranges = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(start) = fields.next().and_then(|field| field.parse::<u16>().ok()) else {
            continue;
        };
        let Some(end) = fields.next().and_then(|field| field.parse::<u16>().ok()) else {
            continue;
        };
        // Only the mark of an administered exclusion may follow the range.
        if let Some(mark) = fields.next() {
            if mark != "*" {
                continue;
            }
        }
        if fields.next().is_some() || start > end {
            continue;
        }
        ranges.push((start, end));
    }
    ranges
}

/// The first port at or above `preferred` that is outside every range.
fn first_unreserved(preferred: u16, ranges: &[(u16, u16)]) -> Option<u16> {
    (u32::from(preferred)..=u32::from(u16::MAX))
        .filter_map(|candidate| u16::try_from(candidate).ok())
        .find(|port| !is_reserved(ranges, *port))
}

/// Whether the port is inside one of the ranges.
fn is_reserved(ranges: &[(u16, u16)], port: u16) -> bool {
    ranges
        .iter()
        .any(|(start, end)| port >= *start && port <= *end)
}

/// Whether the port can be bound right now.
fn is_bindable(port: u16) -> bool {
    match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
        Ok(listener) => {
            drop(listener);
            true
        }
        Err(_) => false,
    }
}

/// The reserved range that holds the port, when the system knows one.
#[cfg(windows)]
fn reserved_problem(port: u16) -> PortProblem {
    match reserved_ranges()
        .into_iter()
        .find(|(start, end)| port >= *start && port <= *end)
    {
        Some((start, end)) => PortProblem::ReservedByOs { start, end },
        None => PortProblem::AccessDenied,
    }
}

/// The reserved range that holds the port, when the system knows one.
///
/// Only Windows reserves port ranges, so every other platform reports a
/// plain access denial.
#[cfg(not(windows))]
fn reserved_problem(_port: u16) -> PortProblem {
    PortProblem::AccessDenied
}

/// Read the exclusion table of the operating system.
#[cfg(windows)]
fn read_exclusion_table() -> std::io::Result<String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    /// Flag that keeps the helper from opening a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let output = Command::new("netsh")
        .args([
            "interface",
            "ipv4",
            "show",
            "excludedportrange",
            "protocol=tcp",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(
            "the exclusion table of the operating system could not be read",
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exclusion table of a machine that runs the hypervisor.
    const TABLE: &str = "\
Start Port    End Port
-----------------------
     38929     39028
     39147     39147 *
";

    /// A port that nothing holds.
    fn free_port() -> u16 {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("binds");
        let port = listener.local_addr().expect("address").port();
        drop(listener);
        port
    }

    #[test]
    fn a_free_port_is_diagnosed_as_free() {
        assert_eq!(diagnose(free_port()), None);
    }

    #[test]
    fn a_port_in_use_is_diagnosed_as_in_use() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("binds");
        let port = listener.local_addr().expect("address").port();
        assert_eq!(diagnose(port), Some(PortProblem::InUse));
        drop(listener);
    }

    #[test]
    fn the_exclusion_table_is_parsed() {
        let ranges = parse_reserved_ranges(TABLE);
        assert_eq!(
            ranges,
            vec![(38929, 39028), (39147, 39147)],
            "the header and the separator are skipped"
        );
    }

    #[test]
    fn a_line_without_a_range_is_skipped() {
        let ranges = parse_reserved_ranges("Protocol TCP\nno range here\n");
        assert!(ranges.is_empty());
    }

    #[test]
    fn an_inverted_range_is_skipped() {
        let ranges = parse_reserved_ranges("     39028     38929\n");
        assert!(ranges.is_empty());
    }

    #[test]
    fn a_trailing_mark_that_is_not_a_star_is_skipped() {
        let ranges = parse_reserved_ranges("     38929     39028 x\n");
        assert!(ranges.is_empty());
    }

    #[test]
    fn a_reserved_port_names_its_range() {
        let problem = PortProblem::ReservedByOs {
            start: 38929,
            end: 39028,
        };
        let message = problem.message();
        assert!(message.contains("38929-39028"), "{message}");
        assert!(message.contains("hypervisor"), "{message}");
    }

    #[test]
    fn every_problem_explains_itself_in_one_sentence() {
        for problem in [
            PortProblem::InUse,
            PortProblem::ReservedByOs {
                start: 38929,
                end: 39028,
            },
            PortProblem::AccessDenied,
        ] {
            let message = problem.message();
            assert!(!message.is_empty());
            assert!(message.ends_with('.'), "{message}");
            assert_eq!(message, problem.to_string());
        }
    }

    #[test]
    fn the_search_skips_a_reserved_range() {
        let ranges = vec![(38929, 39028)];
        assert_eq!(first_unreserved(38947, &ranges), Some(39029));
        assert_eq!(first_unreserved(38929, &ranges), Some(39029));
        assert_eq!(first_unreserved(39028, &ranges), Some(39029));
        assert_eq!(first_unreserved(38947, &[]), Some(38947));
    }

    #[test]
    fn the_suggestion_never_falls_below_the_preferred_port() {
        let suggestion = suggest_free_port(38947);
        assert!(suggestion >= 38947, "the suggestion is {suggestion}");
    }

    #[test]
    fn the_suggestion_skips_the_reserved_ranges_of_the_machine() {
        let suggestion = suggest_free_port(38947);
        for (start, end) in reserved_ranges() {
            assert!(
                suggestion < start || suggestion > end,
                "the suggestion {suggestion} is inside the reserved range {start}-{end}"
            );
        }
    }
}
