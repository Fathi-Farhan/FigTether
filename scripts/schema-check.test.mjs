import assert from "node:assert/strict";
import { test } from "node:test";
import { copyFileSync, mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

// Exercise the real shell comparator in an isolated miniature checkout. The fixture
// generator supplies controlled output; no Cargo source or schema in the repo is edited.
for (const scenario of [
  "matching",
  "missing-generator",
  "empty-committed",
  "empty-generated",
  "drift",
]) {
  test(`schema gate: ${scenario}`, () => {
    const fixture = mkdtempSync(path.join(os.tmpdir(), "figtether-schema-test-"));
    const resolved = realpathSync(fixture);
    assert.equal(path.dirname(resolved), realpathSync(os.tmpdir()));
    try {
      for (const dir of ["scripts", "crates/model/examples", "schemas", "bin"])
        mkdirSync(path.join(fixture, dir), { recursive: true });
      copyFileSync("scripts/schema-check.sh", path.join(fixture, "scripts/schema-check.sh"));
      if (scenario !== "missing-generator")
        writeFileSync(
          path.join(fixture, "crates/model/examples/export_schemas.rs"),
          "// fixture\n",
        );
      if (scenario !== "empty-committed")
        writeFileSync(path.join(fixture, "schemas/test.schema.json"), '{"type":"object"}\n');
      const generator = [
        "#!/bin/sh",
        "for arg do output=$arg; done",
        'if [ "$SCHEMA_FIXTURE" != empty-generated ]; then',
        '  if [ "$SCHEMA_FIXTURE" = drift ]; then',
        '    printf \'%s\\n\' \'{"type":"string"}\' > "$output/test.schema.json"',
        "  else",
        '    printf \'%s\\n\' \'{"type":"object"}\' > "$output/test.schema.json"',
        "  fi",
        "fi",
        "",
      ].join("\n");
      writeFileSync(path.join(fixture, "bin/cargo"), generator, { mode: 0o755 });
      const shell = process.env.FIGTETHER_TEST_SH ?? "sh";
      const run = spawnSync(
        shell,
        [
          "-c",
          'fixture=$(cd "$1" && pwd) && PATH="$fixture/bin:$PATH" sh "$fixture/scripts/schema-check.sh"',
          "schema-fixture",
          fixture.replaceAll("\\", "/"),
        ],
        {
          encoding: "utf8",
          windowsHide: true,
          env: {
            ...process.env,
            SCHEMA_FIXTURE: scenario,
            FIGTETHER_SCHEMA_EXAMPLE: "export_schemas",
            FIGTETHER_SCHEMA_DIR: "schemas",
            FIGTETHER_SCHEMA_PACKAGE: "",
          },
        },
      );
      assert.equal(run.error, undefined);
      assert.equal(run.status, scenario === "matching" ? 0 : 1, run.stdout + run.stderr);
      assert.match(
        run.stdout + run.stderr,
        scenario === "matching" ? /schemas match/ : /schema check:.*(?:failed|differs)/,
      );
    } finally {
      // The resolved target was verified above to be a direct child of the temp root.
      rmSync(resolved, { recursive: true, force: true });
    }
  });
}
