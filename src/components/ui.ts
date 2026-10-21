/** Shared UI primitives: buttons, chips, switches, empty/error states, toasts and confirm dialogs. */
import { html, qs, raw, render, type Raw } from "../lib/dom";
import { icon, type IconName } from "./icons";

export interface ButtonOptions {
  action: string;
  label: string;
  iconName?: IconName;
  variant?: "primary" | "ghost" | "quiet" | "danger" | "default";
  size?: "sm" | "md";
  title?: string;
  disabled?: boolean;
  pressed?: boolean;
  submit?: boolean;
  id?: string;
  data?: Record<string, string | number | boolean | null | undefined>;
  ariaLabel?: string;
}

function dataAttrs(data?: ButtonOptions["data"]): string {
  if (!data) return "";
  return Object.entries(data)
    .filter(([, v]) => v !== undefined && v !== null && v !== false)
    .map(([k, v]) => ` data-${k.toLowerCase()}="${String(v).replace(/"/g, "&quot;")}"`)
    .join("");
}

export function button(options: ButtonOptions): Raw {
  const variant = options.variant ?? "default";
  const classes = ["btn"];
  if (variant === "primary") classes.push("btn-primary");
  else if (variant === "ghost") classes.push("btn-ghost");
  else if (variant === "quiet") classes.push("btn-quiet");
  else if (variant === "danger") classes.push("btn-danger");
  if (options.size === "sm") classes.push("btn-sm");
  return raw(
    `<button type="${options.submit ? "submit" : "button"}" class="${classes.join(" ")}" data-action="${options.action}"` +
      (options.id ? ` id="${options.id}"` : "") +
      (options.title ? ` title="${options.title.replace(/"/g, "&quot;")}"` : "") +
      (options.ariaLabel ? ` aria-label="${options.ariaLabel.replace(/"/g, "&quot;")}"` : "") +
      (options.disabled ? " disabled" : "") +
      (options.pressed !== undefined ? ` aria-pressed="${options.pressed}"` : "") +
      dataAttrs(options.data) +
      `>${options.iconName ? icon(options.iconName, options.size === "sm" ? 15 : 17).value : ""}` +
      `<span>${options.label}</span></button>`,
  );
}

export interface IconButtonOptions {
  action: string;
  iconName: IconName;
  label: string;
  title?: string;
  pressed?: boolean;
  disabled?: boolean;
  size?: number;
  data?: Record<string, string | number | boolean | null | undefined>;
  className?: string;
  badge?: string | number;
}

export function iconButton(options: IconButtonOptions): Raw {
  return raw(
    `<button type="button" class="btn btn-icon${options.className ? ` ${options.className}` : ""}" ` +
      `data-action="${options.action}" aria-label="${options.label.replace(/"/g, "&quot;")}" ` +
      (options.title ? `title="${options.title.replace(/"/g, "&quot;")}" ` : "") +
      (options.pressed !== undefined ? `aria-pressed="${options.pressed}" ` : "") +
      (options.disabled ? "disabled " : "") +
      (options.badge !== undefined && options.badge !== 0 ? `data-badge="${options.badge}" ` : "") +
      dataAttrs(options.data) +
      `>${icon(options.iconName, options.size ?? 18).value}</button>`,
  );
}

export function chip(label: string, options: { action: string; pressed?: boolean; data?: Record<string, string | number> }): Raw {
  return raw(
    `<button type="button" class="chip" data-action="${options.action}"` +
      (options.pressed !== undefined ? ` aria-pressed="${options.pressed}"` : "") +
      dataAttrs(options.data) +
      `>${label}</button>`,
  );
}

export function removableChip(label: string, data: Record<string, string | number>): Raw {
  return raw(
    `<span class="chip" role="group">${label}` +
      `<button type="button" class="chip-remove" data-action="remove-filter" ${dataAttrs(data)} aria-label="Remove filter ${label}">` +
      `${icon("close", 13).value}</button></span>`,
  );
}

export function switchRow(label: string, checked: boolean, action: string, hint?: string, data?: Record<string, string | number>): Raw {
  return raw(
    `<label class="switch grow">` +
      `<input type="checkbox" data-action="${action}"${checked ? " checked" : ""}${dataAttrs(data)} />` +
      `<span class="track" aria-hidden="true"></span>` +
      `<span class="grow"><span class="tile-title">${label}</span>` +
      (hint ? `<div class="tiny muted">${hint}</div>` : "") +
      `</span></label>`,
  );
}

export function selectField(label: string, value: string, options: { value: string; label: string }[], action: string, data?: Record<string, string | number>): Raw {
  return raw(
    `<label class="field"><span class="field-label">${label}</span>` +
      `<select data-action="${action}"${dataAttrs(data)}>` +
      options
        .map((o) => `<option value="${o.value}"${o.value === value ? " selected" : ""}>${o.label}</option>`)
        .join("") +
      `</select></label>`,
  );
}

