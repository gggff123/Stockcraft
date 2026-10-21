/** TypeScript mirrors of the Rust models exposed over Tauri IPC (camelCase). */

export type ProviderState =
  | "idle"
  | "searching"
  | "ok"
  | "empty"
  | "unavailable"
  | "timeout"
  | "rate_limited"
  | "circuit_open"
  | "restricted"
  | "disabled"
  | "malformed";

export type Orientation = "landscape" | "portrait" | "square" | "any";

export interface Asset {
  id: string;
  provider: string;
  providerAssetId: string;
  title?: string | null;
  description?: string | null;
  thumbnailUrl?: string | null;
  previewUrl?: string | null;
  pageUrl?: string | null;
  downloadUrl?: string | null;
  creator?: string | null;
  creatorUrl?: string | null;
  width?: number | null;
  height?: number | null;
  mimeType?: string | null;
  licenseName?: string | null;
  licenseUrl?: string | null;
  attribution?: string | null;
  sourceName?: string | null;
  tags: string[];
  mediaType: string;
  downloadAllowed: boolean;
  dominantColor?: string | null;
  createdAt?: string | null;
  providerMetadata: Record<string, string>;
}

export interface AssetFilters {
  orientation: Orientation;
  minWidth: number;
  minHeight: number;
  mediaType: string;
  license: string;
  color: string;
  category: string;
}

export interface SearchRequest {
  query: string;
  providers: string[];
  limit: number;
  page: number;
  filters: AssetFilters;
  sort: string;
}

export interface ProviderPolicy {
  accessMethod: string;
  docsUrl?: string | null;
  robotsUrl?: string | null;
  termsUrl?: string | null;
  scrapingPermitted: boolean;
  anonymousAccess: boolean;
  rateLimit: string;
  notes: string;
  checkedOn: string;
}

export interface ProviderCapabilities {
  search: boolean;
  pagination: boolean;
  download: boolean;
  orientationFilter: boolean;
  sizeFilter: boolean;
  mediaTypeFilter: boolean;
  licenseFilter: boolean;
  colorFilter: boolean;
  sortOptions: string[];
  mediaTypes: string[];
}

export interface ProviderInfo {
  id: string;
  name: string;
  sourceName: string;
  websiteUrl: string;
  licenseHint: string;
  enabledByDefault: boolean;
  requiresApiKey: boolean;
  hasApiKey: boolean;
  capabilities: ProviderCapabilities;
  policy: ProviderPolicy;
  notes: string;
  contactUrl?: string | null;
}

export interface ProviderStatusInfo {
  provider: string;
  name: string;
  state: ProviderState;
  message: string;
  results: number;
  total?: number | null;
  latencyMs?: number | null;
  hasMore: boolean;
  failures: number;
  circuitOpenUntilMs?: number | null;
  lastSuccessMs?: number | null;
  blocked: boolean;
}

export interface SearchStart {
  searchId: string;
  query: string;
  providers: ProviderStatusInfo[];
}

export interface ProviderPayload {
  searchId: string;
  provider: string;
  assets: Asset[];
  page: number;
  hasMore: boolean;
  total?: number | null;
  status: ProviderStatusInfo;
  error?: string | null;
  append: boolean;
}

export interface SearchFinished {
  searchId: string;
  resultCount: number;
  cancelled: boolean;
  errors: { provider: string; message: string }[];
}

export interface DownloadRecord {
  id: number;
  assetId: string;
  provider: string;
  title: string;
  filePath: string;
  sourceUrl?: string | null;
  licenseName?: string | null;
  licenseUrl?: string | null;
  bytes: number;
  status: "queued" | "running" | "done" | "error" | "cancelled";
  error?: string | null;
  createdAt: string;
  thumbnailUrl?: string | null;
}

export interface Collection {
  id: number;
  name: string;
  description?: string | null;
  itemCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface HistoryItem {
  id: number;
  query: string;
  providers: string;
  resultCount: number;
  searchedAt: string;
}

export interface AppSettings {
  theme: "light" | "dark" | "system";
  density: "cozy" | "standard" | "dense";
  downloadDir: string;
  resultsPerPage: number;
  enabledProviders: string[];
  cacheThumbnails: boolean;
  thumbnailCacheMb: number;
  metadataCacheMinutes: number;
  confirmOverwrite: boolean;
  writeAttributionSidecar: boolean;
  openFolderAfterDownload: boolean;
  safeSearch: boolean;
  reducedMotion: boolean;
  pexelsApiKey: string;
  pixabayApiKey: string;
  freepikApiKey: string;
  historyEnabled: boolean;
}

export interface CacheStats {
  thumbnailBytes: number;
  thumbnailFiles: number;
  metadataRows: number;
  libraryItems: number;
  favorites: number;
  collections: number;
  downloads: number;
  dbBytes: number;
}

export interface Diagnostics {
  appVersion: string;
  os: string;
  arch: string;
  dataDir: string;
  dbPath: string;
  thumbnailDir: string;
  logPath: string;
  providers: { id: string; name: string; state: ProviderState; message: string; failures: number }[];
  counts: CacheStats;
}
