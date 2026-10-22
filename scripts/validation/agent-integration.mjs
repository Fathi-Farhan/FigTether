import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, existsSync, writeFileSync, readFileSync, symlinkSync } from "node:fs";
import { resolve } from "node:path";
import { createHash } from "node:crypto";
import { fixture, mcpClient, until, delay } from "./agent-fixture.mjs";
const flag = (name) => {
  const i = process.argv.indexOf(name);
  return i < 0 ? undefined : process.argv[i + 1];
};
const binary = resolve(
  flag("--binary") ?? process.env.FIGTETHER_TEST_BINARY ?? "target/release/figtether.exe",
);
const out = resolve(
  flag("--out") ??
    `benchmarks/results/agent-integration-${new Date().toISOString().replace(/[:.]/g, "-")}`,
);
if (existsSync(out)) throw Error("choose a new evidence directory");
mkdirSync(out, { recursive: true });
const rows = [],
  calls = [];
const check = (name, ok) => {
  rows.push({ name, ok });
  if (!ok) throw Error(name);
};
const cli = (...args) => spawnSync(binary, args, { encoding: "utf8", windowsHide: true });
const asyncCli = (...args) =>
  new Promise((resolve, reject) => {
    const child = spawn(binary, args, { windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "",
      stderr = "";
    child.stdout.on("data", (c) => (stdout += c));
    child.stderr.on("data", (c) => (stderr += c));
    child.on("error", reject);
    child.on("close", (status) => resolve({ status, stdout, stderr }));
  });
let f, m;
try {
  f = await fixture({ binary, out: resolve(out, "clean") });
  m = await mcpClient(binary, f.runtime);
  const tools = await m.rpc("tools/list");
  check(
    "17 tools include canvas entry and image retrieval",
    tools.tools.length === 17 && tools.tools.some((t) => t.name === "open_design"),
  );
  let opened = await m.tool("open_design", {});
  writeFileSync(resolve(out, "discovery.json"), JSON.stringify(opened.structuredContent, null, 2));
  check(
    "ambiguous roots return named choices",
    opened.structuredContent.status === "choose_frame" &&
      opened.structuredContent.choices.length === 2 &&
      opened.structuredContent.choices[0].name === "Website Test",
  );
  opened = await m.tool("open_design", { frame_name: "Website Test" });
  check(
    "named frame opens with bounded context and editing access",
    opened.structuredContent.status === "ready" &&
      opened.structuredContent.editing_allowed === true &&
      opened.structuredContent.tree.coverage.returned_nodes > 0,
  );
  check(
    "render is a standard MCP PNG block",
    opened.content.some(
      (c) => c.type === "image" && c.mimeType === "image/png" && c.data.length > 100,
    ),
  );
  check(
    "encoded image is absent from text and structured metadata",
    !JSON.stringify(opened.structuredContent).includes("image_base64") &&
      !opened.content.filter((c) => c.type === "text").some((c) => c.text.includes("image_base64")),
  );
  check(
    "live daemon budgets are returned",
    opened.structuredContent.write_budgets.max_operations_per_plan === 50,
  );
  const declared = await m.tool("list_sessions", {});
  check(
    "agents discover the actual write contract without guessing properties",
    declared.structuredContent.data.write_capabilities.create_node_types.includes("COMPONENT") &&
      declared.structuredContent.data.write_capabilities.properties.includes("layoutMode") &&
      declared.structuredContent.data.write_capabilities.approval_required === true,
  );
  let missing = await m.tool("open_design", { frame_name: "No matching frame" });
  check(
    "unknown target cannot choose another frame",
    missing.structuredContent.status === "needs_access_or_target",
  );
  f.failNextRead("TIMEOUT");
  const failedDiscovery = await m.tool("open_design", { session_id: f.session, frame_id: "1:2" });
  check(
    "discovery timeout preserves the actual error instead of asking for a new grant",
    failedDiscovery.isError === true &&
      failedDiscovery.structuredContent.code === "TIMEOUT" &&
      failedDiscovery.structuredContent.status !== "needs_access_or_target",
  );
  const sid = f.session;
  const plan = {
    plan_id: "fixture-heading",
    session_id: sid,
    destructive: false,
    intent: "Add an editable heading",
    operations: [
      {
        kind: "create_node",
        parent_id: "1:2",
        node_type: "TEXT",
        properties: { name: "Hero heading", x: 40, y: 70, fontSize: 36 },
        text: "Design with the canvas in view",
        font_family: "Inter",
        font_style: "Bold",
        fills: [{ type: "SOLID", color: "#192C28" }],
      },
    ],
  };
  const preview = await m.tool("preview_write", { session_id: sid, plan });
  check("native edit preview succeeds", preview.isError !== true);
  const aid = preview.structuredContent.approval_id;
  let approval;
  await until(async () => {
    approval = await m.tool("get_write_approval", { approval_id: aid });
    return approval.structuredContent?.status === "approved";
  });
  const applied = await m.tool("apply_write", {
    plan,
    approval_token: approval.structuredContent.approval_token,
  });
  check(
    "exact approved plan applies in the synthetic editor",
    applied.isError !== true && applied.structuredContent.created.length === 1,
  );
  const replay = await m.tool("apply_write", {
    plan: { ...plan, intent: "Changed plan" },
    approval_token: approval.structuredContent.approval_token,
  });
  check("changed plan cannot reuse approval", replay.isError === true);
  const after = await m.tool("open_design", { session_id: sid, frame_id: "1:2" });
  check(
    "post-edit read and render are available",
    after.structuredContent.tree.data.nodes.length === 2 &&
      after.content.some((c) => c.type === "image"),
  );
  const exported = await m.tool("export_assets", {
    session_id: sid,
    node_ids: ["1:2"],
    format: "png",
    scale: 0.5,
  });
  const asset = exported.structuredContent.data.asset_id;
  const image = await m.tool("get_asset", { session_id: sid, asset_id: asset });
  check(
    "explicit image retrieval returns image",
    image.content.some((c) => c.type === "image"),
  );
  const cross = await m.tool("get_asset", {
    session_id: "11111111-1111-4111-8111-111111111111",
    asset_id: asset,
  });
  check("image retrieval refuses another session", cross.isError === true);
  const page = await m.tool("list_nodes", {
    session_id: sid,
    root_id: "1:2",
    limit: 1,
    fields: ["id", "name", "type"],
  });
  const next = await m.tool("list_nodes", {
    session_id: sid,
    root_id: "1:2",
    limit: 1,
    fields: ["id", "name", "type"],
    cursor: page.structuredContent.coverage.next_cursor,
  });
  check(
    "opaque plugin cursor reaches a different next page",
    next.isError !== true &&
      next.structuredContent.data.nodes[0].id !== page.structuredContent.data.nodes[0].id,
  );
  for (const client of ["codex", "gemini", "claude"]) {
    const workspace = resolve(out, `workspace-${client}`);
    mkdirSync(workspace, { recursive: true });
    const config = resolve(
      workspace,
      client === "codex"
        ? ".codex/config.toml"
        : client === "gemini"
          ? ".gemini/settings.json"
          : ".mcp.json",
    );
    mkdirSync(resolve(config, ".."), { recursive: true });
    writeFileSync(
      config,
      client === "codex"
        ? 'model = "preserve-me"\n'
        : '{"unrelated":"private-setting-value","mcpServers":{"other":{"command":"other"}}}',
    );
    const setupArgs = [
      "setup",
      "--client",
      client,
      "--workspace",
      workspace,
      "--runtime-dir",
      f.runtime,
      "--json",
    ];
    const before = readFileSync(config, "utf8"),
      dry = cli(...setupArgs);
    check(
      `${client} setup previews without touching config`,
      dry.status === 0 && readFileSync(config, "utf8") === before,
    );
    check(
      `${client} preview excludes unrelated settings`,
      !dry.stdout.includes("private-setting-value") && !dry.stdout.includes("preserve-me"),
    );
    check(`${client} project setup installs`, cli(...setupArgs, "--apply").status === 0);
    const verify = await asyncCli(
      "setup",
      "--client",
      client,
      "--workspace",
      workspace,
      "--check",
      "--json",
    );
    let status;
    try {
      status = JSON.parse(verify.stdout);
    } catch {
      status = {};
    }
    check(
      `${client} readiness probes registered adapter and live scoped read`,
      verify.status === 0 &&
        status.checks?.scoped_read === true &&
        status.checks.host_tool_visibility.startsWith("unverified"),
    );
    check(`${client} repeated setup is idempotent`, cli(...setupArgs, "--apply").status === 0);
    const correct = readFileSync(config, "utf8");
    writeFileSync(config, correct.replaceAll(binary.replaceAll("\\", "\\\\"), "changed-adapter"));
    // Use an unambiguous skill edit to verify user changes are not overwritten.
    const skill = resolve(
      workspace,
      client === "codex"
        ? ".agents/skills/figtether-design/SKILL.md"
        : `.${client === "gemini" ? "gemini" : "claude"}/skills/figtether-design/SKILL.md`,
    );
    const correctSkill = readFileSync(skill, "utf8");
    writeFileSync(skill, correctSkill + "\nUser edits\n");
    check(`${client} refuses to overwrite user changes`, cli(...setupArgs, "--apply").status !== 0);
    writeFileSync(skill, correctSkill);
    writeFileSync(config, correct);
    check(
      `${client} removal preserves unrelated settings`,
      cli(...setupArgs, "--remove").status === 0 &&
        readFileSync(config, "utf8").includes(
          client === "codex" ? "preserve-me" : "private-setting-value",
        ) &&
        !existsSync(skill),
    );
  }
  const linked = resolve(out, "linked-workspace"),
    outside = resolve(out, "outside");
  mkdirSync(linked);
  mkdirSync(outside);
  symlinkSync(
    outside,
    resolve(linked, ".codex"),
    process.platform === "win32" ? "junction" : "dir",
  );
  check(
    "setup refuses linked config paths",
    cli("setup", "--client", "codex", "--workspace", linked, "--apply").status !== 0 &&
      !existsSync(resolve(outside, "config.toml")),
  );
  await f.changeGrant("read");
  const stale = await m.tool("get_asset", { session_id: sid, asset_id: asset });
  check("grant replacement invalidates old image", stale.isError === true);
  const readOnly = await m.tool("open_design", { frame_name: "Website Test" });
  check(
    "read-only design provides explicit editing guidance",
    readOnly.structuredContent.editing_allowed === false &&
      readOnly.structuredContent.next_action.includes("Allow editing"),
  );
  const refused = await m.tool("preview_write", { session_id: sid, plan });
  check("read-only grant cannot preview writes", refused.isError === true);
  calls.push(...m.calls);
  m.close();
  m = null;
  await f.close();
  f = null;
  f = await fixture({
    binary,
    out: resolve(out, "warnings"),
    warnings: ["Font unavailable"],
    twoFrames: false,
  });
  m = await mcpClient(binary, f.runtime);
  const warningPlan = { ...plan, session_id: f.session };
  const wp = await m.tool("preview_write", { session_id: f.session, plan: warningPlan });
  await delay(75);
  const pending = await m.tool("get_write_approval", {
    approval_id: wp.structuredContent.approval_id,
  });
  check(
    "warning plan remains pending for a user decision",
    pending.structuredContent.status === "pending" && f.nodes.size === 1,
  );
  calls.push(...m.calls);
  writeFileSync(
    resolve(out, "result.json"),
    JSON.stringify(
      {
        kind: "real-binary-and-MCP-with-synthetic-editor",
        recorded_at: new Date().toISOString(),
        binary_sha256: createHash("sha256").update(readFileSync(binary)).digest("hex"),
        rows,
        calls,
        cost: { model_calls: 0, tokens: null, billed_usd: null, status: "unmeasured" },
        limits: [
          "No real agent model or native Figma editor in this deterministic run",
          "Browser preview is a synthetic scene, not Figma rendering",
          "Host configuration parsing and MCP readiness do not establish model task performance",
        ],
      },
      null,
      2,
    ),
  );
  console.log(JSON.stringify({ passed: rows.length, failed: 0, out }));
} finally {
  writeFileSync(resolve(out, "last-observations.json"), JSON.stringify(rows, null, 2));
  m?.close();
  if (f) await f.close();
}