export function emptyState(options: {
  title: string;
  message: string;
  iconName?: IconName;
  actions?: Raw[];
  art?: Raw;
}): Raw {
  return raw(
    `<div class="state-block">` +
      (options.art ? `<div class="state-art">${options.art.value}</div>` : options.iconName ? `<div style="opacity:.5">${icon(options.iconName, 42).value}</div>` : "") +
      `<h2>${options.title}</h2><p>${options.message}</p>` +
      (options.actions?.length ? `<div class="state-actions">${options.actions.map((a) => a.value).join("")}</div>` : "") +
      `</div>`,
  );
}

export function notice(message: string, tone: "info" | "warn" = "info"): Raw {
  return raw(
    `<div class="notice" data-tone="${tone}">${icon(tone === "warn" ? "warning" : "info", 15).value}<div>${message}</div></div>`,
  );
}

/* -------------------------------------------------------------------- toasts */

export interface ToastOptions {
  title: string;
  message?: string;
  tone?: "info" | "success" | "error" | "warn";
  timeoutMs?: number;
  actions?: { label: string; action: string; data?: Record<string, string | number> }[];
}

interface ToastRecord extends ToastOptions {
  id: number;
  tone: NonNullable<ToastOptions["tone"]>;
}

const toasts: ToastRecord[] = [];
let toastSeq = 1;

export function toast(options: ToastOptions): number {
  const record: ToastRecord = {
    id: toastSeq++,
    tone: options.tone ?? "info",
    timeoutMs: options.timeoutMs ?? 6000,
    title: options.title,
    message: options.message,
    actions: options.actions,
  };
  toasts.push(record);
  renderToasts();
  const ttl = record.timeoutMs ?? 6000;
  if (ttl > 0) {
    window.setTimeout(() => dismissToast(record.id), ttl);
  }
  return record.id;
}

export function dismissToast(id: number): void {
  const index = toasts.findIndex((t) => t.id === id);
  if (index >= 0) toasts.splice(index, 1);
  renderToasts();
}

export function renderToasts(): void {
  const host = qs("#toasts");
  if (!host) return;
  render(
    host,
    toasts
      .map(
        (t) => html`<div class="toast" data-tone="${t.tone}" role="status">
          <div class="toast-body">
            <div class="toast-title">${t.title}</div>
            ${t.message ? html`<div class="toast-msg">${t.message}</div>` : ""}
            ${t.actions?.length
              ? html`<div class="toast-actions">
                  ${t.actions.map((a) =>
                    raw(
                      `<button type="button" class="btn btn-sm btn-ghost" data-action="${a.action}"${dataAttrs(a.data)}>${a.label}</button>`,
                    ),
                  )}
                </div>`
              : ""}
          </div>
          ${raw(iconButton({ action: "toast-dismiss", iconName: "close", label: "Dismiss", size: 15, data: { toast: t.id } }).value)}
        </div>`,
      )
      .join(""),
  );
}

/* ------------------------------------------------------------------- confirm */

interface ConfirmRequest {
  title: string;
  message: string;
  confirmLabel: string;
  danger?: boolean;
  resolve: (ok: boolean) => void;
}

let activeConfirm: ConfirmRequest | null = null;

export function confirmDialog(options: Omit<ConfirmRequest, "resolve">): Promise<boolean> {
  return new Promise((resolve) => {
    activeConfirm = { ...options, resolve };
    renderConfirm();
  });
}

export function resolveConfirm(ok: boolean): void {
  if (!activeConfirm) return;
  const pending = activeConfirm;
  activeConfirm = null;
  renderConfirm();
  pending.resolve(ok);
}

function renderConfirm(): void {
  const host = qs("#overlays");
  if (!host) return;
  if (!activeConfirm) {
    const existing = qs("#confirm-host");
    existing?.remove();
    return;
  }
  const existing = qs("#confirm-host") ?? document.createElement("div");
  existing.id = "confirm-host";
  render(
    existing,
    html`<div class="modal-scrim">
      <div class="modal" style="grid-template-columns:1fr;width:min(440px,100%)" role="dialog" aria-modal="true" aria-label="${activeConfirm.title}">
        <div class="modal-side">
          <div class="modal-side-head"><h2 class="modal-title">${activeConfirm.title}</h2></div>
          <div class="modal-side-body">
            <p>${activeConfirm.message}</p>
            <div class="modal-actions">
              ${raw(button({ action: "confirm-cancel", label: "Cancel", variant: "ghost" }).value)}
              ${raw(
                button({
                  action: "confirm-accept",
                  label: activeConfirm.confirmLabel,
                  variant: activeConfirm.danger ? "danger" : "primary",
                }).value,
              )}
            </div>
          </div>
        </div>
      </div>
    </div>`,
  );
  if (!existing.parentElement) host.appendChild(existing);
  qs<HTMLButtonElement>("[data-action='confirm-accept']", existing)?.focus();
}
