/** Search orchestration on the frontend: submission, streaming results, filters, library views. */
import { api, isDesktop, on } from "../../lib/api";
import { qs, qsa, registerActions } from "../../lib/dom";
import { pluralize } from "../../lib/format";
import { emit, emptyFilters, gridRequest, resetResults, set, state } from "../../lib/store";
import type { Asset, ProviderPayload, ProviderStatusInfo, SearchFinished } from "../../lib/types";
import { icon } from "../../components/icons";
import { dismissToast, toast } from "../../components/ui";
import { clearRenderedIds } from "./results";
import { CATEGORIES } from "./options";
import { closePreview, openPreview } from "../preview/preview";

let suggestTimer = 0;
let minSizeTimer = 0;
let libraryTimer = 0;
let currentSearch = 0;
let pendingCancel = false;

/* ------------------------------------------------------------------ bootstrap */

export async function bootstrap(): Promise<void> {
  try {
    const [settings, providers, favorites, collections, downloads, history, cache, thumbnailBase] = await Promise.all([
      api.getSettings(),
      api.listProviders(),
      api.favoriteIds(),
      api.listCollections(),
      api.listDownloads(),
      api.searchHistory(),
      api.cacheStats(),
      api.thumbnailBase(),
    ]);
    state.mock = !isDesktop;
    state.settings = settings;
    state.settingsDraft = { ...settings };
    state.providers = providers;
    state.enabledProviders = settings.enabledProviders.filter((id) => providers.some((p) => p.id === id));
    state.favoriteIds = new Set(favorites);
    state.collections = collections;
    state.downloads = downloads;
    state.downloadsById = Object.fromEntries(downloads.map((d) => [d.id, d]));
    state.history = history;
    state.cache = cache;
    state.thumbnailBase = thumbnailBase;
    state.gridDensity = settings.density;
    state.suggestions = history.map((h) => h.query).slice(0, 8);
    state.ready = true;

    applyTheme();
    const statuses = await api.providerStatus();
    state.statuses = Object.fromEntries(statuses.map((s) => [s.provider, s]));
    emit();
  } catch (error) {
    state.fatalError = error instanceof Error ? error.message : String(error);
    state.ready = true;
    emit();
  }
}

export function applyTheme(): void {
  const theme = state.settings?.theme ?? "system";
  const prefersDark = window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
  const resolved = theme === "system" ? (prefersDark ? "dark" : "light") : theme;
  document.documentElement.dataset.theme = resolved;
  document.documentElement.dataset.density = state.settings?.density ?? "standard";
  document.documentElement.dataset.motion = state.settings?.reducedMotion ? "reduced" : "normal";
}

/* -------------------------------------------------------------- search flow */

export function computeLocalFilters(): string[] {
  const enabled = state.providers.filter((p) => state.enabledProviders.includes(p.id));
  const notes: string[] = [];
  const filters = state.filters;
  const supports = (key: "orientationFilter" | "sizeFilter" | "mediaTypeFilter" | "licenseFilter" | "colorFilter") =>
    enabled.some((p) => p.capabilities[key]);
  if (filters.orientation !== "any" && !supports("orientationFilter")) notes.push(`orientation (applied locally)`);
  if ((filters.minWidth > 0 || filters.minHeight > 0) && !supports("sizeFilter")) notes.push(`minimum size (applied locally)`);
  if (filters.mediaType !== "any" && !supports("mediaTypeFilter")) notes.push(`image type (applied locally)`);
  if (filters.license !== "any" && !supports("licenseFilter")) notes.push(`licence (applied locally)`);
  if (filters.color !== "any" && !supports("colorFilter")) notes.push(`colour (applied locally where sources publish colour data)`);
  return notes;
}

