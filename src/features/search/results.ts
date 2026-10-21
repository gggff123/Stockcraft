/** Results area: justified image grid, incremental appends, empty/error/loading states. */
import { html, qs, raw, render, type Raw } from "../../lib/dom";
import { pluralize } from "../../lib/format";
import { state, takeGridRequest } from "../../lib/store";
import type { Asset } from "../../lib/types";
import { brandMark, icon } from "../../components/icons";
import { button, notice } from "../../components/ui";
import { matchesFilters, sortAssets } from "./options";
import { skeletonTile, tile } from "./tile";

const EXAMPLE_QUERIES = [
  "mountain landscape",
  "minimalist workspace",
  "blue abstract background",
  "coffee photography",
  "modern architecture",
];

const renderedIds = new Set<string>();

function visibleAssets(): { visible: Asset[]; hidden: number; noColorData: number } {
  const filtered = state.assets.filter((asset) => matchesFilters(asset, state.filters));
  const hidden = state.assets.length - filtered.length;
  const noColorData =
    state.filters.color === "any" ? 0 : state.assets.filter((asset) => !asset.dominantColor).length;
  return { visible: sortAssets(filtered, state.sort), hidden, noColorData };
}

function welcome(): Raw {
  return raw(html`
    <div class="state-block">
      <div class="state-art" style="background:linear-gradient(150deg,var(--brand-500),var(--brand-800));border-radius:24px;display:grid;place-items:center">
        ${brandMark(84)}
      </div>
      <h2>Find free images from several sources at once</h2>
      <p>
        Stockcraft searches Openverse, Wikimedia Commons, NASA and the Cleveland Museum of Art —
        no account and no API key required. Every result keeps its original page, creator and licence
        information. Optional keyed sources are available in Settings: Pexels, Pixabay and Freepik
        (Magnific, preview-only).
      </p>
      <div class="chip-row" style="justify-content:center">
        ${EXAMPLE_QUERIES.map(
          (query) => raw(`<button type="button" class="chip" data-action="quick-search" data-query="${query}">${query}</button>`),
        )}
      </div>
      ${notice(
        "<b>Privacy:</b> your query is sent only to the sources you enable (see Settings → Sources). Nothing is sent to Stockcraft.",
      )}
    </div>
  `);
}

function failureState(): Raw {
  const failed = state.providers
    .filter((provider) => state.enabledProviders.includes(provider.id))
    .map((provider) => ({ provider, status: state.statuses[provider.id] }))
    .filter((entry) => entry.status && ["unavailable", "timeout", "rate_limited", "circuit_open", "malformed", "restricted"].includes(entry.status.state));

  return raw(html`
    <div class="state-block">
      <div style="opacity:.55">${icon("warning", 44)}</div>
      <h2>${state.assets.length ? "Some sources could not be reached" : "No source returned results"}</h2>
      <p>
        ${state.assets.length
          ? "Results from the remaining sources are shown below."
          : "Every enabled source failed for this search. This is usually temporary, or the source may be rate limiting this device."}
      </p>
      <ul style="text-align:left;max-width:640px;margin:0;padding-left:20px" class="tiny muted">
        ${failed.map(
          (entry) => raw(html`<li><b>${entry.provider.name}</b> — ${entry.status?.message || entry.status?.state}</li>`),
        )}
      </ul>
      <div class="state-actions">
        ${button({ action: "retry-all", label: "Retry search", variant: "primary", iconName: "refresh" })}
        ${button({ action: "show-settings", label: "Source settings", variant: "ghost", iconName: "sliders" })}
      </div>
      <div class="tiny muted">
        Source websites:
        ${state.providers
          .filter((p) => state.enabledProviders.includes(p.id))
          .map((p) => raw(` <a href="${p.websiteUrl}" data-action="open-source" data-url="${p.websiteUrl}">${p.name}</a> `))}
      </div>
    </div>
  `);
}

function noResultsState(): Raw {
  return raw(html`
    <div class="state-block">
      <div style="opacity:.5">${icon("search", 42)}</div>
      <h2>No results for “${state.submittedQuery}”</h2>
      <p>
        ${state.ignoredFilters.length
          ? "Some filters are not supported by the currently enabled sources, so results may be narrower than expected."
          : "Try a broader query, a different spelling, or enable more sources."}
      </p>
      <div class="chip-row" style="justify-content:center">
        ${EXAMPLE_QUERIES.map(
          (query) => raw(`<button type="button" class="chip" data-action="quick-search" data-query="${query}">${query}</button>`),
        )}
      </div>
      <div class="state-actions">
        ${button({ action: "retry-all", label: "Retry search", variant: "ghost", iconName: "refresh" })}
        ${button({ action: "show-settings", label: "Enable more sources", variant: "ghost", iconName: "sliders" })}
      </div>
    </div>
  `);
}

function filteredOutState(hidden: number): Raw {
  return raw(html`
    <div class="state-block">
      <div style="opacity:.5">${icon("filter", 42)}</div>
      <h2>All ${hidden} results are hidden by your filters</h2>
      <p>Relax the orientation, size, licence or colour filter to see them again.</p>
      <div class="state-actions">${button({ action: "reset-filters", label: "Reset filters", variant: "primary", iconName: "refresh" })}</div>
    </div>
  `);
}

function libraryEmptyState(): Raw {
  const isFavorites = state.view === "favorites";
  return raw(html`
    <div class="state-block">
      <div style="opacity:.5">${icon(isFavorites ? "heart" : "layers", 42)}</div>
      <h2>${isFavorites ? "No favourites yet" : "This collection is empty"}</h2>
      <p>
        ${isFavorites
          ? "Use the heart button on any result to save it here. Favourites, collections, history and settings are stored locally in SQLite."
          : "Add images from a search result's detail view — the “Add to collection” button saves them here."}
      </p>
      <div class="state-actions">${button({ action: "go-search", label: "Start searching", variant: "primary", iconName: "search" })}</div>
    </div>
  `);
}

