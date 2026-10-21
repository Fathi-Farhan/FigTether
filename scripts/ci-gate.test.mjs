import assert from "node:assert/strict";
import { test } from "node:test";
import { spawnSync } from "node:child_process";
import { assessNeeds } from "./ci-gate.mjs";

test("all successful prerequisites pass, independently of input ordering", () => {
  const needs = { tests: { result: "success" }, build: { result: "success" } };
  const results = assessNeeds(needs);
  assert.ok(results.every((r) => r.passed));
  assert.deepEqual(results, assessNeeds({ build: needs.build, tests: needs.tests }));
});

for (const result of ["failure", "cancelled", "skipped", "neutral", undefined]) {
  test(`a ${result ?? "missing"} prerequisite prevents success`, () => {
    const results = assessNeeds({ build: { result: "success" }, tests: { result } });
    assert.equal(
      results.every((r) => r.passed),
      false,
    );
  });
}

test("absent, empty and malformed result collections cannot pass", () => {
  for (const input of [null, undefined, [], {}, "success", 1]) {
    assert.throws(() => assessNeeds(input));
  }
  assert.equal(assessNeeds({ test: null })[0].passed, false);
});

test("job identifiers cannot inject summary markup", () => {
  assert.throws(() => assessNeeds({ "test | forged success": { result: "success" } }));
});

test("the command exits nonzero for malformed data and skipped prerequisites", () => {
  for (const payload of ["not-json", "{}", '{"test":{"result":"skipped"}}']) {
    const run = spawnSync(process.execPath, ["scripts/ci-gate.mjs"], {
      env: { ...process.env, CI_NEEDS_JSON: payload, GITHUB_STEP_SUMMARY: "" },
      encoding: "utf8",
    });
    assert.equal(run.status, 1);
  }
});

test("the command exits zero only for successful prerequisites", () => {
  const run = spawnSync(process.execPath, ["scripts/ci-gate.mjs"], {
    env: {
      ...process.env,
      CI_NEEDS_JSON: '{"test":{"result":"success"},"audit":{"result":"success"}}',
      GITHUB_STEP_SUMMARY: "",
    },
    encoding: "utf8",
  });
  assert.equal(run.status, 0);
  assert.match(run.stdout, /All configured checks succeeded/);
});