export async function submitSearch(explicitQuery?: string): Promise<void> {
  const query = (explicitQuery ?? state.draft).trim().replace(/\s+/g, " ");
  if (!query) {
    toast({ title: "Enter something to search for", message: "Try “mountain landscape” or “blue abstract background”.", tone: "warn" });
    qs<HTMLInputElement>("#stockcraft-search-input")?.focus();
    return;
  }
  if (query.length > 160) {
    toast({ title: "Query is too long", message: "Searches are limited to 160 characters.", tone: "warn" });
    return;
  }

  if (state.searching && state.searchId) await cancelSearch({ silent: true });

  if (!state.enabledProviders.length) {
    toast({
      title: "No sources enabled",
      message: "Enable at least one source in the sidebar or in Settings → Sources.",
      tone: "warn",
    });
    return;
  }

  state.draft = query;
  state.submittedQuery = query;
  state.suggestOpen = false;
  state.view = "search";
  state.activeCollectionId = null;
  // The previous (finished) session id is kept for "Load more", but a new search must not be able
  // to cancel it by accident while this request is still starting.
  state.searchId = null;
  pendingCancel = false;
  resetResults();
  clearRenderedIds();
  state.ignoredFilters = computeLocalFilters();
  state.searching = true;
  state.finished = false;
  state.cancelled = false;
  state.totals = Object.fromEntries(state.enabledProviders.map((id) => [id, 0]));

  const providerName = (id: string) => state.providers.find((p) => p.id === id)?.name ?? id;
  state.statuses = {
    ...state.statuses,
    ...Object.fromEntries(
      state.enabledProviders.map((id) => [
        id,
        {
          provider: id,
          name: providerName(id),
          state: "searching" as const,
          message: "request in flight",
          results: 0,
          total: null,
          latencyMs: null,
          hasMore: false,
          failures: 0,
          circuitOpenUntilMs: null,
          lastSuccessMs: null,
          blocked: false,
        },
      ]),
    ),
  };
  state.statusMessage = `Searching ${state.enabledProviders.length} source(s) for “${query}”…`;
  const scroller = qs("#results-scroll");
  if (scroller) scroller.scrollTop = 0;
  gridRequest(true);
  emit("header", "sidebar", "toolbar", "rail", "footer", "results");

  try {
    const started = await api.startSearch({
      query,
      providers: state.enabledProviders,
      limit: state.settings?.resultsPerPage ?? 24,
      page: 1,
      filters: state.filters,
      sort: state.sort,
    });
    currentSearch += 1;
    state.searchId = started.searchId;
    if (pendingCancel) {
      pendingCancel = false;
      await cancelSearch({ silent: true });
      return;
    }
    state.statuses = {
      ...state.statuses,
      ...Object.fromEntries(started.providers.map((s) => [s.provider, s])),
    };
    emit("toolbar", "rail", "footer");
    await refreshHistory();
  } catch (error) {
    pendingCancel = false;
    state.searching = false;
    state.finished = true;
    const message = error instanceof Error ? error.message : String(error);
    state.statusMessage = `Search failed: ${message}`;
    state.searchErrors = [{ provider: "search", message }];
    toast({ title: "Search failed", message, tone: "error" });
    emit("rail", "footer", "results", "toolbar");
  }
}

export async function cancelSearch(options: { silent?: boolean } = {}): Promise<void> {
  if (!state.searchId) {
    // The search command has not returned its session id yet; cancel it as soon as it does.
    if (state.searching) pendingCancel = true;
    return;
  }
  const searchId = state.searchId;
  state.searchId = null;
  try {
    await api.cancelSearch(searchId);
  } catch (error) {
    if (!options.silent) {
      toast({ title: "Could not cancel search", message: String(error), tone: "error" });
    }
  }
  state.searching = false;
  state.cancelled = true;
  state.statusMessage = "Search cancelled.";
  state.statuses = Object.fromEntries(
    Object.entries(state.statuses).map(([id, status]) => [
      id,
      status.state === "searching" ? { ...status, state: "empty" as const, message: "cancelled" } : status,
    ]),
  );
  if (!options.silent) emit("toolbar", "rail", "footer", "results");
}

