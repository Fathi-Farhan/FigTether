"use strict";
// No cookies, browser storage, installation credential, or design data in URLs.
let capability = location.hash.slice(1);
history.replaceState(null, "", location.pathname);
let model = null;
let busy = false;
let refreshing = false;
let failures = 0;
let candidateKey = "";
const element = (id) => document.getElementById(id);
const text = (id, value) => {
  if (element(id).textContent !== value) element(id).textContent = value;
};
const controls = () => document.querySelectorAll("button, input");
const headers = () => ({ Authorization: `Bearer ${capability}` });
function offline(message) {
  text("connection", message);
  controls().forEach((node) => {
    node.disabled = true;
  });
}
async function command(command, value = null) {
  if (busy || !model) return;
  busy = true;
  try {
    const response = await fetch("/panel/action", {
      method: "POST",
      cache: "no-store",
      signal: AbortSignal.timeout(4000),
      headers: { ...headers(), "Content-Type": "application/json" },
      body: JSON.stringify({ command, value, grant_epoch: model.grantEpoch }),
    });
    if (!response.ok)
      throw Error("The control was not accepted. Reopen this window from the Figma plugin.");
    text("feedback", "Sent to Figma. Waiting for the plugin to confirm.");
    await new Promise((resolve) => setTimeout(resolve, 180));
    await refresh();
  } catch (error) {
    text("feedback", error.message);
  } finally {
    busy = false;
  }
}
function render(next, logs) {
  model = next;
  controls().forEach((node) => {
    node.disabled = false;
  });
  text("connection", next.nextStepText || "Connected to your Figma plugin.");
  text("shared", next.sharedFrameNames?.join(", ") || "Nothing shared yet.");
  text(
    "editing-state",
    next.writeGrantEnabled
      ? "Your agent can read and edit these frames."
      : "Your agent can read shared frames. Editing is off.",
  );
  element("refresh").disabled = next.connection !== "connected";
  element("share").disabled = !next.canGrant;
  element("editing").disabled = !next.canGrantWrite;
  element("stop").disabled = !next.canRevoke;
  element("stop-confirmation").hidden = !next.revokeConfirmVisible;
  text("editing", next.writeGrantEnabled ? "Turn editing off" : "Allow editing");
  element("auto").checked = next.autoMode === true;
  element("auto").disabled = next.canUseAutoMode !== true;
  text(
    "auto-state",
    next.autoModeReason ??
      (next.autoMode
        ? "On. New warning-free plans may be approved automatically."
        : "Off. You review each change."),
  );
  const signature = JSON.stringify(next.candidates || []);
  if (candidateKey !== signature) {
    candidateKey = signature;
    element("candidates").replaceChildren();
    for (const [index, frame] of (next.candidates || []).entries()) {
      const item = document.createElement("li");
      const label = document.createElement("label");
      label.className = "candidate";
      const box = document.createElement("input");
      box.type = "checkbox";
      box.value = frame.node_id;
      const name = document.createElement("span");
      name.textContent = frame.name || `Unnamed frame ${index + 1}`;
      box.addEventListener("change", () =>
        command(
          "select",
          [...document.querySelectorAll("#candidates input:checked")].map((x) => x.value),
        ),
      );
      label.append(box, name);
      item.append(label);
      element("candidates").append(item);
    }
  }
  document.querySelectorAll("#candidates input").forEach((box) => {
    box.checked = (next.selectedRootIds || []).includes(box.value);
  });
  element("selection-empty").hidden = (next.candidates?.length || 0) > 0;
  const preview = next.writePreview;
  element("proposal").hidden = !preview;
  if (preview) {
    text("intent", preview.intent || "Your agent proposed a change.");
    text(
      "counts",
      `Create ${preview.counts.create} · Change ${preview.counts.change} · Move ${preview.counts.move} · Remove ${preview.counts.remove}`,
    );
    text(
      "decision",
      preview.blocked
        ? "Blocked. Ask your agent to revise this plan."
        : next.writeDecision === "approved"
          ? next.writeApprovedAutomatically
            ? "Auto mode approved this warning-free plan. Waiting for the agent to apply it."
            : "Approved. Waiting for the agent to apply this plan."
          : next.writeDecision === "rejected"
            ? "Rejected."
            : "Review this exact plan before approving.",
    );
    element("warnings").replaceChildren();
    for (const warning of [
      ...new Set([
        ...(preview.warnings || []),
        ...(preview.blocked ? preview.blockedReasons || [] : []),
      ]),
    ]) {
      const li = document.createElement("li");
      li.textContent = warning;
      element("warnings").append(li);
    }
    element("approve").disabled = !next.canApproveWrite;
    element("reject").disabled = !next.canRejectWrite;
  }
  element("activity").replaceChildren();
  for (const line of (logs || []).slice(-60)) {
    const match =
      /^(\d{2}:\d{2}:\d{2}) op=([a-z_]+)(?: count=(\d+))?(?: duration=(\d+)ms)?(?: error=([A-Z_]+))?$/.exec(
        line,
      );
    if (!match) continue;
    const li = document.createElement("li");
    li.textContent = `${match[1]}  ${match[2].replaceAll("_", " ")}${match[3] ? ` · ${match[3]} items` : ""}${match[4] ? ` · ${match[4]} ms` : ""}${match[5] ? ` · ${match[5]}` : ""}`;
    element("activity").append(li);
  }
  text("feedback", "Connected. Changes to sharing are confirmed by the Figma plugin.");
}
async function refresh() {
  if (refreshing || !capability) return;
  refreshing = true;
  try {
    const response = await fetch("/panel/state", {
      headers: headers(),
      cache: "no-store",
      signal: AbortSignal.timeout(4000),
    });
    if (response.status === 401) {
      capability = "";
      offline("This window expired or the plugin reconnected. Open a new window from Figma.");
      return;
    }
    if (!response.ok) throw Error("offline");
    const snapshot = await response.json();
    if (snapshot?.model) {
      render(snapshot.model, snapshot.logs);
      failures = 0;
    } else offline("Waiting for the Figma plugin to publish its state…");
  } catch {
    failures++;
    if (failures >= 2)
      offline("Connection lost. Check that Figma and the local bridge are running.");
  } finally {
    refreshing = false;
  }
}
element("refresh").addEventListener("click", () => command("refresh"));
element("share").addEventListener("click", () => command("share"));
element("editing").addEventListener("click", () => command("editing"));
for (const action of ["stop", "stop_confirm", "stop_cancel"])
  element(action).addEventListener("click", () => command(action));
element("auto").addEventListener("change", () => command("auto", element("auto").checked));
for (const action of ["approve", "reject"])
  element(action).addEventListener("click", () =>
    command(action, {
      planHash: model?.writePreview?.planHash,
      approvalId: model?.writeApprovalId,
    }),
  );
if (!capability) offline("Open this window using Open outside Figma in the FigTether plugin.");
else {
  refresh();
  setInterval(refresh, 1000);
}
