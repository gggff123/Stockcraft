import type {
  AppSettings,
  Asset,
  AssetFilters,
  CacheStats,
  Collection,
  Diagnostics,
  DownloadRecord,
  HistoryItem,
  ProviderInfo,
  ProviderStatusInfo,
} from "./types";

export type View = "search" | "favorites" | "collection";
export type Panel = "none" | "settings" | "collections" | "downloads" | "history";

export interface GridRequest {
  reset: boolean;
  reason?: string;
}

export const emptyFilters: AssetFilters = {
  orientation: "any",
  minWidth: 0,
  minHeight: 0,
  mediaType: "any",
  license: "any",
  color: "any",
  category: "all",
};

export interface State {
  ready: boolean;
  mock: boolean;
  fatalError: string | null;

  view: View;
  activeCollectionId: number | null;

  draft: string;
  submittedQuery: string;
  filters: AssetFilters;
  sort: string;

  providers: ProviderInfo[];
  statuses: Record<string, ProviderStatusInfo>;
  enabledProviders: string[];
  pexelsPanelOpen: boolean;
  pexelsDraft: string;

  searchId: string | null;
  searching: boolean;
  finished: boolean;
  cancelled: boolean;
  cancelledById: Record<string, boolean>;
  assets: Asset[];
  seen: Set<string>;
  totals: Record<string, number>;
  pages: Record<string, number>;
  hasMore: Record<string, boolean>;
  loadingMore: Record<string, boolean>;
  ignoredFilters: string[];
  searchErrors: { provider: string; message: string }[];

  favoriteIds: Set<string>;
  collections: Collection[];
  newCollectionName: string;

  downloads: DownloadRecord[];
  downloadsById: Record<number, DownloadRecord>;
  downloadProgress: Record<number, { received: number; total: number }>;

  history: HistoryItem[];
  suggestions: string[];
  suggestOpen: boolean;
  suggestIndex: number;

  settings: AppSettings | null;
  settingsDraft: AppSettings | null;

  previewId: string | null;
  preview: Asset | null;
  previewLoading: boolean;
  pendingCollectionAssetId: string | null;
  renamingId: number | null;
  renameDraft: string;

  panel: Panel;
  sidebarOpen: boolean;

  cache: CacheStats | null;
  diagnostics: Diagnostics | null;
  thumbnailBase: string;

  statusMessage: string;
  gridDensity: string;
}

export const state: State = {
  ready: false,
  mock: false,
  fatalError: null,

  view: "search",
  activeCollectionId: null,

  draft: "",
  submittedQuery: "",
  filters: { ...emptyFilters },
  sort: "relevance",

  providers: [],
  statuses: {},
  enabledProviders: [],
  pexelsPanelOpen: false,
  pexelsDraft: "",

  searchId: null,
  searching: false,
  finished: false,
  cancelled: false,
  cancelledById: {},
  assets: [],
  seen: new Set(),
  totals: {},
  pages: {},
  hasMore: {},
  loadingMore: {},
  ignoredFilters: [],
  searchErrors: [],

  favoriteIds: new Set(),
  collections: [],
  newCollectionName: "",

  downloads: [],
  downloadsById: {},
  downloadProgress: {},

  history: [],
  suggestions: [],
  suggestOpen: false,
  suggestIndex: -1,

  settings: null,
  settingsDraft: null,

  previewId: null,
  preview: null,
  previewLoading: false,
  pendingCollectionAssetId: null,
  renamingId: null,
  renameDraft: "",

  panel: "none",
  sidebarOpen: false,

  cache: null,
  diagnostics: null,
  thumbnailBase: "",

  statusMessage: "",
  gridDensity: "standard",
};

export type Region = "header" | "sidebar" | "toolbar" | "rail" | "results" | "footer" | "panel" | "preview";

const renderers = new Map<Region, () => void>();
const gridState: GridRequest = { reset: true };

export function mountRegion(region: Region, renderer: () => void): void {
  renderers.set(region, renderer);
}

export function emit(...regions: Region[]): void {
  const targets = regions.length ? regions : (Array.from(renderers.keys()) as Region[]);
  for (const region of targets) {
    try {
      renderers.get(region)?.();
    } catch (error) {
      console.error(`render error in region ${region}`, error);
    }
  }
}

/** Grid rendering is incremental: `reset` clears the previously appended tiles. */
export function gridRequest(reset: boolean, reason?: string): void {
  gridState.reset = reset;
  gridState.reason = reason;
  emit("results");
}

export function takeGridRequest(): GridRequest {
  const current = { reset: gridState.reset, reason: gridState.reason };
  gridState.reset = false;
  return current;
}

export function set(patch: Partial<State>, ...regions: Region[]): void {
  Object.assign(state, patch);
  emit(...regions);
}

export function addAsset(asset: Asset): boolean {
  if (state.seen.has(asset.id)) return false;
  state.seen.add(asset.id);
  state.assets.push(asset);
  return true;
}

export function isFavorite(id: string): boolean {
  return state.favoriteIds.has(id);
}

export function resetResults(): void {
  state.assets = [];
  state.seen = new Set();
  state.totals = {};
  state.pages = {};
  state.hasMore = {};
  state.loadingMore = {};
  state.searchErrors = [];
  state.ignoredFilters = [];
  state.cancelled = false;
}

export function statusList(): ProviderStatusInfo[] {
  return state.providers
    .map((p) => state.statuses[p.id])
    .filter((s): s is ProviderStatusInfo => Boolean(s));
}

export function assetsForView(): Asset[] {
  return state.assets;
}
