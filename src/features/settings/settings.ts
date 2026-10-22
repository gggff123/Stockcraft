/** Settings drawer: appearance, search, downloads, sources (+policy/health), cache, privacy, diagnostics. */
import { api } from "../../lib/api";
import { copyText, html, qs, raw, render, registerActions, type Raw } from "../../lib/dom";
import { formatBytes, stateLabel } from "../../lib/format";
import { emit, state } from "../../lib/store";
import type { AppSettings, ProviderInfo } from "../../lib/types";
import { icon } from "../../components/icons";
import { button, confirmDialog, iconButton, selectField, switchRow, toast } from "../../components/ui";
import { updateSettings } from "../search/controller";

const SETTING_KINDS: Record<string, "string" | "number" | "boolean"> = {
  theme: "string",
  density: "string",
  resultsPerPage: "number",
  thumbnailCacheMb: "number",
  metadataCacheMinutes: "number",
  confirmOverwrite: "boolean",
  writeAttributionSidecar: "boolean",
  openFolderAfterDownload: "boolean",
  safeSearch: "boolean",
  reducedMotion: "boolean",
  historyEnabled: "boolean",
  pexelsApiKey: "string",
  pixabayApiKey: "string",
  freepikApiKey: "string",
};

const PROVIDER_KEY_FIELDS: Record<string, "pexelsApiKey" | "pixabayApiKey" | "freepikApiKey"> = {
  pexels: "pexelsApiKey",
  pixabay: "pixabayApiKey",
  freepik: "freepikApiKey",
};

const THEMES = [
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "system", label: "Match system" },
];

const DENSITIES = [
  { value: "cozy", label: "Cozy (larger)" },
  { value: "standard", label: "Standard" },
  { value: "dense", label: "Dense (smaller)" },
];

function numericOptions(values: number[], suffix = ""): { value: string; label: string }[] {
  return values.map((value) => ({ value: String(value), label: `${value}${suffix}` }));
}

function providerCard(provider: ProviderInfo): Raw {
  const status = state.statuses[provider.id];
  const enabled = state.enabledProviders.includes(provider.id);
  const keyField = PROVIDER_KEY_FIELDS[provider.id] ?? "";
  const keyValue = keyField ? readField(keyField) : "";

  return html`<div class="card">
    <div class="row between wrap">
      <div class="grow">
        <div class="tile-title">${provider.name}</div>
        <div class="tiny muted">${provider.policy.accessMethod}</div>
      </div>
      ${switchRow("Enabled", enabled, "toggle-provider", undefined, { provider: provider.id })}
    </div>
    <div class="tiny muted">
      ${status
        ? html`Status: <b>${stateLabel(status.state, status.results)}</b>${status.message ? html` — ${status.message}` : ""}${status.failures ? html` · ${status.failures} recent failure(s)` : ""}`
        : ""}
    </div>
    <dl class="meta-grid">
      <dt>Anonymous</dt>
      <dd>${provider.policy.anonymousAccess ? "yes — no account or key needed" : "no"}</dd>
      <dt>Scraping</dt>
      <dd>${provider.policy.scrapingPermitted ? "permitted (not used by this adapter)" : "not permitted / not used"}</dd>
      <dt>Rate limits</dt>
      <dd>${provider.policy.rateLimit}</dd>
      <dt>Licences</dt>
      <dd>${provider.licenseHint}</dd>
      <dt>Checked</dt>
      <dd>${provider.policy.checkedOn}</dd>
    </dl>
    <div class="tiny muted">${provider.policy.notes}</div>
    <div class="row wrap">
      <a href="${provider.policy.docsUrl ?? provider.websiteUrl}" data-action="open-source" data-url="${provider.policy.docsUrl ?? provider.websiteUrl}">API documentation</a>
      ${provider.policy.robotsUrl
        ? html`<a href="${provider.policy.robotsUrl}" data-action="open-source" data-url="${provider.policy.robotsUrl}">robots.txt</a>`
        : ""}
      ${provider.policy.termsUrl
        ? html`<a href="${provider.policy.termsUrl}" data-action="open-source" data-url="${provider.policy.termsUrl}">terms</a>`
        : ""}
      <a href="${provider.websiteUrl}" data-action="open-source" data-url="${provider.websiteUrl}">website</a>
    </div>
    ${keyField
      ? html`<div class="row">
          <input
            class="grow"
            type="password"
            id="key-${keyField}"
            data-action="noop-change"
            placeholder="${provider.name} API key (optional)"
            value="${keyValue}"
            aria-label="${provider.name} credential"
            autocomplete="off"
          />
          ${button({ action: "save-provider-key", label: "Save", variant: "ghost", size: "sm", data: { field: keyField, provider: provider.id } })}
        </div>`
      : ""}
    ${provider.requiresApiKey && !provider.hasApiKey
      ? html`<div class="tiny muted">
          Optional: ${provider.name} needs an API key and is disabled by default. Stockcraft never scrapes it. The key is
          stored only in your local database and is never written to logs.
        </div>`
      : ""}
    <div class="row">
      ${button({ action: "retry-provider", label: "Reset and retry", variant: "ghost", size: "sm", iconName: "refresh", data: { provider: provider.id } })}
      ${provider.capabilities.colorFilter ? html`<span class="tiny muted">native colour filter</span>` : ""}
      ${provider.capabilities.mediaTypeFilter ? html`<span class="tiny muted">native media-type filter</span>` : ""}
      ${provider.capabilities.licenseFilter ? html`<span class="tiny muted">native licence filter</span>` : ""}
    </div>
  </div>`;
}

