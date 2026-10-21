/** Header, sidebar, toolbar, provider rail and status footer. */
import { html, raw, render, preserveFocus, qs } from "../../lib/dom";
import { formatBytes, mediaTypeLabel, stateLabel, summarizeProviders } from "../../lib/format";
import { state } from "../../lib/store";
import type { ProviderInfo } from "../../lib/types";
import { brandMark, icon } from "../../components/icons";
import { button, iconButton, removableChip, selectField } from "../../components/ui";
import { CATEGORIES, COLORS, LICENSES, MEDIA_TYPES, ORIENTATIONS, SORTS, activeFilterCount } from "./options";

export const SEARCH_INPUT_ID = "stockcraft-search-input";

function themeIcon(): "sun" | "moon" {
  const theme = state.settings?.theme ?? "system";
  if (theme === "dark") return "moon";
  if (theme === "light") return "sun";
  const prefersDark = typeof window !== "undefined" && window.matchMedia?.("(prefers-color-scheme: dark)").matches;
  return prefersDark ? "moon" : "sun";
}

export function renderHeader(): void {
  const host = qs("#header-host");
  if (!host) return;
  preserveFocus(host, () => {
    render(
      host,
      html`<header class="hdr" role="banner">
        <button type="button" class="brand" data-action="go-search" aria-label="Stockcraft home">
          <span class="brand-mark">${brandMark(26)}</span>
          <span class="brand-text">
            <span class="brand-name">Stockcraft</span>
            <span class="brand-tag">free image foundry</span>
          </span>
        </button>
        ${iconButton({
          action: "toggle-sidebar",
          iconName: "panel",
          label: "Show or hide filters",
          title: "Filters (F)",
          className: "sidebar-toggle",
          pressed: state.sidebarOpen,
        })}
        <div class="search-wrap">
          <form class="search" data-action="search-form" role="search">
            <span aria-hidden="true" style="color:var(--text-dim);display:grid">${icon("search", 18)}</span>
            <input
              id="${SEARCH_INPUT_ID}"
              type="text"
              data-action="search-input"
              value="${state.draft}"
              placeholder="Search free photos, illustrations and artwork — try “blue abstract background”"
              aria-label="Search images"
              autocomplete="off"
              spellcheck="false"
              role="combobox"
              aria-expanded="${state.suggestOpen}"
              aria-controls="search-suggest"
            />
            ${state.draft
              ? html`<button
                  type="button"
                  class="search-clear"
                  data-action="clear-search"
                  aria-label="Clear search"
                  title="Clear search"
                >
                  ${icon("close", 15)}
                </button>`
              : ""}
            ${button({ action: "search-form", label: "Search", variant: "primary", size: "sm", iconName: "search", submit: true })}
          </form>
          ${state.suggestOpen && state.suggestions.length
            ? html`<div class="suggest" id="search-suggest" role="listbox" aria-label="Recent searches">
                <div class="suggest-head">Recent searches</div>
                ${state.suggestions.map(
                  (item, index) => html`<button
                    type="button"
                    class="suggest-item"
                    role="option"
                    aria-selected="${index === state.suggestIndex}"
                    data-action="use-suggestion"
                    data-query="${item}"
                    data-active="${index === state.suggestIndex}"
                  >
                    ${icon("clock", 15)}<span class="grow">${item}</span>
                  </button>`,
                )}
                <div class="row" style="padding:6px 12px 10px;justify-content:space-between">
                  <span class="tiny muted">Enter to search · Esc to dismiss</span>
                  <button type="button" class="btn btn-sm btn-quiet" data-action="clear-history">Clear history</button>
                </div>
              </div>`
            : ""}
        </div>
        <nav class="hdr-nav" aria-label="Main">
          ${iconButton({
            action: "show-favorites",
            iconName: "heart",
            label: "Favourites",
            title: "Favourites (Ctrl+F)",
            pressed: state.view === "favorites",
            badge: state.favoriteIds.size,
          })}
          ${iconButton({
            action: "show-collections",
            iconName: "layers",
            label: "Collections",
            title: "Collections (Ctrl+Shift+C)",
            pressed: state.panel === "collections",
            badge: state.collections.length,
          })}
          ${iconButton({
            action: "show-downloads",
            iconName: "download",
            label: "Downloads",
            title: "Downloads (Ctrl+D)",
            pressed: state.panel === "downloads",
            badge: state.downloads.length,
          })}
          ${iconButton({ action: "show-history", iconName: "clock", label: "Search history", title: "Search history (Ctrl+H)" })}
          <span class="hdr-divider" aria-hidden="true"></span>
          ${iconButton({
            action: "toggle-theme",
            iconName: themeIcon(),
            label: "Switch theme",
            title: "Switch light / dark theme",
          })}
          ${iconButton({
            action: "show-settings",
            iconName: "sliders",
            label: "Settings",
            title: "Settings (Ctrl+,)",
            pressed: state.panel === "settings",
          })}
        </nav>
      </header>`,
    );
  });
}