export async function loadMore(provider?: string): Promise<void> {
  if (!state.searchId) {
    toast({ title: "No active search", message: "Run a search before loading more results.", tone: "warn" });
    return;
  }
  const targets = provider
    ? [provider]
    : Object.entries(state.hasMore)
        .filter(([id, more]) => more && state.enabledProviders.includes(id))
        .map(([id]) => id);
  if (!targets.length) return;

  state.loadingMore = { ...state.loadingMore, ...Object.fromEntries(targets.map((id) => [id, true])) };
  state.statusMessage = "Loading more results…";
  emit("toolbar", "rail", "footer", "results");

  await Promise.all(
    targets.map(async (id) => {
      try {
        const status = await api.loadMore(state.searchId as string, id);
        state.statuses = { ...state.statuses, [id]: status };
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        toast({ title: `Could not load more from ${providerLabel(id)}`, message, tone: "error" });
        state.hasMore = { ...state.hasMore, [id]: false };
        state.searchErrors = [...state.searchErrors, { provider: id, message }];
      } finally {
        state.loadingMore = { ...state.loadingMore, [id]: false };
      }
    }),
  );
  state.statusMessage = `Loaded ${pluralize(state.assets.length, "result")}.`;
  emit("toolbar", "rail", "footer", "results");
}

function providerLabel(id: string): string {
  return state.providers.find((p) => p.id === id)?.name ?? id;
}

async function refreshHistory(): Promise<void> {
  try {
    state.history = await api.searchHistory();
    state.suggestions = state.history.map((h) => h.query).slice(0, 8);
  } catch {
    /* history is non-critical */
  }
}

/* ------------------------------------------------------------------- events */

export async function subscribeSearchEvents(): Promise<() => void> {
  const unlistenProvider = await on<ProviderPayload>("search://provider", (payload) => {
    if (payload.searchId !== state.searchId) return; // stale search: ignore
    const firstResults = state.assets.length === 0;
    const fresh: Asset[] = [];
    for (const asset of payload.assets) {
      if (state.seen.has(asset.id)) continue;
      state.seen.add(asset.id);
      fresh.push(asset);
    }
    if (payload.append) {
      // keep provider results grouped in arrival order
      state.assets = [...state.assets, ...fresh];
    } else if (fresh.length) {
      state.assets = [...state.assets, ...fresh];
    }
    state.pages = { ...state.pages, [payload.provider]: payload.page };
    state.hasMore = { ...state.hasMore, [payload.provider]: payload.hasMore };
    state.totals = { ...state.totals, [payload.provider]: payload.total ?? state.totals[payload.provider] ?? fresh.length };
    state.statuses = { ...state.statuses, [payload.provider]: payload.status };
    state.ignoredFilters = computeLocalFilters();
    // The first payload replaces the skeleton tiles; later payloads append incrementally.
    gridRequest(firstResults && fresh.length > 0);
    emit("toolbar", "rail", "footer", "sidebar");
  });

  const unlistenFinished = await on<SearchFinished>("search://finished", (payload) => {
    if (payload.searchId !== state.searchId) return;
    state.searching = false;
    state.finished = true;
    // Keep the session id: the backend keeps the session alive so "Load more" can request the next
    // page after the initial stream has finished. It is replaced by the next search.
    state.cancelled = payload.cancelled;
    state.searchErrors = payload.errors;
    const failures = payload.errors.length;
    const loaded = state.assets.length;
    state.statusMessage = payload.cancelled
      ? `Search cancelled after ${pluralize(loaded, "result")}.`
      : failures
        ? `Search finished with ${pluralize(loaded, "result")}; ${pluralize(failures, "source")} reported a problem.`
        : `Search finished: ${pluralize(loaded, "result")} from ${state.enabledProviders.length} source(s).`;
    state.statuses = Object.fromEntries(
      Object.entries(state.statuses).map(([id, status]) => [
        id,
        status.state === "searching" ? { ...status, state: status.results ? ("ok" as const) : ("empty" as const), message: "search finished" } : status,
      ]),
    );
    emit("toolbar", "rail", "footer", "results");
  });

  return () => {
    unlistenProvider();
    unlistenFinished();
  };
}

/* ------------------------------------------------------------------ actions */