function tileMarkup(asset: Asset): string {
  const removable = state.view === "collection";
  return tile(asset, { removable }).value;
}

export function renderResults(): void {
  const scroll = qs("#results-scroll");
  const grid = qs("#grid");
  const stateHost = qs("#state-host");
  const loadMoreHost = qs("#load-more");
  if (!scroll || !grid || !stateHost || !loadMoreHost) return;

  if (state.fatalError) {
    render(
      stateHost,
      html`<div class="state-block">
        <div style="opacity:.6">${icon("warning", 44)}</div>
        <h2>Stockcraft could not start</h2>
        <p>${state.fatalError}</p>
        <div class="state-actions">${button({ action: "reload", label: "Reload interface", iconName: "refresh", variant: "primary" })}</div>
      </div>`,
    );
    render(grid, "");
    render(loadMoreHost, "");
    return;
  }

  const { reset } = takeGridRequest();
  const isLibrary = state.view === "favorites" || state.view === "collection";

  if (isLibrary && !state.assets.length) {
    renderedIds.clear();
    render(grid, "");
    render(loadMoreHost, "");
    render(stateHost, libraryEmptyState().value);
    return;
  }

  if (state.view === "search" && !state.submittedQuery && !state.searching) {
    renderedIds.clear();
    render(grid, "");
    render(loadMoreHost, "");
    render(stateHost, welcome().value);
    return;
  }

  const { visible, hidden, noColorData } = visibleAssets();
  const searching = state.searching && !state.assets.length;

  render(stateHost, "");
  if (!visible.length) {
    if (searching) {
      if (reset) {
        renderedIds.clear();
        render(grid, Array.from({ length: 12 }, (_, index) => skeletonTile(index % 3 === 0 ? "3 / 2" : "4 / 3").value).join(""));
      }
      render(loadMoreHost, "");
      return;
    }
    if (hidden > 0 && state.assets.length) {
      renderedIds.clear();
      render(grid, "");
      render(loadMoreHost, "");
      render(stateHost, filteredOutState(hidden).value);
      return;
    }
    const anyFailure = Object.values(state.statuses).some((status) =>
      ["unavailable", "timeout", "rate_limited", "circuit_open", "malformed"].includes(status.state),
    );
    renderedIds.clear();
    render(grid, "");
    render(loadMoreHost, "");
    render(stateHost, (anyFailure ? failureState() : noResultsState()).value);
    return;
  }

  if (reset) renderedIds.clear();
  const fresh = visible.filter((asset) => !renderedIds.has(asset.id));
  for (const asset of fresh) renderedIds.add(asset.id);

  if (reset) {
    render(grid, fresh.map(tileMarkup).join(""));
  } else if (fresh.length) {
    grid.insertAdjacentHTML("beforeend", fresh.map(tileMarkup).join(""));
  }

  const hiddenNote = hidden > 0
    ? html`<div class="tiny muted" style="text-align:center">
        ${hidden} result${hidden === 1 ? "" : "s"} hidden by local filters${noColorData ? html` (${noColorData} without colour data)` : ""}.
        <button type="button" class="btn btn-sm btn-quiet" data-action="reset-filters">Show all</button>
      </div>`
    : "";

  const canLoadMore = isLibrary
    ? []
    : Object.entries(state.hasMore).filter(([provider, more]) => more && state.enabledProviders.includes(provider));
  const anyLoading = canLoadMore.some(([provider]) => state.loadingMore[provider]);

  render(
    loadMoreHost,
    html`${hiddenNote}
      ${canLoadMore.length
        ? html`<div class="row" style="justify-content:center;gap:8px;flex-wrap:wrap">
            ${button({ action: "load-more-all", label: anyLoading ? "Loading…" : `Load more (${canLoadMore.length} source${canLoadMore.length === 1 ? "" : "s"})`, variant: "ghost", iconName: anyLoading ? undefined : "plus", disabled: anyLoading })}
          </div>`
        : html`<div class="tiny muted" style="text-align:center;padding-top:8px">
            ${isLibrary ? "These saved images live in your local library." : state.searching ? "searching other sources…" : `End of results from the current sources${state.cancelled ? " (search cancelled)" : ""}.`}
          </div>`}
      <div class="tiny muted" style="text-align:center;padding-top:4px">
        ${pluralize(state.assets.length, "asset")} in the current view. Thumbnails are previews — the original page holds the full-resolution file.
      </div>`,
  );
}

/** Attaches capture-phase load/error handling for lazy thumbnails (images do not bubble). */
export function initImageStates(): void {
  const scroll = qs("#results-scroll");
  if (!scroll) return;
  scroll.addEventListener(
    "load",
    (event) => {
      const target = event.target;
      if (!(target instanceof HTMLImageElement)) return;
      const media = target.closest<HTMLElement>(".tile-media");
      if (!media) return;
      media.dataset.state = "loaded";
      media.querySelector(".skeleton")?.remove();
      target.classList.add("loaded");
    },
    true,
  );
  scroll.addEventListener(
    "error",
    (event) => {
      const target = event.target;
      if (!(target instanceof HTMLImageElement)) return;
      const media = target.closest<HTMLElement>(".tile-media");
      if (!media) return;
      media.dataset.state = "error";
      media.querySelector(".skeleton")?.remove();
      target.remove();
    },
    true,
  );
}

export function clearRenderedIds(): void {
  renderedIds.clear();
}