function providerNote(provider: ProviderInfo): string {
  if (provider.requiresApiKey && !provider.hasApiKey) return "API key needed";
  if (!provider.policy.anonymousAccess && !provider.hasApiKey) return "key needed";
  if (!provider.policy.scrapingPermitted && provider.requiresApiKey) return "API only";
  return "";
}

export function renderSidebar(): void {
  const host = qs("#sidebar-host");
  if (!host) return;
  const filters = state.filters;
  const enabled = new Set(state.enabledProviders);

  render(
    host,
    html`<aside class="sidebar" id="sidebar" aria-label="Categories and filters">
      <div class="side-head">
        <span class="side-title">Browse</span>
        ${activeFilterCount(filters) > 0
          ? button({ action: "reset-filters", label: "Reset", variant: "quiet", size: "sm" })
          : ""}
      </div>
      <div class="cat-list" role="list">
        ${CATEGORIES.map((category) =>
          raw(html`<button
            type="button"
            class="cat-item"
            role="listitem"
            data-action="choose-category"
            data-category="${category.id}"
            aria-pressed="${category.id === filters.category}"
            ${category.disabled ? "disabled" : ""}
            title="${category.disabled ? `Not available: ${category.note ?? "unsupported"}` : category.term ? `Adds “${category.term}” to your search` : ""}"
          >
            ${icon(category.icon, 16)}
            <span class="grow">${category.label}</span>
            ${category.disabled ? html`<span class="cat-note">n/a</span>` : ""}
          </button>`),
        )}
      </div>

      <div class="section">
        <div class="side-head"><span class="side-title">Sources</span></div>
        <div class="chip-row">
          ${state.providers.map((provider) => {
            const isEnabled = enabled.has(provider.id);
            const note = providerNote(provider);
            return raw(html`<button
              type="button"
              class="chip"
              data-action="toggle-provider"
              data-provider="${provider.id}"
              aria-pressed="${isEnabled}"
              title="${provider.name} — ${provider.policy.accessMethod}${note ? ` (${note})` : ""}"
            >
              ${provider.name}${note ? html`<span class="tiny">· ${note}</span>` : ""}
            </button>`);
          })}
        </div>
        <div class="tiny muted">
          Queries are sent only to the sources you enable. ${enabled.size} of ${state.providers.length} enabled.
        </div>
      </div>

      <div class="section">
        <div class="side-head">
          <span class="side-title">Filters</span>
          <span class="tiny muted">applied locally</span>
        </div>
        ${selectField("Orientation", filters.orientation, ORIENTATIONS, "set-orientation")}
        ${selectField("Image type", filters.mediaType, MEDIA_TYPES, "set-media-type")}
        ${selectField("Licence", filters.license, LICENSES, "set-license")}
        <div class="field">
          <span class="field-label">Minimum size (px)</span>
          <div class="row">
            <input
              type="number"
              min="0"
              step="100"
              id="filter-min-width"
              data-action="set-min-width"
              value="${filters.minWidth || ""}"
              placeholder="width"
              aria-label="Minimum width in pixels"
            />
            <input
              type="number"
              min="0"
              step="100"
              id="filter-min-height"
              data-action="set-min-height"
              value="${filters.minHeight || ""}"
              placeholder="height"
              aria-label="Minimum height in pixels"
            />
          </div>
        </div>
        <div class="field">
          <span class="field-label">Colour</span>
          <div class="swatch-row">
            ${COLORS.map((colour) =>
              raw(html`<button
                type="button"
                class="swatch"
                data-action="set-color"
                data-color="${colour.value}"
                aria-pressed="${filters.color === colour.value}"
                aria-label="${colour.value === "any" ? "Any colour" : `${colour.label} colour`}"
                title="${colour.label}"
                style="background:${colour.swatch === "transparent" ? "var(--surface)" : colour.swatch}"
              >
                ${colour.value === "any" ? icon("close", 12) : ""}
              </button>`),
            )}
          </div>
          <div class="tiny muted">
            Matched against the dominant colour published by each source. Results without colour data are hidden while this
            filter is active.
          </div>
        </div>
        ${activeFilterCount(filters) > 0
          ? html`<div class="tiny muted">${activeFilterCount(filters)} filter(s) active — applied to already loaded results.</div>`
          : ""}
      </div>

      ${state.settings
        ? html`<div class="section">
            <div class="side-head"><span class="side-title">Cache</span></div>
            <div class="tiny muted">
              ${state.cache ? `${formatBytes(state.cache.thumbnailBytes)} of thumbnails cached (${state.cache.thumbnailFiles} files).` : "Cache statistics unavailable."}
            </div>
            ${button({ action: "clear-thumbnails", label: "Clear thumbnail cache", variant: "ghost", size: "sm", iconName: "trash" })}
          </div>`
        : ""}
    </aside>`,
  );
}

