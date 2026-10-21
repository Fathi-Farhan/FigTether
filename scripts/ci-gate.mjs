import { appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

// A skipped, cancelled, missing or unknown result must never become a green gate.
export function assessNeeds(needs) {
  if (!needs || typeof needs !== "object" || Array.isArray(needs)) {
    throw new Error("Expected the GitHub Actions needs object");
  }
  const entries = Object.entries(needs).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  if (entries.length === 0) throw new Error("No prerequisite results were supplied");
  return entries.map(([job, value]) => {
    if (!/^[a-zA-Z_][a-zA-Z0-9_-]*$/.test(job)) throw new Error("Invalid job identifier");
    const observed = value?.result;
    const result = ["success", "failure", "cancelled", "skipped"].includes(observed)
      ? observed
      : "missing-or-unknown";
    return { job, result, passed: result === "success" };
  });
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const results = assessNeeds(JSON.parse(process.env.CI_NEEDS_JSON ?? "null"));
    const passed = results.every((result) => result.passed);
    const summary = [
      "## Pull request quality checks",
      "",
      "| Job | Result |",
      "| --- | --- |",
      ...results.map(({ job, result }) => `| ${job} | ${result} |`),
      "",
      passed
        ? "All configured checks succeeded."
        : "Merge gate failed: a prerequisite did not succeed.",
      "Automated checks cover their defined cases; maintainers still review correctness and scope.",
      "",
    ].join("\n");
    console.log(summary);
    if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary);
    process.exitCode = passed ? 0 : 1;
  } catch (error) {
    // Never echo the raw environment payload into logs.
    console.error("CI gate could not validate prerequisite results:", error.message);
    process.exitCode = 1;
  }
}
