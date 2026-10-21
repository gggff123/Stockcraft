/** Application entry point: shell markup, region renderers, keyboard shortcuts and event wiring. */
import { api, isDesktop, on } from "./lib/api";
import { initDelegation, isTypingTarget, qs, registerActions, render } from "./lib/dom";
import { emit, mountRegion, set, state } from "./lib/store";
import type { DownloadRecord } from "./lib/types";
import { renderToasts, resolveConfirm, toast } from "./components/ui";
import {
  SEARCH_INPUT_ID,
  renderBanner,
  renderFooter,
  renderHeader,
  renderRail,
  renderSidebar,
  renderToolbar,
} from "./features/search/chrome";
import { initImageStates, renderResults } from "./features/search/results";
import {
  applyTheme,
  backToSearch,
  bootstrap,
  loadMore,
  registerControllerActions,
  refreshDownloads,
  subscribeSearchEvents,
} from "./features/search/controller";
import { closePreview, openPreview, registerPreviewActions, renderPreview } from "./features/preview/preview";
import { registerCollectionActions, renderCollectionsPanel } from "./features/collections/collections";
import { registerDownloadActions, renderDownloadsPanel } from "./features/downloads/downloads";
import { refreshStats, registerSettingsActions, renderSettingsPanel } from "./features/settings/settings";

const SHELL = `
  <div id="banner-host"></div>
  <div id="header-host"></div>
  <div class="body">
    <div id="sidebar-host"></div>
    <main class="main" id="main">
      <div id="toolbar-host"></div>
      <div id="rail-host"></div>
      <div class="results" id="results-scroll" tabindex="-1">
        <div class="grid" id="grid" role="list" aria-label="Image results"></div>
        <div id="state-host"></div>
        <div class="load-more" id="load-more"></div>
      </div>
    </main>
  </div>
  <div id="footer-host"></div>
`;

interface DownloadProgress {
  downloadId: number;
  assetId: string;
  received: number;
  total: number;
  status: DownloadRecord["status"];
  filePath: string | null;
  error: string | null;
}

function renderPanel(): void {
  switch (state.panel) {
    case "settings":
      renderSettingsPanel();
      break;
    case "collections":
      renderCollectionsPanel();
      break;
    case "downloads":
      renderDownloadsPanel();
      break;
    default:
      render(qs("#panel-host") ?? document.createElement("div"), "");
      break;
  }
}

function mountShell(): void {
  const app = qs("#app");
  if (!app) throw new Error("Missing #app root");
  render(app, SHELL);
  const overlays = qs("#overlays");
  if (!overlays) throw new Error("Missing #overlays root");
  render(overlays, `<div id="preview-host"></div><div id="panel-host"></div>`);
}

function mountRegions(): void {
  mountRegion("header", () => {
    renderHeader();
    renderBanner();
  });
  mountRegion("sidebar", renderSidebar);
  mountRegion("toolbar", renderToolbar);
  mountRegion("rail", renderRail);
  mountRegion("footer", renderFooter);
  mountRegion("results", renderResults);
  mountRegion("panel", renderPanel);
  mountRegion("preview", renderPreview);
}

function registerGlobalActions(): void {
  registerActions({
    "confirm-accept": { click: () => resolveConfirm(true) },
    "confirm-cancel": { click: () => resolveConfirm(false) },
    "toggle-sidebar-narrow": { click: () => set({ sidebarOpen: !state.sidebarOpen }, "sidebar") },
  });
}