export function renderToolbar(): void {
  const host = qs("#toolbar-host");
  if (!host) return;
  const filters = state.filters;
  const category = CATEGORIES.find((c) => c.id === filters.category) ?? CATEGORIES[0];
  const total = Object.values(state.totals).reduce((sum, value) => sum + (value ?? 0), 0);
  const libraryView = state.view !== "search";

  render(
    host,
    html`<div class="toolbar" role="toolbar" aria-label="Result tools">
      <div class="result-count">
        ${state.view === "favorites"
          ? html`<b>${state.assets.length}</b> favourite${state.assets.length === 1 ? "" : "s"}`
          : state.view === "collection"
            ? html`<b>${state.assets.length}</b> saved image${state.assets.length === 1 ? "" : "s"} in this collection`
            : state.searching
              ? html`<b>${state.assets.length}</b> loaded — searching…`
              : html`<b>${state.assets.length}</b> loaded${total ? html` · <span title="Provider-reported totals are approximate">~${total} available</span>` : ""}`}
      </div>

      ${libraryView
        ? html`<label class="field" style="flex-direction:row;align-items:center;gap:6px;width:230px">
            ${icon("search", 15)}
            <input
              type="text"
              id="library-search"
              data-action="library-search"
              value="${state.draft}"
              placeholder="Filter saved images"
              aria-label="Filter saved images"
            />
          </label>`
        : ""}

      ${category.id !== "all"
        ? html`<span class="tiny muted">category: <b>${category.label}</b>${category.term ? html` · adds “${category.term}”` : ""}${category.mediaType ? html` · local type filter` : ""}</span>`
        : ""}

      ${state.ignoredFilters.length
        ? html`<span class="tiny muted" title="${state.ignoredFilters.join("; ")}">${state.ignoredFilters.length} filter(s) not supported by some sources</span>`
        : ""}

      <div class="chip-row">
        ${filters.orientation !== "any"
          ? removableChip(ORIENTATIONS.find((o) => o.value === filters.orientation)?.label ?? filters.orientation, { filter: "orientation" })
          : ""}
        ${filters.mediaType !== "any"
          ? removableChip(MEDIA_TYPES.find((o) => o.value === filters.mediaType)?.label ?? filters.mediaType, { filter: "mediaType" })
          : ""}
        ${filters.license !== "any"
          ? removableChip(LICENSES.find((o) => o.value === filters.license)?.label ?? filters.license, { filter: "license" })
          : ""}
        ${filters.color !== "any"
          ? removableChip(COLORS.find((o) => o.value === filters.color)?.label ?? filters.color, { filter: "color" })
          : ""}
        ${filters.minWidth > 0 ? removableChip(`≥ ${filters.minWidth}px wide`, { filter: "minWidth" }) : ""}
        ${filters.minHeight > 0 ? removableChip(`≥ ${filters.minHeight}px tall`, { filter: "minHeight" }) : ""}
        ${filters.category !== "all"
          ? removableChip(`category: ${category.label}`, { filter: "category" })
          : ""}
      </div>

      <div class="row" style="margin-left:auto">
        ${raw(`<label class="field" style="min-width:150px"><span class="field-label">Sort</span><select data-action="set-sort">${SORTS.map(
          (s) => `<option value="${s.value}"${s.value === state.sort ? " selected" : ""}>${s.label}</option>`,
        ).join("")}</select></label>`)}
        ${raw(`<label class="field" style="min-width:132px"><span class="field-label">Grid</span><select data-action="set-density">
            ${[
              { value: "cozy", label: "Cozy" },
              { value: "standard", label: "Standard" },
              { value: "dense", label: "Dense" },
            ]
              .map((d) => `<option value="${d.value}"${d.value === (state.settings?.density ?? "standard") ? " selected" : ""}>${d.label}</option>`)
              .join("")}
          </select></label>`)}
        ${libraryView
          ? button({ action: "go-search", label: "Back to search", variant: "ghost", size: "sm", iconName: "search" })
          : ""}
      </div>
    </div>`,
  );
}

