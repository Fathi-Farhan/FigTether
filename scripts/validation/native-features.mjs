// Live Figma evidence only. Never grants access, changes Auto, or clicks approvals.
import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync, appendFileSync, existsSync } from "node:fs";
import { randomUUID } from "node:crypto";
import path from "node:path";
import { mcpClient } from "./agent-fixture.mjs";
const flag = (name) => {
  const i = process.argv.indexOf(name);
  return i < 0 ? undefined : process.argv[i + 1];
};
const binary =
  flag("--binary") ??
  path.resolve("target/debug", process.platform === "win32" ? "figtether.exe" : "figtether");
const session = flag("--session"),
  root = flag("--root"),
  phase = flag("--phase") ?? "read";
assert(session && root, "pass --session UUID and --root GRANTED_FRAME_ID");
assert(["read", "features", "verify-features", "design"].includes(phase), "unknown native phase");
const out = path.resolve(flag("--out") ?? "artifacts/end-user-auto/native");
mkdirSync(out, { recursive: true });
const stateFile = path.join(out, "state.json");
const state = existsSync(stateFile) ? JSON.parse(readFileSync(stateFile, "utf8")) : {};
const m = await mcpClient(
  binary,
  flag("--runtime-dir") ?? path.join(process.env.LOCALAPPDATA, "figtether"),
);
const rows = [];
const redact = (v) =>
  Array.isArray(v)
    ? v.map(redact)
    : v && typeof v === "object"
      ? Object.fromEntries(
          Object.entries(v).map(([k, x]) => [
            k,
            /token|secret|credential/i.test(k) ? "[REDACTED]" : redact(x),
          ]),
        )
      : v;
const body = (r) =>
  r.structuredContent ?? JSON.parse(r.content.find((c) => c.type === "text").text);
