/** Downloads drawer: progress, cancellation, reveal-in-folder and download preferences. */
import { api } from "../../lib/api";
import { html, qs, raw, render, registerActions } from "../../lib/dom";
import { formatBytes, formatRelative } from "../../lib/format";
import { emit, state } from "../../lib/store";
import type { DownloadRecord } from "../../lib/types";
import { icon } from "../../components/icons";
import { button, confirmDialog, emptyState, iconButton, toast } from "../../components/ui";
import { refreshDownloads, updateSettings } from "../search/controller";

function statusLabel(record: DownloadRecord): string {
  switch (record.status) {
    case "queued":
      return "queued";
    case "running":
      return "downloading…";
    case "done":
      return `complete · ${formatBytes(record.bytes)}`;
    case "error":
      return `failed — ${record.error ?? "unknown error"}`;
    case "cancelled":
      return "cancelled";
    default:
      return record.status;
  }
}

export function renderDownloadsPanel(): void {
  const host = qs("#panel-host");
  if (!host) return;
  const running = state.downloads.filter((d) => d.status === "running" || d.status === "queued").length;

  render(
    host,
    html`<div class="drawer-scrim" data-action="close-panel"></div>
      <aside class="drawer" role="dialog" aria-modal="true" aria-label="Downloads" data-stop-click>
        <div class="drawer-head">
          <span style="color:var(--brand)">${icon("download", 20)}</span>
          <h2>Downloads</h2>
          ${running ? html`<span class="chip">${running} active</span>` : ""}
          ${iconButton({ action: "refresh-downloads", iconName: "refresh", label: "Refresh downloads" })}
          ${iconButton({ action: "close-panel", iconName: "close", label: "Close downloads" })}
        </div>
        <div class="drawer-body">
          <div class="card">
            <div class="row between wrap">
              <div class="grow">
                <div class="tile-title">Destination folder</div>
                <div class="tiny muted mono">${state.settings?.downloadDir || "not set"}</div>
              </div>
              ${button({ action: "pick-download-folder", label: "Change…", variant: "ghost", size: "sm", iconName: "folder" })}
              ${button({ action: "reveal-download-folder", label: "Open folder", variant: "ghost", size: "sm", iconName: "external" })}
            </div>
            <div class="tiny muted">
              Downloads are streamed by the Rust backend, written to a temporary file and renamed on completion so a
              partial file never looks finished.
            </div>
          </div>

          ${state.downloads.length
            ? state.downloads.map((record) => raw(html`
                <div class="dl-row">
                  <div class="row">
                    ${record.thumbnailUrl
                      ? html`<img class="dl-thumb" src="${record.thumbnailUrl}" alt="" loading="lazy" />`
                      : ""}
                    <div class="grow">
                      <div class="tile-title">${record.title}</div>
                      <div class="tiny muted">${record.provider} · ${statusLabel(record)} · ${formatRelative(record.createdAt)}</div>
                    </div>
                    ${record.status === "running" || record.status === "queued"
                      ? button({ action: "cancel-download", label: "Cancel", variant: "ghost", size: "sm", iconName: "close", data: { download: record.id } })
                      : ""}
                    ${record.status === "done"
                      ? button({ action: "reveal-download", label: "Show file", variant: "ghost", size: "sm", iconName: "folder", data: { download: record.id } })
                      : ""}
                    ${record.sourceUrl
                      ? iconButton({ action: "open-source", iconName: "external", label: `Open source page for ${record.title}`, size: 15, data: { url: record.sourceUrl } })
                      : ""}
                  </div>
                  ${record.status === "running"
                    ? html`<div class="progress"><i style="width:${progressFor(record)}%"></i></div>`
                    : ""}
                  <div class="tiny muted mono">${record.filePath}</div>
                  ${record.licenseName
                    ? html`<div class="tiny muted">
                        Licence: ${record.licenseName}${record.licenseUrl ? html` · <a href="${record.licenseUrl}" data-action="open-source" data-url="${record.licenseUrl}">terms</a>` : ""}
                      </div>`
                    : ""}
                </div>
              `))
            : raw(emptyState({ title: "No downloads yet", message: "Use the download button on a result to save the file. Stockcraft records the source page and licence next to every download.", iconName: "download" }).value)}

          <div class="tiny muted">
            An <code>attribution.txt</code> file and a JSON metadata sidecar are written next to each download when that
            option is enabled in Settings → Downloads.
          </div>

          ${state.downloads.length
            ? html`<div class="row">${button({ action: "clear-downloads-list", label: "Clear download list", variant: "ghost", size: "sm", iconName: "trash" })}</div>`
            : ""}
        </div>
      </aside>`,
  );
}

function progressFor(record: DownloadRecord): number {
  const progress = state.downloadProgress[record.id];
  if (!progress || !progress.total) return record.status === "running" ? 8 : 0;
  return Math.min(100, Math.round((progress.received / progress.total) * 100));
}

export function registerDownloadActions(): void {
  registerActions({
    "show-downloads": {
      click: () => {
        state.panel = "downloads";
        state.previewId = null;
        emit("panel", "header", "preview");
        void refreshDownloads();
      },
    },
    "refresh-downloads": { click: () => void refreshDownloads() },
    "cancel-download": {
      click: async (element) => {
        const id = Number(element.dataset.download);
        try {
          await api.cancelDownload(id);
          toast({ title: "Download cancelled", tone: "info", timeoutMs: 2500 });
          await refreshDownloads();
        } catch (error) {
          toast({ title: "Could not cancel download", message: error instanceof Error ? error.message : String(error), tone: "error" });
        }
      },
    },
    "reveal-download": {
      click: async (element) => {
        const id = Number(element.dataset.download);
        try {
          const path = await api.downloadPath(id);
          if (!path) {
            toast({ title: "File not found", message: "This download has no recorded path.", tone: "warn" });
            return;
          }
          await api.openPath(path);
        } catch (error) {
          toast({ title: "Could not open the file", message: error instanceof Error ? error.message : String(error), tone: "error" });
        }
      },
    },
    "pick-download-folder": {
      click: async () => {
        try {
          const folder = await api.pickFolder("Choose a folder for downloads");
          if (!folder) return;
          await updateSettings({ downloadDir: folder });
          toast({ title: "Download folder updated", message: folder, tone: "success", timeoutMs: 3500 });
        } catch (error) {
          toast({ title: "Could not choose a folder", message: error instanceof Error ? error.message : String(error), tone: "error" });
        }
      },
    },
    "reveal-download-folder": {
      click: async () => {
        const folder = state.settings?.downloadDir;
        if (!folder) {
          toast({ title: "No download folder set", tone: "warn" });
          return;
        }
        try {
          await api.openPath(folder);
        } catch (error) {
          toast({ title: "Could not open the folder", message: error instanceof Error ? error.message : String(error), tone: "error" });
        }
      },
    },
    "clear-downloads-list": {
      click: async () => {
        const confirmed = await confirmDialog({
          title: "Clear the download list?",
          message: "This only clears Stockcraft's download history. Files on disk are left untouched.",
          confirmLabel: "Clear list",
        });
        if (!confirmed) return;
        try {
          await api.clearDownloadHistory();
          state.downloads = [];
          state.downloadsById = {};
          emit("panel", "header");
        } catch (error) {
          toast({ title: "Could not clear the list", message: error instanceof Error ? error.message : String(error), tone: "error" });
        }
      },
    },
  });
}
