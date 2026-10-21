// Builds editable native Figma nodes using the project's actual MCP write tools.
// UI approval is driven through the desktop helper under the user's test authorization.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
const session = process.argv[2];
if (!session)
  throw new Error("Usage: node scripts/validation/desktop-website.mjs SESSION_ID [BINARY]");
const binary = process.argv[3] ?? "D:/rust/kiln-target/debug/figtether.exe";
const dir = "tmp/desktop-validation";
const output = "benchmarks/results/desktop-2026-10-20";
mkdirSync(dir, { recursive: true });
const elements = [];
const rect = (name, x, y, width, height, color, radius = 0) =>
  elements.push({ name, type: "RECTANGLE", x, y, width, height, color, radius });
const text = (name, value, x, y, width, height, size, color = "#172B23", weight = "Regular") =>
  elements.push({ name, type: "TEXT", text: value, x, y, width, height, size, color, weight });
const green = "#255D46",
  ink = "#172B23",
  muted = "#68756C",
  mint = "#D9F0B4",
  paper = "#F6F7F2";
rect("Nav / Mark", 88, 45, 25, 25, green, 7);
rect("Nav / Mark line", 94, 52, 13, 3, mint, 1);
rect("Nav / Mark stem", 99, 52, 3, 13, mint, 1);
text("Nav / Wordmark", "figtether", 124, 40, 220, 38, 28, ink, "Bold");
text("Nav / Product", "How it works", 755, 48, 128, 24, 15);
text("Nav / Docs", "Documentation", 915, 48, 145, 24, 15);
text("Nav / GitHub", "GitHub ↗", 1100, 48, 100, 24, 15);
rect("Nav / CTA", 1230, 36, 122, 44, ink, 22);
text("Nav / CTA label", "Get started", 1250, 48, 95, 22, 14, "#FFFFFF", "Medium");
rect("Hero / Eyebrow background", 88, 170, 276, 34, "#E5EEDF", 17);
text(
  "Hero / Eyebrow",
  "LOCAL BY DESIGN. CONNECTED BY YOU.",
  104,
  179,
  250,
  20,
  11,
  green,
  "Medium",
);
text("Hero / Title 1", "Design context.", 88, 240, 650, 100, 76, ink, "Bold");
text("Hero / Title 2", "Right where", 88, 331, 650, 100, 76, ink, "Bold");
text("Hero / Title 3", "you build.", 88, 422, 650, 100, 76, green, "Bold");
text(
  "Hero / Description",
  "Bring your Figma canvas into your agent’s workflow.\nA local bridge. A shared context. A little less friction.",
  92,
  546,
  600,
  65,
  18,
  muted,
);
rect("Hero / Primary CTA", 88, 650, 206, 56, green, 28);
text("Hero / Primary label", "Connect your canvas  →", 108, 669, 190, 24, 14, "#FFFFFF", "Medium");
text("Hero / Secondary CTA", "See how it works  ↗", 323, 669, 225, 24, 15, ink, "Medium");
rect("Hero / Local status dot", 92, 738, 7, 7, green, 3);
text("Hero / Local note", "Your machine. Your files. Your flow.", 110, 731, 400, 24, 13, muted);
// The product illustration is drawn as editable rectangles and text.
rect("Preview / Outer", 790, 216, 562, 510, "#E7EBE2", 26);
text(
  "Preview / Caption",
  "ONE BRIDGE. TWO SIDES OF YOUR WORKFLOW.",
  816,
  241,
  500,
  20,
  11,
  muted,
  "Medium",
);
rect("Preview / Editor", 816, 280, 510, 382, "#1D2E26", 16);
rect("Preview / Toolbar", 816, 280, 510, 44, "#304238", 16);
for (let i = 0; i < 3; i++)
  rect(`Preview / Dot ${i + 1}`, 836 + i * 15, 298, 6, 6, ["#E4B69B", "#CFD7B1", "#9BC899"][i], 3);