function installShortcuts(): void {
  window.addEventListener("keydown", (event) => {
    const meta = event.ctrlKey || event.metaKey;
    const key = event.key.toLowerCase();

    if (event.key === "Escape") {
      if (state.previewId) {
        closePreview();
        event.preventDefault();
        return;
      }
      if (state.panel !== "none") {
        state.panel = "none";
        emit("panel", "header");
        event.preventDefault();
        return;
      }
      if (state.suggestOpen) {
        set({ suggestOpen: false, suggestIndex: -1 }, "header");
        event.preventDefault();
        return;
      }
      if (state.sidebarOpen && window.innerWidth <= 980) {
        set({ sidebarOpen: false }, "sidebar");
      }
      return;
    }

    if (meta && key === "k") {
      event.preventDefault();
      qs<HTMLInputElement>(`#${SEARCH_INPUT_ID}`)?.focus();
      qs<HTMLInputElement>(`#${SEARCH_INPUT_ID}`)?.select();
      return;
    }
    if (meta && key === "f") {
      event.preventDefault();
      void import("./features/search/controller").then((m) => m.loadFavorites());
      return;
    }
    if (meta && key === "h") {
      event.preventDefault();
      set(
        {
          suggestions: state.history.map((h) => h.query).slice(0, 12),
          suggestOpen: true,
          suggestIndex: -1,
        },
        "header",
      );
      qs<HTMLInputElement>(`#${SEARCH_INPUT_ID}`)?.focus();
      return;
    }
    if (meta && event.shiftKey && key === "c") {
      event.preventDefault();
      state.panel = "collections";
      emit("panel", "header");
      return;
    }
    if (meta && key === "d") {
      event.preventDefault();
      state.panel = "downloads";
      emit("panel", "header");
      void refreshDownloads();
      return;
    }
    if (meta && event.key === ",") {
      event.preventDefault();
      state.panel = "settings";
      emit("panel", "header");
      void refreshStats();
      return;
    }
    if (meta && key === "r") {
      event.preventDefault();
      void loadMore();
      return;
    }
    if (!isTypingTarget(event.target) && key === "/") {
      event.preventDefault();
      qs<HTMLInputElement>(`#${SEARCH_INPUT_ID}`)?.focus();
      return;
    }
    if (!isTypingTarget(event.target) && key === "f") {
      event.preventDefault();
      set({ sidebarOpen: !state.sidebarOpen }, "sidebar", "header");
    }
  });
}

function subscribeDownloadEvents(): Promise<() => void> {
  return on<DownloadProgress>("download://progress", (payload) => {
    state.downloadProgress = {
      ...state.downloadProgress,
      [payload.downloadId]: { received: payload.received, total: payload.total },
    };
    const existing = state.downloadsById[payload.downloadId];
    if (existing) {
      state.downloadsById = {
        ...state.downloadsById,
        [payload.downloadId]: { ...existing, status: payload.status, bytes: payload.received, error: payload.error },
      };
    }
    if (payload.status === "done") {
      toast({
        title: "Download complete",
        message: payload.filePath ?? existing?.title ?? "File saved.",
        tone: "success",
        timeoutMs: 5000,
        actions: [{ label: "Show downloads", action: "show-downloads" }],
      });
      void refreshDownloads();
    } else if (payload.status === "error") {
      toast({ title: "Download failed", message: payload.error ?? "Unknown error", tone: "error", timeoutMs: 8000 });
      void refreshDownloads();
    } else if (payload.status === "cancelled") {
      void refreshDownloads();
    }
    emit("panel", "header");
  });
}

function handleResponsive(): void {
  const media = window.matchMedia("(max-width: 980px)");
  const apply = () => {
    if (!media.matches) {
      state.sidebarOpen = false;
    }
    document.documentElement.dataset.narrow = media.matches ? "true" : "false";
  };
  apply();
  media.addEventListener("change", apply);
}

async function main(): Promise<void> {
  mountShell();
  mountRegions();
  initDelegation();
  initImageStates();
  registerControllerActions();
  registerPreviewActions();
  registerCollectionActions();
  registerDownloadActions();
  registerSettingsActions();
  registerGlobalActions();
  installShortcuts();
  handleResponsive();
  renderToasts();

  const app = qs("#app");
  app?.setAttribute("aria-busy", "true");
  await bootstrap();
  app?.setAttribute("aria-busy", "false");
  applyTheme();

  await subscribeSearchEvents();
  await subscribeDownloadEvents();

  if (!isDesktop) {
    state.mock = true;
    emit();
  }

  // A first-run hint: the welcome screen is the landing state. Nothing is fetched until the user
  // submits a query, which keeps a clean installation free of surprise network traffic.
  emit();

  const input = qs<HTMLInputElement>(`#${SEARCH_INPUT_ID}`);
  input?.addEventListener("focus", () => {
    if (state.history.length && !state.draft) {
      set({ suggestions: state.history.map((h) => h.query).slice(0, 6), suggestOpen: true }, "header");
    }
  });

  window.addEventListener("beforeunload", () => {
    if (state.searchId) void api.cancelSearch(state.searchId);
  });

  // Expose a tiny debug surface for manual verification in the devtools console.
  (window as unknown as Record<string, unknown>).__stockcraft = { state, api, openPreview, loadMore, backToSearch };
}

void main().catch((error) => {
  console.error(error);
  const app = qs("#app");
  if (app) {
    render(
      app,
      `<div class="state-block" style="height:100vh">
        <h2>Stockcraft failed to start</h2>
        <p>${String(error)}</p>
      </div>`,
    );
  }
});