function syncFavoriteIndicators(id: string): void {
  const on_ = state.favoriteIds.has(id);
  for (const element of qsa<HTMLButtonElement>(`[data-action="toggle-favorite"][data-id="${CSS.escape(id)}"]`)) {
    element.setAttribute("aria-pressed", String(on_));
    element.setAttribute("aria-label", on_ ? "Remove from favourites" : "Add to favourites");
    element.innerHTML = icon(on_ ? "heart-filled" : "heart", 15).value;
  }
}

function findAsset(id: string): Asset | undefined {
  const fromResults = state.assets.find((asset) => asset.id === id);
  if (fromResults) return fromResults;
  if (state.preview && state.preview.id === id) return state.preview;
  return undefined;
}

export async function toggleFavorite(id: string): Promise<void> {
  const asset = findAsset(id);
  if (!asset) return;
  try {
    const nowFavorite = await api.toggleFavorite(asset);
    const next = new Set(state.favoriteIds);
    if (nowFavorite) next.add(id);
    else next.delete(id);
    state.favoriteIds = next;
    syncFavoriteIndicators(id);
    emit("header");
    if (state.view === "favorites") await loadFavorites();
    if (state.previewId) emit("preview");
    toast({
      title: nowFavorite ? "Added to favourites" : "Removed from favourites",
      message: asset.title ?? undefined,
      tone: nowFavorite ? "success" : "info",
      timeoutMs: 2500,
    });
  } catch (error) {
    toast({ title: "Could not update favourites", message: String(error), tone: "error" });
  }
}

export async function downloadAsset(asset: Asset): Promise<void> {
  if (!asset.downloadAllowed || !asset.downloadUrl) {
    toast({
      title: "Download not available",
      message: `${providerLabel(asset.provider)} does not publish a permitted download URL for this asset. Use “Open original page”.`,
      tone: "warn",
    });
    return;
  }
  try {
    const id = await api.downloadAsset(asset, null);
    toast({
      title: "Download started",
      message: `${asset.title ?? "image"} → ${state.settings?.downloadDir ?? "default folder"}`,
      tone: "info",
      actions: [{ label: "Show downloads", action: "show-downloads" }],
      timeoutMs: 8000,
    });
    state.statusMessage = `Download #${id} started.`;
    emit("footer", "header");
    void refreshDownloads();
  } catch (error) {
    toast({ title: "Download failed", message: error instanceof Error ? error.message : String(error), tone: "error" });
  }
}

export async function refreshDownloads(): Promise<void> {
  try {
    state.downloads = await api.listDownloads();
    state.downloadsById = Object.fromEntries(state.downloads.map((d) => [d.id, d]));
    emit("header");
    if (state.panel === "downloads") emit("panel");
  } catch {
    /* non-critical */
  }
}

export async function openSource(url: string | null | undefined): Promise<void> {
  if (!url) return;
  try {
    const ok = await api.validateExternalUrl(url);
    if (!ok) {
      toast({ title: "Blocked unsafe link", message: "Only http(s) links can be opened.", tone: "warn" });
      return;
    }
    await api.openExternalUrl(url);
  } catch (error) {
    toast({ title: "Could not open link", message: String(error), tone: "error" });
  }
}

export async function loadFavorites(query = ""): Promise<void> {
  state.view = "favorites";
  state.activeCollectionId = null;
  const assets = await api.listFavorites(query);
  state.assets = assets;
  state.seen = new Set(assets.map((a) => a.id));
  clearRenderedIds();
  gridRequest(true);
  state.statusMessage = `${pluralize(assets.length, "favourite")} loaded.`;
  emit("header", "toolbar", "rail", "footer", "results", "sidebar");
}

export async function loadCollection(id: number, query = ""): Promise<void> {
  const collection = state.collections.find((c) => c.id === id);
  state.view = "collection";
  state.activeCollectionId = id;
  const assets = await api.listLibrary(query, id);
  state.assets = assets;
  state.seen = new Set(assets.map((a) => a.id));
  clearRenderedIds();
  gridRequest(true);
  state.statusMessage = `${pluralize(assets.length, "image")} in “${collection?.name ?? "collection"}”.`;
  emit("header", "toolbar", "rail", "footer", "results", "sidebar");
}

