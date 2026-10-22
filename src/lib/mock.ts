/**
 * Browser-only mock backend.
 *
 * This module is used exclusively when the frontend runs outside Tauri (browser preview and
 * vitest). It performs no network I/O, generates obviously synthetic placeholder artwork, and the
 * UI displays a "browser preview / mock data" banner whenever it is active.
 */

import type {
  AppSettings,
  Asset,
  Collection,
  DownloadRecord,
  HistoryItem,
  ProviderInfo,
  ProviderStatusInfo,
  SearchRequest,
} from "./types";

type Listener = (payload: unknown) => void;
const listeners = new Map<string, Set<Listener>>();

export function mockOn(event: string, handler: Listener): () => void {
  const set = listeners.get(event) ?? new Set<Listener>();
  set.add(handler);
  listeners.set(event, set);
  return () => set.delete(handler);
}

function emit(event: string, payload: unknown): void {
  for (const handler of listeners.get(event) ?? []) handler(payload);
}

function placeholder(label: string, width: number, height: number, hue: number): string {
  const safe = label.replace(/[<>&]/g, "");
  const font = Math.max(14, Math.round(Math.min(width, height) / 8));
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}">` +
    `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">` +
    `<stop offset="0" stop-color="hsl(${hue} 60% 48%)"/><stop offset="1" stop-color="hsl(${(hue + 46) % 360} 55% 28%)"/>` +
    `</linearGradient><pattern id="h" width="11" height="11" patternUnits="userSpaceOnUse" patternTransform="rotate(38)">` +
    `<line x1="0" y1="0" x2="0" y2="11" stroke="rgba(255,255,255,0.16)" stroke-width="2"/></pattern></defs>` +
    `<rect width="${width}" height="${height}" fill="url(#g)"/><rect width="${width}" height="${height}" fill="url(#h)"/>` +
    `<text x="50%" y="52%" text-anchor="middle" fill="rgba(255,255,255,0.92)" font-family="Segoe UI, sans-serif" ` +
    `font-size="${font}" font-weight="600">${safe}</text></svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

const MOCK_PROVIDERS: ProviderInfo[] = [
  {
    id: "openverse",
    name: "Openverse",
    sourceName: "Openverse (WordPress Foundation)",
    websiteUrl: "https://openverse.org",
    licenseHint: "CC licences and public domain",
    enabledByDefault: true,
    requiresApiKey: false,
    hasApiKey: false,
    capabilities: {
      search: true,
      pagination: true,
      download: true,
      orientationFilter: false,
      sizeFilter: false,
      mediaTypeFilter: false,
      licenseFilter: true,
      colorFilter: false,
      sortOptions: ["relevance", "newest"],
      mediaTypes: ["photo", "illustration", "artwork"],
    },
    policy: {
      accessMethod: "Official REST API (api.openverse.org/v1)",
      docsUrl: "https://api.openverse.org/v1/",
      robotsUrl: null,
      termsUrl: "https://openverse.org/terms/",
      scrapingPermitted: false,
      anonymousAccess: true,
      rateLimit: "anonymous: 20/min burst, 200/day sustained",
      notes: "API only. The Openverse website/dataset is never scraped.",
      checkedOn: "2026-10-21",
    },
    notes: "Mock provider used for browser preview.",
  },
  {
    id: "wikimedia",
    name: "Wikimedia Commons",
    sourceName: "Wikimedia Commons",
    websiteUrl: "https://commons.wikimedia.org",
    licenseHint: "Per-file licence (CC, public domain)",
    enabledByDefault: true,
    requiresApiKey: false,
    hasApiKey: false,
    capabilities: {
      search: true,
      pagination: true,
      download: true,
      orientationFilter: false,
      sizeFilter: false,
      mediaTypeFilter: true,
      licenseFilter: false,
      colorFilter: false,
      sortOptions: ["relevance"],
      mediaTypes: ["photo", "illustration", "diagram", "map"],
    },
    policy: {
      accessMethod: "MediaWiki Action API",
      docsUrl: "https://www.mediawiki.org/wiki/API:Search",
      robotsUrl: "https://commons.wikimedia.org/robots.txt",
      termsUrl: "https://foundation.wikimedia.org/wiki/Policy:Terms_of_Use",
      scrapingPermitted: false,
      anonymousAccess: true,
      rateLimit: "MediaWiki API etiquette; app uses serial requests",
      notes: "API only; descriptive User-Agent required.",
      checkedOn: "2026-10-21",
    },
    notes: "Mock provider used for browser preview.",
  },
  {
    id: "nasa",
    name: "NASA Image Library",
    sourceName: "NASA Image and Video Library",
    websiteUrl: "https://images.nasa.gov",
    licenseHint: "NASA media usage guidelines (mostly public domain)",
    enabledByDefault: true,
    requiresApiKey: false,
    hasApiKey: false,
    capabilities: {
      search: true,
      pagination: true,
      download: true,
      orientationFilter: false,
      sizeFilter: false,
      mediaTypeFilter: false,
      licenseFilter: false,
      colorFilter: false,
      sortOptions: ["relevance"],
      mediaTypes: ["photo"],
    },
    policy: {
      accessMethod: "Public JSON API",
      docsUrl: "https://images.nasa.gov/docs/images.nasa.gov_api_docs.pdf",
      robotsUrl: null,
      termsUrl: "https://www.nasa.gov/nasa-brand-center/images-and-media/",
      scrapingPermitted: false,
      anonymousAccess: true,
      rateLimit: "undocumented; app limits to 1 request/second",
      notes: "Not all NASA media is public domain - licence shown per item.",
      checkedOn: "2026-10-21",
    },
    notes: "Mock provider used for browser preview.",
  },
  {
    id: "cleveland",
    name: "Cleveland Museum of Art",
    sourceName: "Cleveland Museum of Art Open Access",
    websiteUrl: "https://www.clevelandart.org",
    licenseHint: "CC0 for open-access works; otherwise copyrighted",
    enabledByDefault: true,
    requiresApiKey: false,
    hasApiKey: false,
    capabilities: {
      search: true,
      pagination: true,
      download: true,
      orientationFilter: false,
      sizeFilter: false,
      mediaTypeFilter: false,
      licenseFilter: false,
      colorFilter: false,
      sortOptions: ["relevance"],
      mediaTypes: ["artwork"],
    },
    policy: {
      accessMethod: "Open Access API",
      docsUrl: "https://openaccess-api.clevelandart.org/",
      robotsUrl: null,
      termsUrl: "https://www.clevelandart.org/open-access",
      scrapingPermitted: false,
      anonymousAccess: true,
      rateLimit: "undocumented public API; app limits to 2 requests/second",
      notes: "Only items with share_license_status CC0 expose a download URL.",
      checkedOn: "2026-10-21",
    },
    notes: "Mock provider used for browser preview.",
  },
  {
    id: "pexels",
    name: "Pexels",
    sourceName: "Pexels",
    websiteUrl: "https://www.pexels.com",
    licenseHint: "Pexels licence",
    enabledByDefault: false,
    requiresApiKey: true,
    hasApiKey: false,
    capabilities: {
      search: true,
      pagination: true,
      download: true,
      orientationFilter: true,
      sizeFilter: true,
      mediaTypeFilter: false,
      licenseFilter: false,
      colorFilter: false,
      sortOptions: ["relevance", "newest"],
      mediaTypes: ["photo"],
    },
    policy: {
      accessMethod: "Official API only (key required)",
      docsUrl: "https://www.pexels.com/api/documentation/",
      robotsUrl: "https://www.pexels.com/robots.txt",
      termsUrl: "https://www.pexels.com/terms-of-service/",
      scrapingPermitted: false,
      anonymousAccess: false,
      rateLimit: "documented per-plan (default 200 requests/hour)",
      notes: "robots.txt disallows *q=* (search URLs) and */api/v*; public-page adapter intentionally not implemented.",
      checkedOn: "2026-10-21",
    },
    notes: "Disabled until an API key is provided in Settings.",
  },
  {
    id: "pixabay",
    name: "Pixabay",
    sourceName: "Pixabay",
    websiteUrl: "https://pixabay.com",
    licenseHint: "Pixabay Content License (royalty-free; attribution not required)",
    enabledByDefault: false,
    requiresApiKey: true,
    hasApiKey: false,
    capabilities: {
      search: true,
      pagination: true,
      download: true,
      orientationFilter: true,
      sizeFilter: true,
      mediaTypeFilter: true,
      licenseFilter: false,
      colorFilter: true,
      sortOptions: ["relevance", "newest"],
      mediaTypes: ["photo", "illustration", "vector"],
    },
    policy: {
      accessMethod: "Official API only (key required)",
      docsUrl: "https://pixabay.com/api/docs/",
      robotsUrl: "https://pixabay.com/robots.txt",
      termsUrl: "https://pixabay.com/service/terms/",
      scrapingPermitted: false,
      anonymousAccess: false,
      rateLimit: "100 requests per 60 seconds; responses must be cached for 24 hours",
      notes: "API only. Responses are cached for 24 hours; basic keys expose scaled images up to 1280 px.",
      checkedOn: "2026-10-21",
    },
    notes: "Optional keyed source; mock does not call the real API.",
  },
  {
    id: "freepik",
    name: "Freepik (Magnific)",
    sourceName: "Freepik / Magnific stock library",
    websiteUrl: "https://www.freepik.com",
    licenseHint: "Freemium or premium Freepik licence; attribution and a plan may be required",
    enabledByDefault: false,
    requiresApiKey: true,
    hasApiKey: false,
    capabilities: {
      search: true,
      pagination: true,
      download: false,
      orientationFilter: true,
      sizeFilter: false,
      mediaTypeFilter: true,
      licenseFilter: false,
      colorFilter: false,
      sortOptions: ["relevance", "newest"],
      mediaTypes: ["photo", "vector", "illustration"],
    },
    policy: {
      accessMethod: "Official Magnific Resources API (key required)",
      docsUrl: "https://docs.magnific.com/api-reference/resources/get-all-resources",
      robotsUrl: "https://www.freepik.com/robots.txt",
      termsUrl: "https://www.freepik.com/terms-of-use",
      scrapingPermitted: false,
      anonymousAccess: false,
      rateLimit: "API-plan dependent; app limits to 1 request/second",
      notes: "Preview-only. The API returns licensed previews; downloads require a Freepik plan.",
      checkedOn: "2026-10-21",
    },
    notes: "Optional keyed source; mock does not call the real API. Results are view-only.",
  },
];

const SORTS = ["relevance", "newest"] as const;

function makeAssets(query: string, provider: string, page: number, count: number): Asset[] {
  const base = query.trim() || "stock";
  const mediaTypes = ["photo", "illustration", "artwork"];
  return Array.from({ length: count }, (_, index) => {
    const seed = page * 100 + index;
    const width = [1600, 1920, 2400, 1200, 3000][seed % 5];
    const height = [1067, 1080, 1600, 1600, 2000][seed % 5];
    const mediaType = mediaTypes[seed % mediaTypes.length];
    return {
      id: `${provider}:mock-${seed}`,
      provider,
      providerAssetId: `mock-${seed}`,
      title: `${base.replace(/\b\w/g, (c) => c.toUpperCase())} #${seed}`,
      description: `Synthetic placeholder used for browser preview of Stockcraft (${provider}).`,
      thumbnailUrl: placeholder(`${base} ${seed}`, 480, Math.round((480 * height) / width), (seed * 47) % 360),
      previewUrl: placeholder(`${base} ${seed}`, 900, Math.round((900 * height) / width), (seed * 47) % 360),
      pageUrl: `https://example.org/${provider}/mock-${seed}`,
      downloadUrl: null,
      creator: ["A. Okonkwo", "L. Moreau", "T. Nakamura", "S. Pereira", "J. Lindqvist"][seed % 5],
      creatorUrl: null,
      width,
      height,
      mimeType: "image/jpeg",
      licenseName: ["CC BY 4.0", "CC0 1.0", "Public domain", "CC BY-SA 4.0"][seed % 4],
      licenseUrl: "https://creativecommons.org/licenses/",
      attribution: `"${base}" by mock author, used for browser preview only.`,
      sourceName: provider,
      tags: [base, "mock", mediaType],
      mediaType,
      downloadAllowed: false,
      dominantColor: null,
      providerMetadata: { mock: "true" },
    } satisfies Asset;
  });
}

