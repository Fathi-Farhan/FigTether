// Evaluate collectBrowser.toString() inside the page being reviewed.
// Collects visible rendered DOM observations, never input values.
export function collectBrowser(options = {}) {
  const limit = Math.min(10000, Math.max(1, options.limit ?? 5000));
  const all = [...document.body.querySelectorAll("*")];
  const truncated = all.length > limit;
  const controls =
    'button,input:not([type="hidden"]),select,textarea,summary,a[href],[role="button"],[role="link"],[role="checkbox"],[role="switch"],[role="tab"]';
  const skip = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEMPLATE"]);
  const cssPath = (element) => {
    const parts = [];
    for (let e = element; e && e !== document.body; e = e.parentElement) {
      const tag = e.tagName.toLowerCase(),
        siblings = e.parentElement
          ? [...e.parentElement.children].filter((x) => x.tagName === e.tagName)
          : [e];
      parts.unshift(tag + ":nth-of-type(" + (siblings.indexOf(e) + 1) + ")");
    }
    return "body>" + parts.join(">");
  };
  const background = (element) => {
    // Ancestor opacity/filter/blending affects even an opaque child background.
    for (let e = element; e; e = e.parentElement) {
      const s = getComputedStyle(e);
      if (
        Number(s.opacity) !== 1 ||
        s.filter !== "none" ||
        s.mixBlendMode !== "normal" ||
        (s.backdropFilter && s.backdropFilter !== "none")
      )
        return null;
    }
    for (let e = element; e; e = e.parentElement) {
      const s = getComputedStyle(e);
      if (
        Number(s.opacity) !== 1 ||
        s.backgroundImage !== "none" ||
        s.filter !== "none" ||
        s.mixBlendMode !== "normal" ||
        (s.backdropFilter && s.backdropFilter !== "none")
      )
        return null;
      for (const pseudo of ["::before", "::after"]) {
        const p = getComputedStyle(e, pseudo);
        if (
          p.content &&
          p.content !== "none" &&
          p.content !== "normal" &&
          (p.backgroundImage !== "none" ||
            !["rgba(0, 0, 0, 0)", "transparent"].includes(p.backgroundColor))
        )
          return null;
      }
      const c = s.backgroundColor;
      if (/^rgb\(/.test(c) || /^rgba\([^,]+,[^,]+,[^,]+,\s*1\)$/.test(c)) return c;
      if (c !== "rgba(0, 0, 0, 0)" && c !== "transparent") return null;
    }
    return null;
  };
  const nodes = [];
  let hasShadow = false,
    hasFrame = false;
  for (const el of all.slice(0, limit)) {
    if (skip.has(el.tagName)) continue;
    if (el.shadowRoot) hasShadow = true;
    if (el.tagName === "IFRAME") hasFrame = true;
    const s = getComputedStyle(el),
      r = el.getBoundingClientRect();
    if (
      s.display === "none" ||
      s.visibility !== "visible" ||
      Number(s.opacity) === 0 ||
      r.width === 0 ||
      r.height === 0
    )
      continue;
    let hidden = false;
    for (let a = el.parentElement; a; a = a.parentElement) {
      if (Number(getComputedStyle(a).opacity) === 0) {
        hidden = true;
        break;
      }
      if (a.tagName === "DETAILS" && !a.open) {
        const summary = [...a.children].find((c) => c.tagName === "SUMMARY");
        if (!summary || !(summary === el || summary.contains(el))) {
          hidden = true;
          break;
        }
      }
    }
    if (hidden) continue;
    const direct = [...el.childNodes]
      .filter((n) => n.nodeType === 3)
      .map((n) => n.textContent)
      .join(" ")
      .replace(/\s+/g, " ")
      .trim();
    const control = el.matches(controls),
      graphic = el.tagName === "IMG" || el.tagName === "SVG";
    const surface =
      !control &&
      !graphic &&
      (el.matches("section,article,aside,fieldset") ||
        (parseFloat(s.borderTopWidth) > 0 && el.children.length > 0));
    if (!direct && !control && !graphic && !surface) continue;
    const disabled =
      Boolean(el.disabled) ||
      el.getAttribute("aria-disabled") === "true" ||
      Boolean(el.closest("[inert]"));
    const opacityUnknown =
      Number(s.opacity) !== 1 || s.textShadow !== "none" || s.filter !== "none";
    const n = {
      id: cssPath(el),
      role: control ? "control" : graphic ? "graphic" : surface ? "surface" : "text",
      visible: true,
      text: el.tagName === "INPUT" || el.tagName === "TEXTAREA" ? "" : direct,
      disabled,
      rect: { x: r.x, y: r.y, width: r.width, height: r.height },
      foreground: opacityUnknown ? null : s.color,
      background: opacityUnknown ? null : background(el),
      font_size: parseFloat(s.fontSize),
      font_weight:
        s.fontWeight === "bold" ? 700 : s.fontWeight === "normal" ? 400 : Number(s.fontWeight),
      border_radius: parseFloat(s.borderTopLeftRadius),
      background_image: s.backgroundImage,
      box_shadow: s.boxShadow,
    };
    if (!Number.isFinite(n.font_weight)) n.font_weight = null;
    if (!Number.isFinite(n.border_radius) || s.borderTopLeftRadius.includes("%"))
      n.border_radius = null;
    if (control) {
      n.name =
        el.getAttribute("aria-label") ||
        [...(el.labels ?? [])].map((l) => l.textContent?.trim() ?? "").join(" ") ||
        direct ||
        el.getAttribute("title") ||
        "";
      n.name_source = "heuristic";
    }
    if (graphic && el.getAttribute("alt") === "") n.contrast_role = "decorative";
    nodes.push(n);
  }
  const snapshot = {
    schema_version: 1,
    source: { kind: "dom", collector_version: "1.0.0" },
    capture: {
      complete: !truncated && !hasShadow && !hasFrame,
      limitations: [
        "Visible light-DOM observations in one state; no iframe or shadow-root traversal.",
        "Input values and password values are never read.",
        "Pseudo-elements, complex compositing, images and gradients are not resolved into text backgrounds.",
        "Accessible names are heuristic unless replaced with browser AX evidence.",
        "Focus, hover, error recovery, hidden states, and task success are not tested.",
      ],
      truncated,
    },
    viewport: {
      width: document.documentElement.clientWidth,
      height: document.documentElement.clientHeight,
      scroll_width: document.documentElement.scrollWidth,
    },
    nodes,
  };
  if (typeof options.reflowException === "boolean")
    snapshot.viewport.reflow_exception = options.reflowException;
  return snapshot;
}