export function renderSettingsPanel(): void {
  const host = qs("#panel-host");
  if (!host) return;
  const settings = state.settingsDraft ?? state.settings;
  if (!settings) {
    render(host, html`<div class="drawer-scrim" data-action="close-panel"></div><aside class="drawer" data-stop-click><div class="drawer-body"><p>Settings are still loading…</p></div></aside>`);
    return;
  }
  const cache = state.cache;
  const diagnostics = state.diagnostics;
  const enabledNames = state.providers.filter((p) => state.enabledProviders.includes(p.id)).map((p) => p.name);

  render(
    host,
    html`<div class="drawer-scrim" data-action="close-panel"></div>
      <aside class="drawer" role="dialog" aria-modal="true" aria-label="Settings" data-stop-click>
        <div class="drawer-head">
          <span style="color:var(--brand)">${icon("sliders", 20)}</span>
          <h2>Settings</h2>
          ${iconButton({ action: "close-panel", iconName: "close", label: "Close settings" })}
        </div>
        <div class="drawer-body">
          <div class="section">
            <h3>Appearance</h3>
            <div class="card">
              ${selectField("Theme", settings.theme, THEMES, "setting-value", { field: "theme" })}
              ${selectField("Grid density", settings.density, DENSITIES, "setting-value", { field: "density" })}
              ${switchRow("Reduce motion", settings.reducedMotion, "setting-toggle", "Disables non-essential animation.", { field: "reducedMotion" })}
            </div>
          </div>

          <div class="section">
            <h3>Search</h3>
            <div class="card">
              ${selectField("Results per source", String(settings.resultsPerPage), numericOptions([12, 24, 48]), "setting-value", { field: "resultsPerPage" })}
              ${switchRow("Filter mature content", settings.safeSearch, "setting-toggle", "Applied natively by Openverse and Pixabay; others filter what they can locally.", { field: "safeSearch" })}
              ${switchRow("Keep search history", settings.historyEnabled, "setting-toggle", "Stores the last queries locally for the suggestion list.", { field: "historyEnabled" })}
            </div>
          </div>

          <div class="section">
            <h3>Downloads</h3>
            <div class="card">
              <div class="row between wrap">
                <div class="grow">
                  <div class="tile-title">Default folder</div>
                  <div class="tiny muted mono">${settings.downloadDir || "not set — you will be asked on first download"}</div>
                </div>
                ${button({ action: "pick-download-folder", label: "Change…", variant: "ghost", size: "sm", iconName: "folder" })}
              </div>
              ${switchRow("Never overwrite existing files", settings.confirmOverwrite, "setting-toggle", "When on, a numbered suffix is added instead of replacing an existing file.", { field: "confirmOverwrite" })}
              ${switchRow("Write attribution sidecar", settings.writeAttributionSidecar, "setting-toggle", "Saves attribution.txt and a JSON metadata file next to each download.", { field: "writeAttributionSidecar" })}
              ${switchRow("Open folder after download", settings.openFolderAfterDownload, "setting-toggle", undefined, { field: "openFolderAfterDownload" })}
            </div>
          </div>

          <div class="section">
            <h3>Sources &amp; policy</h3>
            ${state.providers.map((provider) => raw(providerCard(provider)))}
            <div class="tiny muted">
              Every adapter carries the policy record it was approved against, including the date it was checked. A source
              whose terms or robots.txt forbid the access we need is shipped disabled — that is why the Pexels
              public-page adapter does not exist in Stockcraft.
            </div>
            <div class="tiny muted">
              Pixabay and Freepik (Magnific) are optional keyed APIs and stay disabled until you save a key. Freepik
              results are preview-only because reuse needs a Freepik licence; Stockcraft links to the source instead of
              downloading it. Google Images is not included: google.com/robots.txt disallows <span class="mono">/search</span>
              and <span class="mono">/imgres</span>, and Google's Custom Search JSON API is closed to new customers and ends
              on 2027-01-01. Stockcraft does not scrape Google.
            </div>
          </div>

          <div class="section">
            <h3>Cache &amp; storage</h3>
            <div class="card">
              <dl class="meta-grid">
                <dt>Thumbnails</dt>
                <dd>${cache ? `${formatBytes(cache.thumbnailBytes)} · ${cache.thumbnailFiles} file(s)` : "unknown"}</dd>
                <dt>Metadata rows</dt>
                <dd>${cache ? cache.metadataRows : "unknown"}</dd>
                <dt>Saved assets</dt>
                <dd>${cache ? `${cache.libraryItems} (${cache.favorites} favourite(s), ${cache.collections} collection(s))` : "unknown"}</dd>
                <dt>Downloads</dt>
                <dd>${cache ? cache.downloads : "unknown"}</dd>
                <dt>Database</dt>
                <dd>${cache ? formatBytes(cache.dbBytes) : "unknown"}</dd>
              </dl>
              ${selectField("Thumbnail cache limit", String(settings.thumbnailCacheMb), numericOptions([64, 128, 256, 512, 1024], " MB"), "setting-value", { field: "thumbnailCacheMb" })}
              ${selectField("Metadata cache TTL", String(settings.metadataCacheMinutes), numericOptions([5, 15, 30, 60, 240], " min"), "setting-value", { field: "metadataCacheMinutes" })}
              <div class="row wrap">
                ${button({ action: "clear-cache", label: "Clear thumbnails", variant: "ghost", size: "sm", iconName: "trash", data: { scope: "thumbnails" } })}
                ${button({ action: "clear-cache", label: "Clear metadata", variant: "ghost", size: "sm", iconName: "trash", data: { scope: "metadata" } })}
                ${button({ action: "clear-cache", label: "Clear everything", variant: "ghost", size: "sm", iconName: "trash", data: { scope: "all" } })}
                ${button({ action: "refresh-cache-stats", label: "Refresh", variant: "ghost", size: "sm", iconName: "refresh" })}
              </div>
            </div>
          </div>

          <div class="section">
            <h3>Privacy</h3>
            <div class="card">
              <div class="tiny">
                Stockcraft is local-first. There is no account, no telemetry and no Stockcraft server. Your search terms are
                sent only to the sources you enable:
              </div>
              <ul class="tiny muted" style="margin:0;padding-left:18px">
                ${enabledNames.map((name) => raw(`<li>${name}</li>`))}
              </ul>
              <div class="tiny muted">
                Remote images are fetched by the Rust backend through a caching proxy so provider requests stay in one
                place. Remote text is always rendered as plain text — never as HTML or script.
              </div>
            </div>
          </div>

          <div class="section">
            <h3>About &amp; diagnostics</h3>
            <div class="card">
              <dl class="meta-grid">
                <dt>Version</dt>
                <dd>${diagnostics?.appVersion ?? "unknown"}</dd>
                <dt>Platform</dt>
                <dd>${diagnostics ? `${diagnostics.os} ${diagnostics.arch}` : "unknown"}</dd>
                <dt>Data folder</dt>
                <dd class="mono">${diagnostics?.dataDir ?? "—"}</dd>
                <dt>Database</dt>
                <dd class="mono">${diagnostics?.dbPath ?? "—"}</dd>
                <dt>Thumbnails</dt>
                <dd class="mono">${diagnostics?.thumbnailDir ?? "—"}</dd>
                <dt>Log file</dt>
                <dd class="mono">${diagnostics?.logPath ?? "—"}</dd>
              </dl>
              <div class="row wrap">
                ${button({ action: "refresh-diagnostics", label: "Refresh", variant: "ghost", size: "sm", iconName: "refresh" })}
                ${button({ action: "copy-diagnostics", label: "Copy diagnostics", variant: "ghost", size: "sm", iconName: "check" })}
                <a href="https://api.openverse.org/v1/" data-action="open-source" data-url="https://api.openverse.org/v1/">provider documentation</a>
              </div>
            </div>
          </div>
        </div>
      </aside>`,
  );
}

