import { readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import path from "node:path";
import { build } from "../../../plugins/figma/node_modules/esbuild/lib/main.js";
import { browser } from "./browser.mjs";
await build({
  entryPoints: ["scripts/validation/bridge-ux/fixture.ts"],
  bundle: true,
  platform: "browser",
  format: "iife",
  outfile: "tmp/bridge-ux/fixture.js",
  alias: { "@figtether/shared": path.resolve("plugins/figma/shared/src/index.ts") },
});
let html = readFileSync("plugins/figma/ui/index.html", "utf8");
html = html
  .replace(
    /<link[^>]*href="[^"]*(?:styles|ui)\.css"[^>]*>/,
    `<style>${readFileSync("plugins/figma/ui/styles.css", "utf8")}</style>`,
  )
  .replace(/<script[^>]*src="[^"]*ui\.js"[^>]*><\/script>/, '<script src="./fixture.js"></script>');
html = html.replace(
  'src="./assets/figtether-icon.png"',
  `src="data:image/png;base64,${readFileSync("plugins/figma/ui/assets/figtether-icon.png").toString("base64")}"`,
);
writeFileSync("tmp/bridge-ux/fixture.html", html);
const b = await browser(),
  observations = [];
const check = async (name, expression) => {
  const ok = await b.eval(expression);
  observations.push({ name, ok });
  if (!ok) throw Error(name);
};
try {
  await b.send("Page.navigate", {
    url: pathToFileURL(path.resolve("tmp/bridge-ux/fixture.html")).href,
  });
  await new Promise((r) => setTimeout(r, 250));
  await b.capture("tmp/bridge-ux/03-after-unpaired.png");
  await b.eval("fixture.connect()");
  await b.capture("tmp/bridge-ux/04-frame-selection.png");
  await check(
    "named frame is visible",
    'document.querySelector(".candidate").textContent.includes("Website Test")',
  );
  await b.eval('document.getElementById("allow-button").click()');
  await check(
    "sharing confirmed by name",
    'document.getElementById("shared-frames").textContent === "Website Test"',
  );
  await b.eval('document.getElementById("write-grant-button").click()');
  await b.capture("tmp/bridge-ux/05-shared.png");
  await b.eval('document.getElementById("auto-mode").click();fixture.preview()');
  await check(
    "clean new plan auto approved",
    "fixture.approvals().length===1 && fixture.approvals()[0].automatic===true",
  );
  await b.eval('fixture.preview(["Font unavailable. Choose a font installed in Figma."])');
  await check(
    "warning pauses auto mode",
    '!document.getElementById("auto-mode").checked && fixture.approvals().length===1',
  );
  await b.eval('document.getElementById("write-approval-panel").scrollIntoView()');
  await b.capture("tmp/bridge-ux/06-warning.png");
  await b.eval('fixture.logs();document.getElementById("log-list").scrollIntoView()');
  await b.capture("tmp/bridge-ux/07-activity.png");
  await check(
    "activity has no numbered markers",
    'getComputedStyle(document.getElementById("log-list")).listStyleType === "none" && document.getElementById("log-list").children.length===200',
  );
  await b.eval('document.getElementById("clear-log-button").click()');
  await check(
    "activity clear and count agree",
    'document.getElementById("log-list").children.length===0 && document.getElementById("log-count").textContent==="0 events"',
  );
  await b.eval(
    'window.scrollTo(0,0);document.querySelector(".candidate input").focus();document.querySelector(".candidate input").click()',
  );
  await check(
    "checkbox focus survives render",
    'document.activeElement === document.querySelector(".candidate input")',
  );
  for (const width of [320, 440, 740]) {
    await b.send("Emulation.setDeviceMetricsOverride", {
      width,
      height: 950,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await check(
      `no horizontal overflow at ${width}px`,
      "document.documentElement.scrollWidth <= window.innerWidth",
    );
  }
  await b.send("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-color-scheme", value: "light" }],
  });
  await b.capture("tmp/bridge-ux/08-light.png", 440, 950);
  writeFileSync(
    "tmp/bridge-ux/browser-results.json",
    JSON.stringify(
      {
        kind: "controlled-browser-fixture",
        chrome: (await b.send("Browser.getVersion")).product,
        observations,
        limits: [
          "Synthetic plugin/daemon ports, not a native Figma connection",
          "Screenshots establish appearance; interaction checks use actual DOM and production controller with test ports",
          "No real design mutations or approval clicks in Figma",
        ],
      },
      null,
      2,
    ),
  );
  console.log(JSON.stringify(observations));
} finally {
  writeFileSync(
    "tmp/bridge-ux/browser-observations-last.json",
    JSON.stringify(observations, null, 2),
  );
  await b.close();
}