export function renderRail(): void {
  const host = qs("#rail-host");
  if (!host) return;
  const statuses = state.providers
    .filter((provider) => state.enabledProviders.includes(provider.id))
    .map((provider) => state.statuses[provider.id] ?? null)
    .filter((value): value is NonNullable<typeof value> => Boolean(value));

  render(
    host,
    html`<div class="provider-rail" role="status" aria-label="Source status">
      ${statuses.length
        ? statuses.map((status) =>
            raw(html`<span class="provider-pill" data-state="${status.state}" title="${status.message || stateLabel(status.state, status.results)}">
              <span class="dot" aria-hidden="true"></span>
              <b>${status.name}</b>
              <span>${stateLabel(status.state, status.results)}</span>
              ${status.total ? html`<span class="tiny muted">(of ${status.total})</span>` : ""}
              ${status.blocked || status.state === "circuit_open" || status.state === "unavailable" || status.state === "rate_limited"
                ? html`<button type="button" data-action="retry-provider" data-provider="${status.provider}">retry</button>`
                : ""}
            </span>`),
          )
        : html`<span class="tiny muted">No sources enabled. Open Settings → Sources, or enable one in the sidebar.</span>`}
      ${state.providers.some((p) => !state.enabledProviders.includes(p.id))
        ? html`<button type="button" class="chip" data-action="show-settings" data-section="sources">
            ${icon("plus", 13)} Add sources
          </button>`
        : ""}
    </div>`,
  );
}

export function renderFooter(): void {
  const host = qs("#footer-host");
  if (!host) return;
  const statuses = state.providers
    .filter((provider) => state.enabledProviders.includes(provider.id))
    .map((provider) => state.statuses[provider.id])
    .filter(Boolean);
  const summary = summarizeProviders(statuses.map((s) => ({ name: s.name, state: s.state })));
  const pending = Object.values(state.loadingMore).filter(Boolean).length;

  render(
    host,
    html`<footer class="status-rail" role="contentinfo">
      <div class="row grow" style="gap:8px;min-width:0">
        ${state.searching || pending ? html`<span class="spinner" aria-hidden="true"></span>` : ""}
        <span class="tiny muted" style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">
          ${state.statusMessage}
          ${summary ? html` · ${summary}` : ""}
          ${pending ? html` · loading more from ${pending} source(s)` : ""}
        </span>
      </div>
      ${state.searching
        ? button({ action: "cancel-search", label: "Cancel search", variant: "ghost", size: "sm", iconName: "close" })
        : state.view === "search" && state.assets.length
          ? button({ action: "search-again", label: "Refresh", variant: "ghost", size: "sm", iconName: "refresh" })
          : ""}
      ${state.mock ? raw(`<span class="chip" title="Browser preview uses synthetic placeholder images">mock data</span>`) : ""}
      <span class="tiny muted">${state.providers.filter((p) => state.enabledProviders.includes(p.id)).length} source(s) enabled</span>
    </footer>`,
  );
}

export function renderBanner(): void {
  const host = qs("#banner-host");
  if (!host) return;
  if (!state.mock) {
    render(host, "");
    return;
  }
  render(
    host,
    html`<div class="banner" role="note">
      ${icon("info", 16)}
      <span class="grow">
        <b>Browser preview.</b> Stockcraft is running outside Tauri, so all images and results on this screen are synthetic
        placeholders and no network request is made. Launch the desktop app for real provider results.
      </span>
    </div>`,
  );
}

export function mediaTypeText(mediaType: string): string {
  return mediaTypeLabel(mediaType);
}
