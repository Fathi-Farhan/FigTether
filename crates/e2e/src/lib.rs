//! FigTether end-to-end test harness.
//!
//! This crate hosts the deterministic half of the FigTether acceptance suite.
//! It drives the shipped crates, so the acceptance flows prove the behaviour
//! of the code that is actually shipped.
//!
//! # What the crate holds
//!
//! - [`shipped`]: the lifecycle helper of the shipped `figtether-daemon`. It
//!   starts the daemon on a free loopback port with a temporary runtime
//!   directory, waits until the health endpoint answers, and shuts the daemon
//!   down cleanly at the end of the test.
//! - [`fake_plugin`]: an in-process plugin client that dials the daemon bridge
//!   endpoint (`ws://127.0.0.1:<port>/bridge`) with the real
//!   `figtether_protocol` frames, performs the `hello_auth` handshake, answers
//!   capability probes and serves read requests from a fixture corpus.
//! - [`fake_client`]: a local client that speaks
//!   `figtether_protocol::client::ClientRequest` and `ClientResponse` over the
//!   daemon client endpoint.
//! - [`fixtures`]: the corpus of the repository. The harness reads the
//!   committed files under `fixtures/`, so the acceptance flows run against
//!   the same documents as the acceptance catalog. The inline generator stays
//!   available as a fallback for a scenario that mutates a document or that
//!   needs a document of an exact size.
//! - [`params`]: constructors for the tool parameters, so a test only states
//!   what matters for its scenario.
//! - [`daemon`]: the harness daemon, a stand-in for `figtether-daemon`. The
//!   module comment names the behaviours only it can produce.
//! - [`mcp_stdio`]: a JSON-RPC over stdio server used by the MCP lifecycle
//!   acceptance test. The module comment names the behaviour only it can
//!   produce.
//!
//! # Scope and limits of this harness (PRD 8.8)
//!
//! This harness does **not** replace testing against real Figma Desktop. The
//! PRD and `docs/06-testing/01-test-strategy.md` rule 5.2.3 are explicit:
//! automation never replaces desktop tests, and source inspection never
//! substitutes for runtime proof. The harness proves the protocol, the
//! authorization rules, the epoch handling and the resource contract of the
//! Rust side of the product. It cannot prove anything about the editor.
//!
//! The following acceptance scenarios still require a real Figma Desktop run
//! with the plugin installed, and the tests in this crate are supporting
//! evidence for them only:
//!
//! - `AT03` new user install, pair, grant and read on two operating systems;
//! - `AT04` fresh read tracks edits while snapshots stay stable (the editor
//!   edit itself is desktop only, the harness simulates it);
//! - `AT08` the mixed soak of the resource contract, which needs one real
//!   editor session inside the run;
//! - `AT09` sleep and wake of the machine, and the desktop capture of the
//!   offline window;
//! - `AT10` normalization fidelity against curated fixtures, including the
//!   desktop versus CI diff;
//! - `AT11` asset export fidelity, which depends on the Figma renderer;
//! - `AT12` capability probing under real seat and permission contexts;
//! - `AT14` the second, independent MCP client of the catalog, which is a
//!   desktop client;
//! - `AT17` scope changes driven by editorial control of the hierarchy;
//! - `AT18` the document, selection and page state observed in the editor.
//!
//! `AT01`, `AT02`, `AT05`, `AT06`, `AT07` and `AT15` are properties of the
//! Rust core, the daemon and the MCP adapter, so the harness is the primary
//! evidence for them.
//!
//! # Determinism rules
//!
//! - No test sleeps longer than it has to. Readiness and state changes are
//!   polled with a deadline instead of guessed with a fixed sleep.
//! - Every deadline used by a test is generous, so a slow machine does not
//!   turn a passing flow into a failure.
//! - Every fixture is read from the committed corpus or generated in memory
//!   and deterministically, so no test depends on files that another agent
//!   may or may not have written yet.
//! - Every test cleans up after itself, also when it fails: the daemon of a
//!   test is stopped and its runtime directory is removed with the temporary
//!   directory that holds it.

pub mod daemon;
pub mod fake_client;
pub mod fake_plugin;
pub mod fixtures;
pub mod harness;
pub mod mcp_stdio;
pub mod params;
pub mod shipped;

pub use daemon::{DaemonProfile, HarnessDaemon};
pub use fake_client::FakeLocalClient;
pub use fake_plugin::{FakePlugin, PluginBehavior, PluginError};
pub use fixtures::FixtureCorpus;
pub use harness::{assert_loopback_only, DaemonHarness, HarnessError};
pub use shipped::{free_loopback_port, ShippedDaemon, ShortLivedPairingDaemon};