interface MockState {
  settings: AppSettings;
  favorites: Map<string, Asset>;
  collections: Collection[];
  collectionItems: Map<number, string[]>;
  library: Map<string, Asset>;
  downloads: DownloadRecord[];
  history: HistoryItem[];
  nextSearch: number;
  cancelled: Set<string>;
  timers: number[];
}

const state: MockState = {
  settings: {
    theme: "system",
    density: "standard",
    downloadDir: "C:\\Users\\Public\\Pictures\\Stockcraft",
    resultsPerPage: 24,
    enabledProviders: ["openverse", "wikimedia", "nasa", "cleveland"],
    cacheThumbnails: true,
    thumbnailCacheMb: 256,
    metadataCacheMinutes: 30,
    confirmOverwrite: true,
    writeAttributionSidecar: true,
    openFolderAfterDownload: false,
    safeSearch: true,
    reducedMotion: false,
    pexelsApiKey: "",
    pixabayApiKey: "",
    freepikApiKey: "",
    historyEnabled: true,
  },
  favorites: new Map(),
  collections: [],
  collectionItems: new Map(),
  library: new Map(),
  downloads: [],
  history: [],
  nextSearch: 1,
  cancelled: new Set(),
  timers: [],
};

function status(provider: ProviderInfo, over: Partial<ProviderStatusInfo> = {}): ProviderStatusInfo {
  return {
    provider: provider.id,
    name: provider.name,
    state: "ok",
    message: "browser preview",
    results: 0,
    total: null,
    latencyMs: 120,
    hasMore: false,
    failures: 0,
    circuitOpenUntilMs: null,
    lastSuccessMs: Date.now(),
    blocked: false,
    ...over,
  };
}