async function call(id, name, args, allowError = false) {
  const started = performance.now();
  let r;
  try {
    r = await m.tool(name, args);
  } catch (error) {
    const row = {
      id,
      tool: name,
      elapsed_ms: performance.now() - started,
      is_error: true,
      data: {
        code: "HARNESS_TRANSPORT_FAILURE",
        message: String(error.message),
        action: "Inspect transport and retained editor evidence; no native success is established.",
      },
    };
    rows.push(row);
    appendFileSync(path.join(out, "calls.jsonl"), JSON.stringify(row) + "\n");
    console.log(JSON.stringify({ id, tool: name, error: true, code: row.data.code }));
    if (!allowError) throw error;
    return row.data;
  }
  const d = body(r);
  const row = {
    id,
    tool: name,
    elapsed_ms: performance.now() - started,
    is_error: r.isError === true,
    data: redact(d),
  };
  rows.push(row);
  appendFileSync(path.join(out, "calls.jsonl"), JSON.stringify(row) + "\n");
  console.log(
    JSON.stringify({
      id,
      tool: name,
      error: r.isError === true,
      code: d.code,
      status: d.status,
      elapsed_ms: row.elapsed_ms,
    }),
  );
  for (const [i, b] of (r.content ?? []).entries())
    if (b.type === "image")
      writeFileSync(path.join(out, `${id}-${i}.png`), Buffer.from(b.data, "base64"));
  if (r.isError && !allowError)
    throw Error(
      `${id}: ${d.code}. ${d.action ?? "Inspect the retained response before retrying."}`,
    );
  return d;
}
async function write(id, operations) {
  const plan = { plan_id: `${id}-${randomUUID()}`, session_id: session, intent: id, operations };
  writeFileSync(path.join(out, `${id}.plan.json`), JSON.stringify(plan, null, 2));
  const p = await call(`${id}-preview`, "preview_write", { session_id: session, plan });
  assert(
    p.blocked !== true && (p.preview?.warnings ?? p.warnings ?? []).length === 0,
    "Preview needs review; no apply attempted",
  );
  const a = await call(`${id}-approval`, "get_write_approval", {
    approval_id: p.approval_id ?? p.data?.approval_id,
  });
  const token = a.approval_token ?? a.data?.approval_token;
  assert(
    token,
    "Approval is pending. Review it in the Bridge; this script does not approve or enable Auto.",
  );
  const result = await call(`${id}-apply`, "apply_write", {
    session_id: session,
    plan,
    approval_token: token,
  });
  assert.equal(result.status, "applied", "A partial write must be inspected before another plan");
  assert.equal(result.applied_operations, operations.length);
  return result.created ?? [];
}
const create = (parent, type, name, x, y, width, height, extra = {}) => ({
  kind: "create_node",
  parent_id: parent,
  node_type: type,
  properties: { name, x, y, width, height, ...extra.properties },
  ...Object.fromEntries(Object.entries(extra).filter(([key]) => key !== "properties")),
});
const fill = (color) => [{ type: "SOLID", color }];
async function learn(parent = root) {
  return call(`learn-${parent.replaceAll(":", "-")}`, "list_nodes", {
    session_id: session,
    root_id: parent,
    depth: 2,
    limit: 80,
    fields: ["id", "type", "name", "parent_id", "geometry", "text", "fills"],
  });
}
async function render(id, node = root, scale = 0.35) {
  const e = await call(`${id}-export`, "export_assets", {
    session_id: session,
    node_ids: [node],
    format: "png",
    scale,
  });
  await call(id, "get_asset", { session_id: session, asset_id: e.data.asset_id });
}
try {
  const opened = await call(`open-${phase}`, "open_design", {
    session_id: session,
    frame_id: root,
  });
  assert.equal(
    opened.status,
    "ready",
    "Open the intended shared frame before running native checks",
  );
  assert.equal(opened.render_available, true, "A live PNG is required for native evidence");
  if (phase === "read") {
    const calls = [
      ["bridge", "get_bridge_status", {}],
      ["sessions", "list_sessions", {}],
      ["selection", "get_selection", { session_id: session }],
      [
        "read-node",
        "get_nodes",
        {
          session_id: session,
          node_ids: [root],
          depth: 0,
          fields: ["id", "name", "type", "geometry", "fills"],
        },
      ],
      ["inspect", "inspect_design", { session_id: session, root_id: root, detail: "full" }],
      ["tokens", "get_design_tokens", { session_id: session, root_id: root, kinds: ["all"] }],
      [
        "svg",
        "export_assets",
        { session_id: session, node_ids: [root], format: "svg", scale: 0.25 },
      ],
      ["watch", "watch_scope", { session_id: session, root_id: root }],
      ["changes", "get_changes", { session_id: session, watch_id: "not-created" }],
      ["snapshot", "get_snapshot_info", { snapshot_id: randomUUID() }],
      ["unknown-job", "get_job_status", { job_id: randomUUID() }],
    ];
    for (const [id, name, args] of calls) await call(id, name, args, true);
    await render("read-render");
  } else if (phase === "features") {
    assert(
      !state.qa,
      "A native fixture already exists. Inspect its retained state before resuming; do not duplicate a partial write.",
    );
    const created = await write("native-feature-fixture", [
      create(root, "FRAME", "Native feature checks", 72, 100, 1200, 620, {
        fills: fill("#F5F5EF"),
      }),
    ]);
    state.qa = created[0];
    writeFileSync(stateFile, JSON.stringify(state, null, 2));
    await learn();
    const ids = await write("native-shapes-and-typography", [
      create(state.qa, "RECTANGLE", "Solid / transparency", 24, 24, 120, 90, {
        fills: [{ type: "SOLID", color: "#255D4680", opacity: 0.8 }],
      }),
      create(state.qa, "ELLIPSE", "Ellipse", 180, 24, 90, 90, { fills: fill("#D9EFAD") }),
      create(state.qa, "LINE", "Line", 310, 24, 110, 0),
      create(state.qa, "VECTOR", "Vector node", 450, 24, 100, 90),
      create(state.qa, "COMPONENT", "Reusable component", 590, 24, 160, 90, {
        fills: fill("#245D45"),
        properties: { cornerRadius: 8 },
      }),
      create(state.qa, "TEXT", "Editable text", 24, 155, 600, 50, {
        text: "An editable idea.",
        font_family: "Inter",
        font_style: "Regular",
        properties: { fontSize: 28 },
        fills: fill("#172D23"),
      }),
      create(state.qa, "FRAME", "Auto layout", 24, 260, 600, 120, {
        properties: {
          layoutMode: "HORIZONTAL",
          itemSpacing: 12,
          paddingLeft: 16,
          paddingRight: 16,
          paddingTop: 16,
          paddingBottom: 16,
          layoutSizingVertical: "HUG",
        },
        fills: fill("#E3E9DE"),
      }),
    ]);
    [state.rectangle, , , , , state.text, state.autoLayout] = ids;
    writeFileSync(stateFile, JSON.stringify(state, null, 2));
    await learn(state.qa);
    await write("native-text-properties-and-layout", [
      {
        kind: "set_text",
        node_id: state.text,
        text: "A little room to make something new.",
        font_family: "Inter",
        font_style: "Regular",
      },
      {
        kind: "set_properties",
        node_id: state.rectangle,
        properties: { cornerRadius: 12, rotation: 8, opacity: 0.75 },
      },
      create(state.autoLayout, "FRAME", "Fill container", 0, 0, 160, 64, {
        properties: { layoutSizingHorizontal: "FILL" },
        fills: fill("#C4D6B8"),
      }),
    ]);
    const gradients = [
      "GRADIENT_LINEAR",
      "GRADIENT_RADIAL",
      "GRADIENT_ANGULAR",
      "GRADIENT_DIAMOND",
    ];
    await write(
      "native-gradient-fills",
      gradients.map((type, i) =>
        create(state.qa, "RECTANGLE", type, 24 + i * 200, 450, 180, 100, {
          fills: [
            {
              type,
              stops: [
                { position: 0, color: "#153F2E" },
                { position: 1, color: "#D8F59A" },
              ],
            },
          ],
        }),
      ),
    );
    await learn(state.qa);
    await render("native-feature-render", state.qa, 0.7);
    await write("hide-completed-native-fixture", [
      { kind: "set_properties", node_id: state.qa, properties: { visible: false } },
    ]);
  } else if (phase === "verify-features") {
    assert(state.qa, "Run features first and retain its state file");
    await learn();
    await learn(state.qa);
    const layoutChildren = await learn(state.autoLayout);
    const child = layoutChildren.data.nodes.find(
      (n) => n.name === "Fill container" && n.parent_id === state.autoLayout,
    );
    assert(child, "Read the real Auto Layout child before editing it");
    await write("show-native-fixture-for-render", [
      { kind: "set_properties", node_id: state.qa, properties: { visible: true, x: 72, y: 100 } },
      {
        kind: "set_properties",
        node_id: child.id,
        properties: { layoutSizingHorizontal: "FILL", width: 160, height: 64 },
      },
    ]);
    try {
      const observed = await call("native-feature-readback", "get_nodes", {
        session_id: session,
        node_ids: [state.rectangle, state.text, state.autoLayout, child.id],
        depth: 1,
        fields: ["id", "name", "type", "geometry", "text", "fills", "layout"],
      });
      const nodes = observed.data.nodes;
      assert.equal(
        nodes.find((n) => n.id === state.text).typography.text,
        "A little room to make something new.",
      );
      assert(
        Math.abs(
          nodes.find((n) => n.id === state.rectangle).geometry.local_transform.rotation - 8,
        ) < 0.01,
      );
      assert.equal(nodes.find((n) => n.id === state.autoLayout).geometry.bounding_box.height, 96);
      assert.equal(
        nodes.find((n) => n.id === child.id).geometry.bounding_box.width,
        568,
        "FILL must occupy the 600px parent minus 16px padding on both sides",
      );
      await render("native-feature-visible-render", state.qa, 0.7);
      const svg = await call("native-small-svg", "export_assets", {
        session_id: session,
        node_ids: [state.rectangle],
        format: "svg",
      });
      assert.equal(svg.data.mime, "image/svg+xml");
      const inlineSvg = await call(
        "native-svg-inline-refusal",
        "get_asset",
        { session_id: session, asset_id: svg.data.asset_id },
        true,
      );
      assert(inlineSvg.code, "SVG inline delivery must be refused rather than mislabeled as a PNG");
    } finally {
      await write("hide-verified-native-fixture", [
        { kind: "set_properties", node_id: state.qa, properties: { visible: false } },
      ]);
    }
    for (const kind of ["insert_child", "move_node", "delete_node"]) {
      const blocked = await call(
        `native-${kind}-requires-destructive-opt-in`,
        "preview_write",
        {
          session_id: session,
          plan: {
            plan_id: `refuse-${kind}-${randomUUID()}`,
            session_id: session,
            destructive: false,
            operations: [
              {
                kind,
                node_id: state.text,
                ...(kind === "delete_node" ? {} : { parent_id: state.qa }),
              },
            ],
          },
        },
        true,
      );
      assert.equal(blocked.code, "DESTRUCTIVE_NOT_ALLOWED");
    }
  } else {
    const parent = flag("--fixture");
    assert(parent, "Pass --fixture ID of the previously created, approved native frame");
    assert(!state.design_done, "Design already completed; inspect the canvas before another run");
    await learn();
    await write("studio-notes-base", [
      { kind: "set_properties", node_id: root, properties: { height: 5480 } },
      {
        kind: "set_properties",
        node_id: parent,
        properties: {
          name: "Studio Notes / A little room to explore",
          x: 72,
          y: 4510,
          width: 1296,
          height: 850,
        },
      },
      { kind: "set_fills", node_id: parent, fills: fill("#153F2E") },
    ]);
    const text = (name, value, x, y, width, height, size, color = "#F5F5EA", style = "Regular") =>
      create(parent, "TEXT", name, x, y, width, height, {
        text: value,
        font_family: "Inter",
        font_style: style,
        properties: { fontSize: size },
        fills: fill(color),
      });
    const ops = [
      text("Eyebrow", "THE STUDIO / NOTES & POSSIBILITIES", 56, 48, 750, 24, 13, "#D9F1AE"),
      text("Headline", "Good ideas need\nroom to move.", 56, 116, 760, 210, 76, "#F5F5EA", "Bold"),
      text(
        "Description",
        "Keep a reference. Try a different rhythm.\nMake a little space for the unexpected.",
        60,
        350,
        680,
        74,
        23,
        "#C4D5C7",
      ),
      create(parent, "ELLIPSE", "Orbit / outer", 943, 103, 240, 240, { fills: fill("#D7F3AC") }),
      create(parent, "ELLIPSE", "Orbit / cutout", 1012, 137, 175, 175, { fills: fill("#153F2E") }),
      create(parent, "RECTANGLE", "Orbit / horizon", 879, 332, 330, 2, { fills: fill("#91B69B") }),
      text("Orbit caption", "MAKE ROOM FOR WHAT COMES NEXT.", 880, 367, 350, 42, 12, "#D9F1AE"),
      create(parent, "RECTANGLE", "Editorial divider", 56, 480, 1184, 1, {
        fills: fill("#5E846A"),
      }),
    ];
    const entries = [
      ["01", "Collect a spark.", "Save the small detail you keep coming back to."],
      ["02", "Shift the rhythm.", "A new arrangement can change the whole feeling."],
      ["03", "Make it yours.", "Keep what feels right. Let the rest evolve."],
    ];
    entries.forEach(([n, title, description], i) => {
      const x = 56 + i * 404;
      ops.push(
        text(`Note ${n} / index`, n, x, 530, 100, 38, 30, "#D9F1AE"),
        text(`Note ${n} / title`, title, x, 594, 370, 38, 25, "#F5F5EA", "Bold"),
        text(`Note ${n} / description`, description, x, 652, 350, 70, 17, "#C4D5C7"),
      );
    });
    for (let start = 0; start < ops.length; start += 12)
      await write(`studio-notes-content-${start}`, ops.slice(start, start + 12));
    state.design_done = true;
    state.design = parent;
    writeFileSync(stateFile, JSON.stringify(state, null, 2));
    await learn(parent);
    await render("studio-notes-final", parent, 0.7);
    await render("full-canvas-final", root, 0.3);
  }
} finally {
  writeFileSync(
    path.join(out, `${phase}.summary.json`),
    JSON.stringify(
      {
        session_id: session,
        root_id: root,
        phase,
        calls: rows.length,
        failures: rows
          .filter((r) => r.is_error)
          .map((r) => ({ id: r.id, code: r.data.code, action: r.data.action })),
        claims:
          "Native editor evidence only for recorded successful calls; unsupported and untested features remain separate.",
      },
      null,
      2,
    ),
  );
  m.close();
}
