import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import prettier from "prettier";
const root = fileURLToPath(new URL("../", import.meta.url));
const workflow = JSON.parse(
  readFileSync(path.join(root, "integrations/figtether/workflow.json"), "utf8"),
);
const instructions = [workflow.introduction, ...workflow.steps, workflow.report].join(" ") + "\n";
const skill = await prettier.format(
  `---\nname: ${workflow.name}\ndescription: ${workflow.description}\n---\n\n# Design through FigTether\n\n${workflow.introduction}\n\n${workflow.steps.map((step, i) => `${i + 1}. ${step}`).join("\n\n")}\n\n${workflow.report}\n`,
  {
    ...(await prettier.resolveConfig(path.join(root, ".agents/skills/figtether-design/SKILL.md"))),
    parser: "markdown",
  },
);
const files = new Map([
  ["integrations/figtether/instructions.txt", instructions],
  ["integrations/figtether/SKILL.md", skill],
  [".agents/skills/figtether-design/SKILL.md", skill],
]);
let failures = 0;
for (const [relative, body] of files) {
  const file = path.join(root, relative);
  if (process.argv.includes("--check")) {
    let actual;
    try {
      actual = readFileSync(file, "utf8").replaceAll("\r\n", "\n");
    } catch {
      actual = null;
    }
    if (actual !== body) {
      console.error(`Workflow drift: ${relative}`);
      failures++;
    }
  } else {
    mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(file, body);
  }
}
if (failures) process.exitCode = 1;
else
  console.log(
    `Agent workflow ${process.argv.includes("--check") ? "current" : "generated"}: ${files.size} files`,
  );
