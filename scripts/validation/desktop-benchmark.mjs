import { writeFileSync, mkdirSync } from "node:fs";
import { execFileSync } from "node:child_process";
const [session, binary, output] = process.argv.slice(2);
if (!session || !binary || !output)
  throw new Error(
    "Usage: node scripts/validation/desktop-benchmark.mjs SESSION_ID BINARY UNIQUE_RUN_DIRECTORY",
  );
mkdirSync(output, { recursive: true });
const root = process.env.FIGTETHER_DESKTOP_ROOT ?? "2:2";
const cached = Number(process.env.FIGTETHER_DESKTOP_CACHED_READS ?? 10000);
const fresh = Number(process.env.FIGTETHER_DESKTOP_FRESH_READS ?? 1000);
const exports = Number(process.env.FIGTETHER_DESKTOP_EXPORTS ?? 100);
for (const count of [cached, fresh, exports])
  if (!Number.isSafeInteger(count) || count < 1)
    throw new Error("Workload counts must be positive integers.");
const metadata = { session_id: session, root_id: root, depth: 0, fields: ["id", "name", "type"] };
const cacheArgs = { ...metadata, consistency: "cached", max_age_ms: 600000 };
const spec = {
  binary,
  output,
  label: "release-live-benchmark",
  quiet: true,
  calls: [
    { name: "list_nodes", lane: "W1-warmup", expected_nodes: 1, arguments: metadata },
    { name: "list_nodes", lane: "W1-cache-prime", arguments: cacheArgs },
    {
      name: "list_nodes",
      lane: "W1-cached-root",
      repeat: cached,
      expected_nodes: 1,
      arguments: cacheArgs,
    },
    {
      name: "list_nodes",
      lane: "W2-fresh-72-nodes",
      repeat: fresh,
      expected_nodes: 72,
      arguments: {
        session_id: session,
        root_id: root,
        depth: 2,
        limit: 100,
        fields: ["id", "name", "type", "visible"],
        consistency: "fresh",
      },
    },
    {
      name: "export_assets",
      lane: "W3-website-png-1440x1640",
      repeat: exports,
      arguments: { session_id: session, node_ids: [root], format: "png", scale: 1 },
    },
  ],
};
const file = `${output}/workload.json`;
writeFileSync(file, JSON.stringify(spec, null, 2));
writeFileSync(`${output}/phase.txt`, "mixed-benchmark");
execFileSync(process.execPath, ["scripts/validation/mcp-run.mjs", file], {
  stdio: "inherit",
  windowsHide: true,
});
writeFileSync(`${output}/sampling.stop`, "completed");