function provider(id: string): ProviderInfo | undefined {
  return MOCK_PROVIDERS.find((p) => p.id === id);
}

function saveAsset(asset: Asset): void {
  state.library.set(asset.id, asset);
}

export async function mockCall<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const a = (args ?? {}) as Record<string, any>;
  switch (command) {
    case "list_providers":
      return structuredClone(MOCK_PROVIDERS) as T;

    case "provider_status":
      return MOCK_PROVIDERS.map((p) => status(p, { state: p.enabledByDefault ? "idle" : "disabled" })) as T;

    case "reset_provider_health":
      return MOCK_PROVIDERS.map((p) => status(p, { state: "idle" })) as T;

    case "get_thumbnail_base":
      return "" as T;

    case "get_settings":
      return structuredClone(state.settings) as T;

    case "update_settings": {
      state.settings = { ...state.settings, ...(a.patch as Partial<AppSettings>) };
      return structuredClone(state.settings) as T;
    }

    case "start_search": {
      const request = a.request as SearchRequest;
      const searchId = `mock-search-${state.nextSearch++}`;
      const chosen = (request.providers?.length ? request.providers : state.settings.enabledProviders).filter(
        (id) => provider(id)?.enabledByDefault || provider(id)?.hasApiKey,
      );
      const providers = chosen.map((id) => provider(id)!).filter(Boolean);
      state.cancelled.delete(searchId);
      state.history.unshift({
        id: state.history.length + 1,
        query: request.query,
        providers: chosen.join(","),
        resultCount: chosen.length * 12,
        searchedAt: new Date().toISOString(),
      });
      providers.forEach((p, index) => {
        const timer = window.setTimeout(() => {
          if (state.cancelled.has(searchId)) return;
          const assets = makeAssets(request.query, p.id, 1, 12);
          emit("search://provider", {
            searchId,
            provider: p.id,
            assets,
            page: 1,
            hasMore: true,
            total: 120,
            status: status(p, { state: "ok", results: assets.length, total: 120, hasMore: true }),
            error: null,
            append: false,
          });
        }, 220 + index * 260);
        state.timers.push(timer);
      });
      const finish = window.setTimeout(() => {
        if (state.cancelled.has(searchId)) return;
        emit("search://finished", { searchId, resultCount: providers.length * 12, cancelled: false, errors: [] });
      }, 240 + providers.length * 260 + 300);
      state.timers.push(finish);
      return { searchId, query: request.query, providers: providers.map((p) => status(p, { state: "searching" })) } as T;
    }

    case "cancel_search": {
      state.cancelled.add(a.searchId as string);
      for (const timer of state.timers) window.clearTimeout(timer);
      state.timers = [];
      emit("search://finished", {
        searchId: a.searchId,
        resultCount: 0,
        cancelled: true,
        errors: [],
      });
      return undefined as T;
    }

    case "load_more": {
      const p = provider(a.provider as string);
      if (!p) throw { code: "unknown_provider", message: `Unknown provider ${a.provider}` };
      const assets = makeAssets("more", p.id, 2, 12);
      emit("search://provider", {
        searchId: a.searchId,
        provider: p.id,
        assets,
        page: 2,
        hasMore: false,
        total: 120,
        status: status(p, { state: "ok", results: assets.length, hasMore: false }),
        error: null,
        append: true,
      });
      return status(p, { state: "ok", results: assets.length, hasMore: false }) as T;
    }

    case "get_asset_details": {
      const found = state.library.get(`${a.provider}:${(a.assetId as string).split(":").pop()}`);
      if (found) return found as T;
      return makeAssets("detail", a.provider as string, 1, 1)[0] as T;
    }

    case "toggle_favorite": {
      const asset = a.asset as Asset;
      if (state.favorites.has(asset.id)) {
        state.favorites.delete(asset.id);
        const stored = state.library.get(asset.id);
        if (stored) state.library.set(asset.id, stored);
        return false as T;
      }
      state.favorites.set(asset.id, asset);
      saveAsset(asset);
      return true as T;
    }

    case "favorite_ids":
      return Array.from(state.favorites.keys()) as T;

    case "list_favorites":
      return Array.from(state.favorites.values()) as T;

    case "list_library": {
      const items = Array.from(state.library.values());
      const query = String(a.query ?? "").toLowerCase();
      return (query ? items.filter((i) => (i.title ?? "").toLowerCase().includes(query)) : items) as T;
    }

    case "list_collections":
      return structuredClone(state.collections) as T;

    case "create_collection": {
      const name = String(a.name ?? "").trim();
      if (!name) throw { code: "validation", message: "Collection name is required" };
      if (state.collections.some((c) => c.name.toLowerCase() === name.toLowerCase()))
        throw { code: "conflict", message: "A collection with that name already exists" };
      const now = new Date().toISOString();
      const collection: Collection = {
        id: state.collections.length + 1,
        name,
        description: String(a.description ?? ""),
        itemCount: 0,
        createdAt: now,
        updatedAt: now,
      };
      state.collections.push(collection);
      return structuredClone(collection) as T;
    }

    case "rename_collection": {
      const collection = state.collections.find((c) => c.id === a.id);
      if (!collection) throw { code: "not_found", message: "Collection not found" };
      collection.name = String(a.name ?? collection.name);
      collection.description = String(a.description ?? collection.description ?? "");
      collection.updatedAt = new Date().toISOString();
      return structuredClone(collection) as T;
    }

    case "delete_collection": {
      state.collections = state.collections.filter((c) => c.id !== a.id);
      state.collectionItems.delete(a.id as number);
      return undefined as T;
    }

    case "add_to_collection": {
      const collection = state.collections.find((c) => c.id === a.id);
      const asset = a.asset as Asset;
      if (!collection) throw { code: "not_found", message: "Collection not found" };
      saveAsset(asset);
      const items = new Set(state.collectionItems.get(collection.id) ?? []);
      items.add(asset.id);
      state.collectionItems.set(collection.id, Array.from(items));
      collection.itemCount = items.size;
      return structuredClone(collection) as T;
    }

    case "remove_from_collection": {
      const collection = state.collections.find((c) => c.id === a.id);
      if (!collection) throw { code: "not_found", message: "Collection not found" };
      const items = new Set(state.collectionItems.get(collection.id) ?? []);
      items.delete(a.assetId as string);
      state.collectionItems.set(collection.id, Array.from(items));
      collection.itemCount = items.size;
      return structuredClone(collection) as T;
    }

    case "list_collection_items": {
      const ids = new Set(state.collectionItems.get(a.collectionId) ?? []);
      return Array.from(state.library.values()).filter((asset) => ids.has(asset.id)) as T;
    }

    case "download_asset": {
      const asset = a.asset as Asset;
      const id = state.downloads.length + 1;
      const record: DownloadRecord = {
        id,
        assetId: asset.id,
        provider: asset.provider,
        title: asset.title ?? "untitled",
        filePath: `${state.settings.downloadDir}\\${asset.providerAssetId}.jpg`,
        sourceUrl: asset.pageUrl ?? null,
        licenseName: asset.licenseName ?? null,
        licenseUrl: asset.licenseUrl ?? null,
        bytes: 0,
        status: "running",
        error: null,
        createdAt: new Date().toISOString(),
        thumbnailUrl: asset.thumbnailUrl ?? null,
      };
      state.downloads.unshift(record);
      emit("download://progress", { downloadId: id, assetId: asset.id, received: 0, total: 512000, status: "running", filePath: record.filePath, error: null });
      window.setTimeout(() => {
        record.status = "done";
        record.bytes = 512000;
        emit("download://progress", { downloadId: id, assetId: asset.id, received: 512000, total: 512000, status: "done", filePath: record.filePath, error: null });
      }, 900);
      return id as T;
    }

    case "cancel_download": {
      const record = state.downloads.find((d) => d.id === a.downloadId);
      if (record && record.status === "running") {
        record.status = "cancelled";
        emit("download://progress", { downloadId: record.id, assetId: record.assetId, received: 0, total: 0, status: "cancelled", filePath: record.filePath, error: null });
      }
      return undefined as T;
    }

    case "list_downloads":
      return structuredClone(state.downloads) as T;

    case "clear_download_history":
      state.downloads = [];
      return 0 as T;

    case "download_path":
      return (state.downloads.find((d) => d.id === a.downloadId)?.filePath ?? "") as T;

    case "open_path":
    case "open_external_url":
      return undefined as T;

    case "validate_external_url":
      try {
        const url = new URL(String(a.url));
        return (url.protocol === "http:" || url.protocol === "https:") as T;
      } catch {
        return false as T;
      }

    case "pick_folder":
      return state.settings.downloadDir as T;

    case "search_history":
      return structuredClone(state.history.slice(0, a.limit ?? 12)) as T;

    case "clear_search_history":
      state.history = [];
      return undefined as T;

    case "delete_history_item":
      state.history = state.history.filter((h) => h.id !== a.id);
      return undefined as T;

    case "clear_cache":
      return 0 as T;

    case "cache_stats":
      return {
        thumbnailBytes: 0,
        thumbnailFiles: 0,
        metadataRows: 0,
        libraryItems: state.library.size,
        favorites: state.favorites.size,
        collections: state.collections.length,
        downloads: state.downloads.length,
        dbBytes: 0,
      } as T;

    case "get_diagnostics":
      return {
        appVersion: "1.0.1-browser-preview",
        os: "browser",
        arch: "web",
        dataDir: "(browser preview)",
        dbPath: "(browser preview)",
        thumbnailDir: "(browser preview)",
        logPath: "(browser preview)",
        providers: MOCK_PROVIDERS.map((p) => ({
          id: p.id,
          name: p.name,
          state: p.enabledByDefault ? "idle" : "disabled",
          message: "browser preview",
          failures: 0,
        })),
        counts: {
          thumbnailBytes: 0,
          thumbnailFiles: 0,
          metadataRows: 0,
          libraryItems: state.library.size,
          favorites: state.favorites.size,
          collections: state.collections.length,
          downloads: state.downloads.length,
          dbBytes: 0,
        },
      } as T;

    default:
      throw { code: "unknown_command", message: `Command ${command} is not available in browser preview` };
  }
}

export function mockReset(): void {
  for (const timer of state.timers) window.clearTimeout(timer);
  state.timers = [];
  state.favorites.clear();
  state.collections = [];
  state.collectionItems.clear();
  state.library.clear();
  state.downloads = [];
  state.history = [];
}

export const MOCK_STATE = state;
export const MOCK_SORTS = SORTS;
export { makeAssets as mockMakeAssets };
