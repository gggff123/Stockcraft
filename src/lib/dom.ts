/**
 * Minimal DOM helpers. All rendering goes through `html` (auto-escaping tagged template) or
 * `esc`, so remote text from providers can never be injected as markup.
 */

const ESCAPES: Record<string, string> = {
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
  "'": "&#39;",
};

export function esc(value: unknown): string {
  if (value === null || value === undefined) return "";
  return String(value).replace(/[&<>"']/g, (c) => ESCAPES[c]);
}

/** Marks a string as already-safe markup for `html`. Only use with literals or `esc()` output. */
export class Raw {
  constructor(readonly value: string) {}
  toString(): string {
    return this.value;
  }
}

export function raw(value: string | Raw): Raw {
  return value instanceof Raw ? value : new Raw(value);
}

/**
 * Tagged template that escapes every interpolated value unless it is `Raw`.
 *
 * The result is `Raw` itself, so nested templates (`html` inside `html`, or inside an array)
 * compose without being escaped a second time. `render` knows how to unwrap it.
 */
export function html(strings: TemplateStringsArray, ...values: unknown[]): Raw {
  let out = strings[0] ?? "";
  for (let i = 0; i < values.length; i++) {
    const v = values[i];
    if (v instanceof Raw) out += v.value;
    else if (Array.isArray(v)) out += v.map((x) => (x instanceof Raw ? x.value : esc(x))).join("");
    else out += esc(v);
    out += strings[i + 1] ?? "";
  }
  return new Raw(out);
}

/**
 * Copies text to the system clipboard.
 *
 * WebView2 can deny the async Clipboard API when the page is not focused or when the platform
 * permission prompt is unavailable, so fall back to a temporary selection copy.
 */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    /* fall through to the selection-based copy */
  }
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.setAttribute("aria-hidden", "true");
  area.style.position = "fixed";
  area.style.top = "0";
  area.style.left = "0";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.focus();
  area.select();
  area.setSelectionRange(0, area.value.length);
  try {
    return document.execCommand("copy");
  } catch {
    return false;
  } finally {
    area.remove();
  }
}

export function qs<T extends Element = HTMLElement>(selector: string, root: ParentNode = document): T | null {
  return root.querySelector<T>(selector);
}

export function qsa<T extends Element = HTMLElement>(selector: string, root: ParentNode = document): T[] {
  return Array.from(root.querySelectorAll<T>(selector));
}

/** Replaces a host element's content with rendered markup. */
export function render(host: Element, markup: string | Raw): void {
  host.innerHTML = markup instanceof Raw ? markup.value : markup;
}

export function configure(host: Element, attrs: Record<string, string | number | boolean | undefined | null>): void {
  for (const [key, value] of Object.entries(attrs)) {
    if (value === undefined || value === null || value === false) host.removeAttribute(key);
    else if (value === true) host.setAttribute(key, "");
    else host.setAttribute(key, String(value));
  }
}

type Handler = (element: HTMLElement, event: Event) => void;

export interface Actions {
  click?: Handler;
  change?: Handler;
  input?: Handler;
  submit?: Handler;
  keydown?: Handler;
  dblclick?: Handler;
}

const registry = new Map<string, Actions>();

export function registerActions(map: Record<string, Actions>): void {
  for (const [name, handlers] of Object.entries(map)) registry.set(name, handlers);
}

export function hasAction(name: string): boolean {
  return registry.has(name);
}

function closestAction(target: EventTarget | null): { name: string; element: HTMLElement } | null {
  if (!(target instanceof Element)) return null;
  const holder = target.closest<HTMLElement>("[data-action]");
  if (!holder) return null;
  // A data-stop-click container (for example a modal body inside a closing scrim) swallows the
  // action even if an outer element declares one.
  for (let node: Element | null = target; node && node !== holder; node = node.parentElement) {
    if (node.hasAttribute("data-stop-click")) return null;
  }
  const name = holder.dataset.action ?? "";
  return name ? { name, element: holder } : null;
}

function dispatch(kind: keyof Actions, event: Event): boolean {
  const found = closestAction(event.target);
  if (!found) return false;
  const handler = registry.get(found.name)?.[kind];
  if (!handler) return false;
  handler(found.element, event);
  return true;
}

export function initDelegation(root: Document = document): void {
  root.addEventListener("click", (event) => {
    if (!closestAction(event.target)) return;
    dispatch("click", event);
  });
  root.addEventListener("dblclick", (event) => void dispatch("dblclick", event));
  root.addEventListener("change", (event) => void dispatch("change", event));
  root.addEventListener("input", (event) => void dispatch("input", event));
  root.addEventListener("submit", (event) => {
    if (dispatch("submit", event)) event.preventDefault();
  });
  root.addEventListener("keydown", (event) => void dispatch("keydown", event));
}

export function focusFirst(selector: string): void {
  const el = qs<HTMLElement>(selector);
  el?.focus();
}

/** Re-renders the document while keeping keyboard focus (and caret position for text fields). */
export function preserveFocus(_host: Element, renderFn: () => void): void {
  const active = document.activeElement as HTMLElement | null;
  const id = active?.id ?? "";
  const isText = active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement;
  const selection = isText ? active.selectionStart : null;
  renderFn();
  if (!id) return;
  const next = qs<HTMLElement>(`#${id}`);
  if (!next) return;
  next.focus();
  if (selection !== null && (next instanceof HTMLInputElement || next instanceof HTMLTextAreaElement)) {
    try {
      next.setSelectionRange(selection, selection);
    } catch {
      /* not a text input */
    }
  }
}

export function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable;
}
