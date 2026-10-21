//! Probe the actual stdio adapter without claiming visibility in another host.
use crate::{cli::SetupArgs, commands::CommandOutcome, output};
use anyhow::{bail, Context};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

struct Probe {
    child: Child,
    responses: mpsc::Receiver<Value>,
    id: u64,
}
impl Drop for Probe {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Probe {
    fn start(binary: &Path, args: &[String]) -> anyhow::Result<Self> {
        let mut cmd = Command::new(binary);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().context("adapter stdout unavailable")?;
        let (tx, responses) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(v) = serde_json::from_str(&line) {
                    if tx.send(v).is_err() {
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            responses,
            id: 0,
        })
    }
    fn send(&mut self, value: &Value) -> anyhow::Result<()> {
        writeln!(
            self.child
                .stdin
                .as_mut()
                .context("adapter stdin unavailable")?,
            "{value}"
        )?;
        Ok(())
    }
    fn rpc(&mut self, method: &str, params: Value) -> anyhow::Result<Value> {
        self.id += 1;
        let id = self.id;
        self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(25);
        loop {
            let v = self
                .responses
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .context("MCP adapter did not answer; check daemon and runtime")?;
            if v["id"] == id {
                if v.get("error").is_some() {
                    bail!("MCP request was refused");
                }
                return Ok(v["result"].clone());
            }
        }
    }
}

pub fn run(
    args: &SetupArgs,
    root: &Path,
    binary: &Path,
    command_args: &[String],
) -> anyhow::Result<CommandOutcome> {
    let mut stages = json!({"project_entry":super::setup::installed(root,args.client),"adapter_initialized":false,"tools_discovered":false,"plugin_paired":false,"frames_shared":false,"scoped_read":false,"host_tool_visibility":"unverified; call open_design inside the host"});
    let probe = (|| -> anyhow::Result<()> {
        let mut p = Probe::start(binary, command_args)?;
        let init=p.rpc("initialize",json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"figtether-agent-check","version":env!("CARGO_PKG_VERSION")}}))?;
        stages["adapter_initialized"] = json!(init["serverInfo"]["name"] == "figtether-mcp");
        p.send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
        let tools = p.rpc("tools/list", json!({}))?;
        let names: Vec<_> = tools["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t["name"].as_str())
            .collect();
        stages["tools_discovered"] = json!([
            "open_design",
            "get_asset",
            "preview_write",
            "get_write_approval",
            "apply_write"
        ]
        .iter()
        .all(|n| names.contains(n)));
        stages["tool_count"] = json!(names.len());
        let sessions = p.rpc("tools/call", json!({"name":"list_sessions","arguments":{}}))?;
        let list = sessions["structuredContent"]["data"]["sessions"]
            .as_array()
            .context("list_sessions did not return sessions")?;
        stages["plugin_paired"] = json!(list
            .iter()
            .any(|s| s["state"] != "revoked" && s["state"] != "offline"));
        if let Some(s) = list.iter().find(|s| {
            s["state"] == "ready"
                && s["granted_root_ids"]
                    .as_array()
                    .is_some_and(|a| !a.is_empty())
        }) {
            stages["frames_shared"] = json!(true);
            let read=p.rpc("tools/call",json!({"name":"get_nodes","arguments":{"session_id":s["session_id"],"node_ids":[s["granted_root_ids"][0]],"fields":["id","type","name"],"depth":0}}))?;
            stages["scoped_read"] = json!(
                read["isError"] != true
                    && read["structuredContent"]["data"]["nodes"]
                        .as_array()
                        .is_some_and(|n| !n.is_empty())
            );
        }
        Ok(())
    })();
    let ok = probe.is_ok()
        && [
            "project_entry",
            "adapter_initialized",
            "tools_discovered",
            "plugin_paired",
            "frames_shared",
            "scoped_read",
        ]
        .iter()
        .all(|k| stages[k] == true);
    let report = json!({"ok":ok,"client":args.client.as_str(),"checks":stages,"error":probe.err().map(|_|"Adapter check failed. Start the daemon, pair the plugin, share a frame, and verify the runtime. No credentials are printed."),"next_action":"In the chosen host, refresh MCP and ask: Use FigTether open_design for my shared frame. CLI readiness does not prove the host exposes the tools."});
    output::line(&serde_json::to_string_pretty(&report)?);
    if ok {
        Ok(CommandOutcome::ok())
    } else {
        Ok(CommandOutcome::failed("agent connection needs attention"))
    }
}
