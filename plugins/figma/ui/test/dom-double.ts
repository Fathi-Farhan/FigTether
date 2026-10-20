/**
 * A minimal DOM double.
 *
 * The test environment of the plugin has no browser, so the double implements
 * the small part of the document the interface of the plugin uses: the
 * elements by their id, the children of an element, the text of an element,
 * the hidden and the disabled state and the click handlers.
 *
 * The double is deliberately small. It exists so that the panels of the
 * interface can be tested without a browser, not so that a browser can be
 * emulated.
 */

/** One element of the document. */
export class FakeElement {
  /** The name of the tag. */
  readonly tagName: string;
  /** The identifier of the element. */
  readonly id: string;
  /** The children of the element. */
  readonly children: FakeElement[] = [];
  /** The handlers of the element, per event. */
  readonly listeners = new Map<string, ((event: unknown) => void)[]>();
  /** The text of the element. */
  textContent = "";
  /** Whether the element is hidden. */
  hidden = false;
  /** Whether the element is disabled. */
  disabled = false;
  /** Whether a checkbox is checked. */
  checked = false;
  /** The value of a field. */
  value = "";
  /** The type of a field. */
  type = "";
  /** The data attributes of the element. */
  readonly dataset: Record<string, string> = {};
  /** The parent of the element. */
  parent: FakeElement | null = null;
  /** Whether the element holds the focus. */
  focused = false;

  constructor(tagName: string, id = "") {
    this.tagName = tagName;
    this.id = id;
  }

  /** The first child of the element. */
  get firstChild(): FakeElement | null {
    return this.children[0] ?? null;
  }

  /** The number of children of the element. */
  get childElementCount(): number {
    return this.children.length;
  }

  /** Append a child to the element. */
  appendChild(child: FakeElement): FakeElement {
    child.parent = this;
    this.children.push(child);
    return child;
  }

  /** Remove a child from the element. */
  removeChild(child: FakeElement): FakeElement {
    const index = this.children.indexOf(child);
    if (index >= 0) {
      this.children.splice(index, 1);
    }
    child.parent = null;
    return child;
  }

  /** Register a handler for an event. */
  addEventListener(type: string, handler: (event: unknown) => void): void {
    const handlers = this.listeners.get(type) ?? [];
    handlers.push(handler);
    this.listeners.set(type, handlers);
  }

  /** Fire an event on the element. */
  dispatch(type: string): void {
    for (const handler of this.listeners.get(type) ?? []) {
      handler({ type });
    }
  }

  /** Fire a click on the element. */
  click(): void {
    this.dispatch("click");
  }

  /** Move the focus to the element. */
  focus(): void {
    this.focused = true;
  }

  /** The text of the children of the element. */
  get childText(): string {
    return this.children.map((child) => child.textContent).join("");
  }
}

/** A document that holds the elements by their id. */
export class FakeDocument {
  /** The elements of the document. */
  readonly elements = new Map<string, FakeElement>();

  constructor(ids: readonly string[]) {
    for (const id of ids) {
      this.elements.set(id, new FakeElement("div", id));
    }
  }

  /** The element with the given id, when the document holds one. */
  getElementById(id: string): FakeElement | null {
    return this.elements.get(id) ?? null;
  }

  /** Create an element that is not part of the document. */
  createElement(tagName: string): FakeElement {
    return new FakeElement(tagName);
  }
}

/** The identifiers of the markup of the interface. */
export function idsOf(html: string): string[] {
  const ids: string[] = [];
  const pattern = /\sid="([^"]+)"/g;
  let match = pattern.exec(html);
  while (match !== null) {
    ids.push(match[1] as string);
    match = pattern.exec(html);
  }
  return ids;
}