function readField(field: string): string {
  return String((state.settingsDraft as unknown as Record<string, unknown>)?.[field] ?? "");
}

async function saveCredential(field: keyof AppSettings, value: string): Promise<void> {
  await updateSettings({ [field]: value } as Partial<AppSettings>);
  try {
    state.providers = await api.listProviders();
  } catch {
    /* non-critical */
  }
  emit("panel");
}

export function registerSettingsActions(): void {
  registerActions({
    "show-settings": {
      click: () => {
        state.panel = "settings";
        state.previewId = null;
        emit("panel", "header", "preview");
        void refreshStats();
      },
    },
    "close-panel": {
      click: () => {
        state.panel = "none";
        emit("panel", "header");
      },
    },
    "setting-toggle": {
      change: (element) => {
        const field = element.dataset.field ?? "";
        if (SETTING_KINDS[field] !== "boolean") return;
        const value = (element as HTMLInputElement).checked;
        void updateSettings({ [field]: value } as Partial<AppSettings>);
      },
    },
    "setting-value": {
      change: (element) => {
        const field = element.dataset.field ?? "";
        const kind = SETTING_KINDS[field];
        if (!kind || kind === "boolean") return;
        const raw_ = (element as HTMLSelectElement | HTMLInputElement).value;
        if (kind === "number") {
          const numeric = Number(raw_);
          if (!Number.isFinite(numeric)) return;
          void updateSettings({ [field]: numeric } as Partial<AppSettings>);
        } else {
          void updateSettings({ [field]: raw_ } as Partial<AppSettings>);
        }
      },
    },
    "save-provider-key": {
      click: async (element) => {
        const field = (element.dataset.field ?? "") as keyof AppSettings;
        if (!SETTING_KINDS[field as string] || SETTING_KINDS[field as string] !== "string") return;
        const input = qs<HTMLInputElement>(`#key-${field}`);
        if (!input) return;
        await saveCredential(field, input.value.trim());
        toast({
          title: input.value.trim() ? "Credential saved locally" : "Credential cleared",
          message: "Stored in your local Stockcraft database only — never logged.",
          tone: "success",
          timeoutMs: 3500,
        });
      },
    },
    "clear-cache": {
      click: async (element) => {
        const scope = (element.dataset.scope ?? "all") as "thumbnails" | "metadata" | "all";
        const confirmed = await confirmDialog({
          title: `Clear ${scope === "all" ? "all caches" : scope}?`,
          message:
            scope === "metadata"
              ? "Cached provider responses will be re-fetched on the next search."
              : "Cached thumbnail files will be deleted from disk and re-fetched when needed. Downloads are not affected.",
          confirmLabel: "Clear cache",
        });
        if (!confirmed) return;
        try {
          const freed = await api.clearCache(scope);
          await refreshStats();
          toast({ title: "Cache cleared", message: `${formatBytes(freed)} freed.`, tone: "success", timeoutMs: 3500 });
        } catch (error) {
          toast({ title: "Could not clear the cache", message: error instanceof Error ? error.message : String(error), tone: "error" });
        }
      },
    },
    "refresh-cache-stats": { click: () => void refreshStats() },
    "refresh-diagnostics": { click: () => void refreshStats() },
    "copy-diagnostics": {
      click: async () => {
        const text = JSON.stringify(
          {
            diagnostics: state.diagnostics,
            cache: state.cache,
            settings: { ...state.settings, pexelsApiKey: state.settings?.pexelsApiKey ? "set (hidden)" : "unset" },
            providers: state.providers.map((p) => ({ id: p.id, policy: p.policy })),
          },
          null,
          2,
        );
        if (await copyText(text)) {
          toast({ title: "Diagnostics copied", message: "Secrets are redacted in the copied report.", tone: "success", timeoutMs: 3000 });
        } else {
          toast({ title: "Clipboard unavailable", message: "Could not copy diagnostics.", tone: "warn" });
        }
      },
    },
    "noop-change": { change: () => undefined },
  });
}

export async function refreshStats(): Promise<void> {
  try {
    const [cache, diagnostics] = await Promise.all([api.cacheStats(), api.diagnostics()]);
    state.cache = cache;
    state.diagnostics = diagnostics;
    emit("panel", "sidebar");
  } catch (error) {
    toast({ title: "Could not read diagnostics", message: error instanceof Error ? error.message : String(error), tone: "warn" });
  }
}

export { readField };
