import { test } from "node:test";
import assert from "node:assert/strict";
import { selectManifestPort } from "../plugins/figma/manifest-config.mjs";

const manifest = (domains) => ({ networkAccess: { devAllowedDomains: domains } });
test("the WebSocket endpoint selects the bundle port", () => {
  assert.equal(
    selectManifestPort(manifest(["http://localhost:1234", "ws://localhost:39029"])),
    39029,
  );
});
test("an environment variable cannot move the bundle away from its manifest", () => {
  const source = manifest(["ws://localhost:39029"]);
  assert.equal(selectManifestPort(source, "39029"), 39029);
  assert.throws(() => selectManifestPort(source, "38947"), /disagrees/);
});
test("missing, remote, zero, overflowing and ambiguous ports fail before bundling", () => {
  for (const domains of [
    [],
    ["ws://example.com:1234"],
    ["ws://localhost:0"],
    ["ws://localhost:65536"],
    ["ws://localhost:39029", "ws://localhost:0"],
    ["ws://localhost:39029", "ws://localhost:65536"],
    ["ws://localhost:1234", "ws://localhost:1235"],
  ])
    assert.throws(() => selectManifestPort(manifest(domains)), /one ws/);
});