export function backToSearch(): void {
  state.view = "search";
  state.activeCollectionId = null;
  state.draft = state.submittedQuery;
  clearRenderedIds();
  gridRequest(true);
  state.statusMessage = state.submittedQuery ? `Showing results for “${state.submittedQuery}”.` : "Ready.";
  emit("header", "toolbar", "rail", "footer", "results", "sidebar");
}

export function registerControllerActions(): void {
  registerActions({
    "search-form": {
      submit: () => void submitSearch(),
      click: () => {
        /* submit button path is handled by the form's submit event */
      },
    },
    "search-input": {
      input: (element) => {
        const input = element as HTMLInputElement;
        set({ draft: input.value });
        window.clearTimeout(suggestTimer);
        suggestTimer = window.setTimeout(() => {
          const needle = input.value.trim().toLowerCase();
          const matches = state.history
            .map((h) => h.query)
            .filter((q, index, all) => all.indexOf(q) === index)
            .filter((q) => (needle ? q.toLowerCase().includes(needle) && q.toLowerCase() !== needle : false))
            .slice(0, 6);
          set({ suggestions: matches, suggestOpen: matches.length > 0, suggestIndex: -1 }, "header");
        }, 220);
      },
      keydown: (_element, event) => {
        const keyEvent = event as KeyboardEvent;
        if (keyEvent.key === "Escape") {
          set({ suggestOpen: false, suggestIndex: -1 }, "header");
          return;
        }
        if (keyEvent.key === "ArrowDown" || keyEvent.key === "ArrowUp") {
          if (!state.suggestions.length) return;
          keyEvent.preventDefault();
          const delta = keyEvent.key === "ArrowDown" ? 1 : -1;
          const next = (state.suggestIndex + delta + state.suggestions.length + 1) % (state.suggestions.length + 1) - 1;
          set({ suggestIndex: next, suggestOpen: true }, "header");
          return;
        }
        if (keyEvent.key === "Enter") {
          keyEvent.preventDefault();
          if (state.suggestIndex >= 0 && state.suggestions[state.suggestIndex]) {
            const query = state.suggestions[state.suggestIndex];
            set({ draft: query, suggestOpen: false }, "header");
            void submitSearch(query);
          } else {
            void submitSearch();
          }
          return;
        }
        if (keyEvent.key === "Tab") set({ suggestOpen: false }, "header");
      },
    },
    "clear-search": {
      click: () => {
        set({ draft: "", suggestOpen: false }, "header");
        resetResults();
        clearRenderedIds();
        gridRequest(true);
        state.statusMessage = "Search cleared.";
        emit("toolbar", "rail", "footer", "results");
        qs<HTMLInputElement>("#stockcraft-search-input")?.focus();
      },
    },
    "quick-search": { click: (element) => void submitSearch(element.dataset.query ?? "") },
    "use-suggestion": {
      click: (element) => {
        const query = element.dataset.query ?? "";
        set({ draft: query, suggestOpen: false }, "header");
        void submitSearch(query);
      },
    },
    "cancel-search": { click: () => void cancelSearch() },
    "search-again": { click: () => void submitSearch(state.submittedQuery) },
    "retry-all": {
      click: async () => {
        try {
          const statuses = await api.resetProviderHealth(null);
          state.statuses = Object.fromEntries(statuses.map((s) => [s.provider, s]));
        } catch (error) {
          toast({ title: "Could not reset sources", message: String(error), tone: "error" });
        }
        if (state.submittedQuery) await submitSearch(state.submittedQuery);
        else emit("rail");
      },
    },
    "retry-provider": {
      click: async (element) => {
        const provider = element.dataset.provider ?? "";
        try {
          const statuses = await api.resetProviderHealth(provider);
          state.statuses = Object.fromEntries(statuses.map((s) => [s.provider, s]));
          toast({ title: `${providerLabel(provider)} re-armed`, message: "Retrying this source now.", tone: "info" });
          state.statuses = {
            ...state.statuses,
            [provider]: { ...(state.statuses[provider] as ProviderStatusInfo), state: "idle", failures: 0, circuitOpenUntilMs: null },
          };
        } catch (error) {
          toast({ title: "Could not reset source", message: String(error), tone: "error" });
        }
        if (state.submittedQuery && !state.searching) await submitSearch(state.submittedQuery);
        else emit("rail");
      },
    },
    "load-more-all": { click: () => void loadMore() },
    "reset-filters": {
      click: () => {
        set({ filters: { ...emptyFilters } });
        state.ignoredFilters = computeLocalFilters();
        gridRequest(true);
        emit("sidebar", "toolbar", "results", "footer");
      },
    },
    "remove-filter": {
      click: (element) => {
        const filter = element.dataset.filter ?? "";
        const filters = { ...state.filters };
        switch (filter) {
          case "orientation":
            filters.orientation = "any";
            break;
          case "mediaType":
            filters.mediaType = "any";
            break;
          case "license":
            filters.license = "any";
            break;
          case "color":
            filters.color = "any";
            break;
          case "minWidth":
            filters.minWidth = 0;
            break;
          case "minHeight":
            filters.minHeight = 0;
            break;
          case "category":
            filters.category = "all";
            break;
          default:
            break;
        }
        set({ filters });
        state.ignoredFilters = computeLocalFilters();
        gridRequest(true);
        emit("sidebar", "toolbar", "results");
      },
    },
    "set-orientation": {
      change: (element) => {
        set({ filters: { ...state.filters, orientation: (element as HTMLSelectElement).value as typeof state.filters.orientation } });
        afterFilterChange();
      },
    },
    "set-media-type": {
      change: (element) => {
        set({ filters: { ...state.filters, mediaType: (element as HTMLSelectElement).value } });
        afterFilterChange();
      },
    },
    "set-license": {
      change: (element) => {
        set({ filters: { ...state.filters, license: (element as HTMLSelectElement).value } });
        afterFilterChange();
      },
    },
    "set-sort": {
      change: (element) => {
        set({ sort: (element as HTMLSelectElement).value });
        gridRequest(true);
        emit("toolbar", "results");
      },
    },
    "set-density": {
      change: async (element) => {
        const density = (element as HTMLSelectElement).value as "cozy" | "standard" | "dense";
        await updateSettings({ density });
      },
    },
    "set-min-width": {
      change: (element) => {
        window.clearTimeout(minSizeTimer);
        minSizeTimer = window.setTimeout(() => {
          set({ filters: { ...state.filters, minWidth: Number((element as HTMLInputElement).value) || 0 } });
          afterFilterChange();
        }, 250);
      },
    },
    "set-min-height": {
      change: (element) => {
        window.clearTimeout(minSizeTimer);
        minSizeTimer = window.setTimeout(() => {
          set({ filters: { ...state.filters, minHeight: Number((element as HTMLInputElement).value) || 0 } });
          afterFilterChange();
        }, 250);
      },
    },
    "set-color": {
      click: (element) => {
        set({ filters: { ...state.filters, color: element.dataset.color ?? "any" } });
        afterFilterChange();
      },
    },
    "choose-category": {
      click: (element) => {
        const category = CATEGORIES.find((c) => c.id === element.dataset.category);
        if (!category || category.disabled) {
          toast({
            title: `${category?.label ?? "This category"} is not available`,
            message: "No enabled source supports it yet. Stockcraft does not show filters that do nothing.",
            tone: "warn",
          });
          return;
        }
        set({ filters: { ...state.filters, category: category.id } });
        if (category.term) {
          const base = (state.draft || state.submittedQuery).trim();
          const query = base ? `${base} ${category.term}` : category.term;
          set({ draft: query }, "header");
          void submitSearch(query);
        } else {
          afterFilterChange();
        }
      },
    },
    "toggle-provider": {
      click: async (element) => {
        const id = element.dataset.provider ?? "";
        const enabled = new Set(state.enabledProviders);
        if (enabled.has(id)) enabled.delete(id);
        else enabled.add(id);
        if (!enabled.size) {
          toast({ title: "Keep at least one source enabled", tone: "warn" });
          return;
        }
        state.enabledProviders = Array.from(enabled);
        await updateSettings({ enabledProviders: state.enabledProviders });
        toast({
          title: enabled.has(id) ? `${providerLabel(id)} enabled` : `${providerLabel(id)} disabled`,
          message: enabled.has(id) ? "It will receive your next search query." : "It will not receive further queries.",
          tone: "info",
          timeoutMs: 3000,
        });
        emit("sidebar", "rail", "footer", "toolbar");
      },
    },
    "toggle-sidebar": {
      click: () => set({ sidebarOpen: !state.sidebarOpen }, "header", "sidebar"),
    },
    "toggle-theme": {
      click: async () => {
        const next = document.documentElement.dataset.theme === "dark" ? "light" : "dark";
        await updateSettings({ theme: next });
      },
    },
    "go-search": { click: () => backToSearch() },
    "show-favorites": { click: () => void loadFavorites() },
    "open-collection": { click: (element) => void loadCollection(Number(element.dataset.collection)) },
    "library-search": {
      input: (element) => {
        window.clearTimeout(libraryTimer);
        const value = (element as HTMLInputElement).value;
        set({ draft: value });
        libraryTimer = window.setTimeout(() => {
          if (state.view === "favorites") void loadFavorites(value);
          else if (state.view === "collection" && state.activeCollectionId) void loadCollection(state.activeCollectionId, value);
        }, 220);
      },
    },
    "remove-from-active-collection": {
      click: async (element) => {
        if (!state.activeCollectionId) return;
        const id = element.dataset.id ?? "";
        try {
          const collection = await api.removeFromCollection(state.activeCollectionId, id);
          state.collections = state.collections.map((c) => (c.id === collection.id ? collection : c));
          await loadCollection(collection.id, state.draft);
          toast({ title: "Removed from collection", message: collection.name, tone: "info", timeoutMs: 2500 });
        } catch (error) {
          toast({ title: "Could not remove image", message: String(error), tone: "error" });
        }
      },
    },
    "open-preview": {
      click: (element) => void openPreview(element.dataset.id ?? "", element.dataset.provider ?? ""),
    },
    "show-history": {
      click: async () => {
        await refreshHistory();
        set({ suggestions: state.history.map((h) => h.query).slice(0, 12), suggestOpen: true, suggestIndex: -1 }, "header");
        qs<HTMLInputElement>("#stockcraft-search-input")?.focus();
      },
    },
    "clear-history": {
      click: async () => {
        try {
          await api.clearSearchHistory();
          set({ history: [], suggestions: [], suggestOpen: false }, "header");
          toast({ title: "Search history cleared", tone: "info", timeoutMs: 2500 });
        } catch (error) {
          toast({ title: "Could not clear history", message: String(error), tone: "error" });
        }
      },
    },
    "close-preview": { click: () => closePreview() },
    "toggle-favorite": { click: (element) => void toggleFavorite(element.dataset.id ?? "") },
    "quick-download": {
      click: (element) => {
        const asset = findAsset(element.dataset.id ?? "");
        if (asset) void downloadAsset(asset);
      },
    },
    "open-source": { click: (element) => void openSource(element.dataset.url) },
    "toast-dismiss": { click: (element) => dismissToast(Number(element.dataset.toast)) },
    "noop": { click: () => undefined },
    "reload": { click: () => window.location.reload() },
  });
}

function afterFilterChange(): void {
  state.ignoredFilters = computeLocalFilters();
  gridRequest(true);
  emit("sidebar", "toolbar", "results", "footer");
}

export async function updateSettings(patch: Partial<NonNullable<typeof state.settings>>): Promise<void> {
  try {
    const settings = await api.updateSettings(patch);
    set({ settings, settingsDraft: { ...settings }, gridDensity: settings.density });
    applyTheme();
    emit("header", "sidebar", "panel", "results", "footer", "toolbar");
  } catch (error) {
    toast({ title: "Could not save settings", message: error instanceof Error ? error.message : String(error), tone: "error" });
  }
}
