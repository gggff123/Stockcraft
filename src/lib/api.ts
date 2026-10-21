/**
 * Tauri IPC bridge.
 *
 * In the desktop app every call is routed to a Rust command and every subscription to a Tauri
 * event. When the frontend is opened in a plain browser (used for UI preview/tests) a clearly
 * labelled in-memory mock backend is used instead; it never reaches the network and the UI shows a
 * banner so mock data can never be mistaken for provider results.
 */

import type {
  AppSettings,
  Asset,
  CacheStats,
  Collection,
  Diagnostics,
  DownloadRecord,
  HistoryItem,
  ProviderInfo,
  ProviderPayload,
  ProviderStatusInfo,
  SearchFinished,
  SearchRequest,
  SearchStart,
} from "./types";
import { mockCall, mockOn, mockReset } from "./mock";

export const isDesktop: boolean =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in (window as unknown as Record<string, unknown>);

export class ApiError extends Error {
  readonly code: string;
  readonly detail?: string;

  constructor(code: string, message: string, detail?: string) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.detail = detail;
  }
}

function toApiError(error: unknown, command: string): ApiError {
  if (error instanceof ApiError) return error;
  if (typeof error === "object" && error !== null) {
    const value = error as Record<string, unknown>;
    if (typeof value.code === "string" && typeof value.message === "string") {
      return new ApiError(value.code, value.message, typeof value.detail === "string" ? value.detail : undefined);
    }
    if (typeof value.message === "string") return new ApiError("unknown", value.message);
  }
  return new ApiError("unknown", `${command} failed: ${String(error)}`);
}

export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    if (isDesktop) {
      const core = await import("@tauri-apps/api/core");
      return await core.invoke<T>(command, args);
    }
    return await mockCall<T>(command, args);
  } catch (error) {
    throw toApiError(error, command);
  }
}

export type Unlisten = () => void;

export async function on<T>(event: string, handler: (payload: T) => void): Promise<Unlisten> {
  if (isDesktop) {
    const api = await import("@tauri-apps/api/event");
    const unlisten = await api.listen<T>(event, (e) => handler(e.payload));
    return unlisten;
  }
  return mockOn(event, handler as (payload: unknown) => void);
}

/* ------------------------------------------------------------------ commands */

export const api = {
  // search
  startSearch: (request: SearchRequest) => call<SearchStart>("start_search", { request }),
  cancelSearch: (searchId: string) => call<void>("cancel_search", { searchId }),
  loadMore: (searchId: string, provider: string) => call<ProviderStatusInfo>("load_more", { searchId, provider }),

  // providers
  listProviders: () => call<ProviderInfo[]>("list_providers"),
  providerStatus: () => call<ProviderStatusInfo[]>("provider_status"),
  resetProviderHealth: (provider?: string | null) => call<ProviderStatusInfo[]>("reset_provider_health", { provider: provider ?? null }),

  // assets
  getAssetDetails: (provider: string, assetId: string) => call<Asset>("get_asset_details", { provider, assetId }),

  // downloads
  downloadAsset: (asset: Asset, targetDir?: string | null) => call<number>("download_asset", { asset, targetDir: targetDir ?? null }),
  cancelDownload: (downloadId: number) => call<void>("cancel_download", { downloadId }),
  listDownloads: (limit = 60) => call<DownloadRecord[]>("list_downloads", { limit }),
  clearDownloadHistory: () => call<number>("clear_download_history"),
  openPath: (path: string) => call<void>("open_path", { path }),
  openExternalUrl: (url: string) => call<void>("open_external_url", { url }),
  validateExternalUrl: (url: string) => call<boolean>("validate_external_url", { url }),
  pickFolder: (title?: string) => call<string | null>("pick_folder", { title: title ?? null }),
  downloadPath: (downloadId: number) => call<string>("download_path", { downloadId }),

  // settings
  getSettings: () => call<AppSettings>("get_settings"),
  updateSettings: (patch: Partial<AppSettings>) => call<AppSettings>("update_settings", { patch }),

  // library (favourites + collections)
  toggleFavorite: (asset: Asset) => call<boolean>("toggle_favorite", { asset }),
  listFavorites: (query = "", limit = 200, offset = 0) =>
    call<Asset[]>("list_favorites", { query, limit, offset }),
  favoriteIds: () => call<string[]>("favorite_ids"),
  listLibrary: (query = "", collectionId?: number | null, limit = 200, offset = 0) =>
    call<Asset[]>("list_library", { query, collectionId: collectionId ?? null, limit, offset }),

  listCollections: () => call<Collection[]>("list_collections"),
  createCollection: (name: string, description = "") => call<Collection>("create_collection", { name, description }),
  renameCollection: (id: number, name: string, description = "") =>
    call<Collection>("rename_collection", { id, name, description }),
  deleteCollection: (id: number) => call<void>("delete_collection", { id }),
  addToCollection: (collectionId: number, asset: Asset) =>
    call<Collection>("add_to_collection", { collectionId, asset }),
  removeFromCollection: (collectionId: number, assetId: string) =>
    call<Collection>("remove_from_collection", { collectionId, assetId }),

  // history
  searchHistory: (limit = 12) => call<HistoryItem[]>("search_history", { limit }),
  clearSearchHistory: () => call<void>("clear_search_history"),
  deleteHistoryItem: (id: number) => call<void>("delete_history_item", { id }),

  // cache / diagnostics
  clearCache: (scope: "thumbnails" | "metadata" | "all") => call<number>("clear_cache", { scope }),
  cacheStats: () => call<CacheStats>("cache_stats"),
  diagnostics: () => call<Diagnostics>("get_diagnostics"),
  thumbnailBase: () => call<string>("get_thumbnail_base"),
};

export type { ProviderPayload, SearchFinished };
export const __testing = { mockReset };