text("Preview / Window title", "figtether.local", 1000, 292, 180, 24, 12, "#D2DECF");
rect("Preview / Canvas", 834, 344, 188, 236, "#F6F7F2", 9);
rect("Preview / Canvas bar", 848, 361, 160, 5, "#DEE4DA", 2);
rect("Preview / Canvas logo", 848, 380, 18, 18, green, 4);
text("Preview / Mini headline", "A little more\nconnected.", 848, 415, 165, 62, 22, ink, "Bold");
rect("Preview / Mini CTA", 848, 490, 90, 20, green, 10);
text("Preview / Mini CTA label", "Get started", 862, 495, 80, 16, 8, "#FFFFFF", "Medium");
for (let i = 0; i < 3; i++)
  rect(
    `Preview / Mini card ${i + 1}`,
    848 + i * 54,
    528,
    46,
    35,
    ["#D7E9C4", "#DFE5D7", "#D7E9C4"][i],
    5,
  );
text("Preview / Agent label", "AGENT CONTEXT", 1044, 347, 250, 22, 11, "#9DBAA7", "Medium");
text(
  "Preview / Agent code",
  "> get_nodes(selection)\n\n  source   plugin_live\n  frame    Homepage\n  nodes    ready\n  access   your scope\n\n✓ Context received.",
  1044,
  389,
  270,
  185,
  12,
  "#D9F0B4",
);
rect("Preview / Connection rule", 834, 605, 474, 1, "#43554A");
rect("Preview / Connected dot", 839, 626, 6, 6, mint, 3);
text("Preview / Connected", "Connected locally", 852, 618, 230, 24, 12, "#C4D5C8");
text("Preview / Scope", "Scope is yours", 1175, 618, 135, 24, 11, "#9DBAA7");
text(
  "Preview / Below caption",
  "Figma Design  ↔  FigTether  ↔  Your agent",
  891,
  685,
  440,
  24,
  13,
  green,
  "Medium",
);
rect("Section / Divider", 88, 833, 1264, 1, "#DDE3D8");
text("Section / Kicker", "LESS HANDOFF. MORE MOMENTUM.", 88, 890, 550, 24, 12, green, "Medium");
text(
  "Section / Heading",
  "A shorter path from\ncanvas to code.",
  88,
  932,
  640,
  120,
  43,
  ink,
  "Bold",
);
text(
  "Section / Intro",
  "Keep the details that matter close to the work.\nShare a clear slice of your design, then let your\nagent ask for the context it needs.",
  825,
  937,
  530,
  90,
  17,
  muted,
);
const cards = [
  [
    "01",
    "Start with your canvas",
    "Select a frame in Figma. Share the scope\nyou want your agent to work with.",
  ],
  [
    "02",
    "Stay close to the source",
    "Read structure, text, styles, and layout\nfrom your running editor session.",
  ],
  [
    "03",
    "Keep the controls",
    "Review a proposed change before it\nreaches your design. Undo when needed.",
  ],
];
cards.forEach(([number, title, body], i) => {
  const x = 88 + i * 430;
  rect(`Feature ${number} / Card`, x, 1100, 404, 216, "#FFFFFF", 18);
  rect(`Feature ${number} / Badge`, x + 24, 1124, 36, 30, "#E9F1E2", 8);
  text(`Feature ${number} / Number`, number, x + 33, 1132, 30, 22, 11, green, "Medium");
  text(`Feature ${number} / Title`, title, x + 24, 1177, 370, 35, 22, ink, "Bold");
  text(`Feature ${number} / Body`, body, x + 24, 1230, 370, 65, 15, muted);
});
rect("Closing / Background", 88, 1366, 1264, 150, ink, 24);
text("Closing / Title", "Keep your workflow in flow.", 122, 1401, 850, 52, 34, "#F6F7F2", "Bold");
text(
  "Closing / Description",
  "An open-source bridge for the way you already work.",
  124,
  1461,
  900,
  26,
  15,
  "#BDCCBD",
);
rect("Closing / CTA", 1096, 1412, 220, 56, mint, 28);
text("Closing / CTA label", "Build with FigTether  ↗", 1117, 1430, 195, 26, 14, ink, "Medium");
text("Footer / Wordmark", "figtether", 88, 1550, 230, 26, 16, ink, "Bold");
text("Footer / Note", "Local Figma context for AI agents.", 294, 1553, 600, 24, 12, muted);
text("Footer / Links", "Documentation     GitHub ↗", 1132, 1553, 260, 24, 12, muted);

const stateFile = `${dir}/website-state.json`;
const state = existsSync(stateFile)
  ? JSON.parse(readFileSync(stateFile, "utf8"))
  : { session, root: "2:2", elements, created: [], completed: [] };
if (state.session !== session)
  throw new Error("Website state belongs to another session; review before resuming.");
writeFileSync(
  `${dir}/website-fixture.json`,
  JSON.stringify({ width: 1440, height: 1640, root: "2:2", background: paper, elements }, null, 2),
);
function execute(label, operations) {
  if (state.completed.includes(label)) return;
  const plan = {
    plan_id: label,
    destructive: false,
    intent: "Create the FigTether website landing page in the granted test frame.",
    operations,
  };
  const spec = {
    binary,
    output,
    label: `${label}-preview`,
    quiet: true,
    calls: [{ name: "preview_write", arguments: { session_id: session, plan } }],
  };
  const specFile = `${dir}/website-step.json`;
  writeFileSync(specFile, JSON.stringify(spec));
  execFileSync(process.execPath, ["scripts/validation/mcp-run.mjs", specFile], {
    stdio: "inherit",
    windowsHide: true,
  });
  const preview = JSON.parse(readFileSync(`${output}/last-response.json`, "utf8"));
  if (preview.blocked || !preview.approval_id)
    throw new Error(`Preview refused: ${JSON.stringify(preview.reasons ?? preview)}`);
  execFileSync(
    "powershell",
    [
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      "scripts/validation/figma-window.ps1",
      "-Action",
      "Invoke",
      "-Name",
      "Approve the plan",
    ],
    { stdio: "inherit", windowsHide: true },
  );
  writeFileSync(
    specFile,
    JSON.stringify({
      binary,
      output,
      label: `${label}-apply`,
      quiet: true,
      apply: { session_id: session, plan, approval_id: preview.approval_id },
    }),
  );
  execFileSync(process.execPath, ["scripts/validation/mcp-run.mjs", specFile], {
    stdio: "inherit",
    windowsHide: true,
  });
  const applied = JSON.parse(readFileSync(`${output}/last-response.json`, "utf8"));
  if (applied.status !== "applied" || applied.applied_operations !== operations.length)
    throw new Error(`Apply incomplete: ${JSON.stringify(applied)}`);
  state.created.push(...(applied.created ?? []));
  state.completed.push(label);
  writeFileSync(stateFile, JSON.stringify(state, null, 2));
  console.log(`Completed ${label}: ${operations.length} operations`);
}
execute("website-base-v1", [
  { kind: "set_properties", node_id: "2:2", properties: { height: 1640 } },
  { kind: "set_fills", node_id: "2:2", fills: [{ type: "SOLID", color: paper }] },
]);
// Keep both the operation and property-change budgets intact.
for (let start = 0; start < elements.length; start += 24) {
  execute(
    `website-create-${start}-v1`,
    elements.slice(start, start + 24).map((e) => ({
      kind: "create_node",
      parent_id: "2:2",
      node_type: e.type,
      properties: {
        name: e.name,
        x: e.x,
        y: e.y,
        width: e.width,
        height: e.height,
        ...(e.type === "TEXT" ? { fontSize: e.size } : { cornerRadius: e.radius }),
      },
    })),
  );
}
// Teach the daemon the descendants before proposing changes to their real IDs.
const learnSpec = {
  binary,
  output,
  label: "website-learn-descendants",
  quiet: true,
  calls: [
    {
      name: "list_nodes",
      arguments: {
        session_id: session,
        root_id: "2:2",
        depth: 1,
        limit: 100,
        fields: ["id", "type", "name", "parent_id"],
      },
    },
  ],
};
writeFileSync(`${dir}/website-learn.json`, JSON.stringify(learnSpec));
execFileSync(process.execPath, ["scripts/validation/mcp-run.mjs", `${dir}/website-learn.json`], {
  stdio: "inherit",
  windowsHide: true,
});
const styles = [];
elements.forEach((e, i) => {
  const id = state.created[i];
  if (!id) throw new Error(`Missing created node ${i}`);
  styles.push({ kind: "set_fills", node_id: id, fills: [{ type: "SOLID", color: e.color }] });
  if (e.type === "TEXT")
    styles.push({
      kind: "set_text",
      node_id: id,
      text: e.text,
      font_family: "Inter",
      font_style: e.weight,
    });
});
for (let start = 0; start < styles.length; start += 45)
  execute(`website-style-${start}-v1`, styles.slice(start, start + 45));
console.log(
  JSON.stringify({ website_nodes: elements.length, root: state.root, completed: state.completed }),
);
