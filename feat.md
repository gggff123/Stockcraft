# Stockcraft — Feature Inventory & Verification Record

Stockcraft is a local-first Windows desktop application for discovering, previewing, organising and
downloading free images from several independent sources. It is built with Tauri 2, a Rust backend,
and a TypeScript + Vite frontend.

**This file is the single source of truth for project status.** It records what was actually run,
not merely what exists in the source tree.

Last verification pass: **2026-10-21** (Windows 10/11 x64, WebView2 155, packaged release build).

> **Release status:** version `0.1.0` is published as a GitHub Release at
> <https://github.com/gggff123/Stockcraft/releases/latest> (tag `v0.1.0`), with the NSIS installer
> and a SHA-256 checksum file. See section 13 for the full publishing record.

---

## 0. Snapshot

| Item | Value |
| --- | --- |
| App version | `0.1.0` |
| Verified platform | Windows x64 (release executable and NSIS installer) |
| Anonymous working sources | **Openverse, Wikimedia Commons, NASA Image Library, Cleveland Museum of Art** |
| Optional keyed sources | **Pexels official API; Pixabay official API; Freepik / Magnific Resources API (preview-only)** — disabled by default, no live keys tested |
| Excluded sources | **Pexels public pages, Freepik public pages, Google Images** (provider `robots.txt`/API restrictions) |
| Removed source | **Art Institute of Chicago** (IIIF previews blocked by Cloudflare) |
| App icon | Supplied cross-hatched gorilla artwork; generated PNG/ICO/ICNS and mobile icon sets |
| Rust tests | **65 unit + 1 integration = 66 passed, 0 failed** |
| Frontend tests | **19 passed** |
| Release executable | `src-tauri/target/release/stockcraft.exe` — 7,689,216 bytes |
| Installer | `src-tauri/target/release/bundle/nsis/Stockcraft_0.1.0_x64-setup.exe` — 3,159,695 bytes |
| Install/uninstall | Verified on the current build: silent install, launch, clean uninstall |

### Verified end-to-end

* Packaged release search for **“sunrise”**: **81 results from 4 sources**; all four provider chips
  reported result counts and visible thumbnails loaded.
* Current NSIS installer: silent install succeeded, the installed app launched with the new gorilla
  icon, and the uninstaller removed the installation directory.
* The Sources panel in the packaged app shows Pixabay and Freepik (Magnific) cards, their policy
  records, API-key fields and “disabled until a key is saved” states.
* Pagination, search cancellation, collection CRUD, settings persistence, cache clearing, provider
  health reset, keyboard shortcuts, diagnostics copy, download cancellation and filename safety were
  each exercised manually in earlier recorded runs and are listed in section 10.2.
* Narrow **900 × 600** window: settings drawer fits, scrolls vertically, no horizontal overflow.

### Not verified

* Native folder-picker dialog interaction (`rfd::FileDialog`).
* Live Pexels, Pixabay and Freepik / Magnific API keys. All three adapters compile, are policy-gated
  and have fixture tests, but were never called with a real key.
* Freepik / Magnific's `filters[ids]` detail lookup (the search response parser is fixture-tested).
* macOS and Linux packaging/runtime.
* A full screen-reader/accessibility audit.

---

## 1. Status markers and evidence rules

| Marker | Meaning |
| --- | --- |
| `[x]` | Implemented **and** verified by an automated test or a manual run recorded in this file. |
| `[-]` | Implemented in code, but no recorded test/manual verification yet. |
| `[ ]` | Planned / not implemented. |
| `[!]` | Blocked, removed, unavailable, or dependent on external access/credentials. |

Rules honoured here:

1. A feature is only `[x]` after it was executed. Compiling is not verification.
2. “Implemented” (`[-]`) and “verified” (`[x]`) are reported separately.
3. No provider is described as working unless it returned real results in a recorded run.
4. Remote metadata is never invented to fill a gap; failed providers are shown as failed.

---

## 2. Provider inventory & policy verification

Verification date: **2026-10-21**. Requests used a descriptive User-Agent
(`Stockcraft/0.1 (+image discovery desktop app)`). All core providers are public, documented APIs;
no website or search-results page is scraped.

| Provider | Access method | Anonymous | Licence | Status | Evidence |
| --- | --- | --- | --- | --- | --- |
| **Openverse** | REST API `https://api.openverse.org/v1/images/` | Yes | Per-item CC / public domain + attribution | `[x]` | Live search in release and installed builds |
| **Wikimedia Commons** | MediaWiki Action API `commons.wikimedia.org/w/api.php` | Yes | Per-file `extmetadata` | `[x]` | Live search in release and installed builds |
| **NASA Image Library** | `https://images-api.nasa.gov/search` | Yes | NASA media guidelines; not all public domain | `[x]` | Live search; 35 MB original download cancelled successfully |
| **Cleveland Museum of Art** | `https://openaccess-api.clevelandart.org/` | Yes | `share_license_status` (CC0 for open access) | `[x]` | Live search in release and installed builds |
| **Pexels — public pages** | HTML scraping | n/a | Pexels licence | `[!]` | Excluded: `robots.txt` disallows the needed URLs |
| **Pexels — official API** | `https://api.pexels.com/v1/search` | No — key required | Pexels licence | `[-]` | Optional keyed adapter; fixture/key-gate tests, no live key tested |
| **Pixabay — official API** | `https://pixabay.com/api/` | No — key required | Pixabay Content License | `[-]` | Optional keyed adapter; fixture/key/colour tests, no live key tested |
| **Freepik / Magnific — Resources API** | `https://api.magnific.com/v1/resources` | No — key required | Freemium or premium Freepik licence | `[-]` | Optional **preview-only** adapter; fixture/key/licence tests, no live key tested |
| **Freepik — public pages** | HTML scraping | n/a | Freepik licence | `[!]` | Excluded: `robots.txt` disallows `/search?`, `/*/search?` and download paths |
| **Google Images** | HTML scraping | n/a | Varies by result | `[!]` | Excluded: `robots.txt` disallows `/search` and `/imgres`; Custom Search API is closed |
| **Art Institute of Chicago** | API + IIIF | Yes | `is_public_domain` | `[!]` | Removed: IIIF previews returned Cloudflare 403 |
| **Metropolitan Museum of Art** | `collectionapi.metmuseum.org` | Yes | CC0 where `isPublicDomain` | `[ ]` | Anonymous access investigated; adapter not implemented |
| Unsplash / Flickr / Smithsonian / Rijksmuseum / Europeana | Varies | Mostly no | Varies | `[ ]` | Require API keys and/or OAuth; not implemented |

### 2.1 Pexels decision (recorded 2026-10-21)

`https://www.pexels.com/robots.txt` contains, for `User-Agent: *`:

```
Disallow: *q=*
Disallow: *page=21..25
Disallow: */api/v*
Disallow: *orientation=*
Disallow: *size=*
Disallow: *color=*
Disallow: */download/*
```

A public-page search adapter would need exactly those URLs (search query strings, filters, paging and
download paths). Stockcraft therefore does **not** implement it and does **not** claim Pexels as a
working anonymous source.

A key-based adapter exists for the official API. It is disabled until the user saves a key in
**Settings → Sources & policy**, performs no scraping, and is clearly labelled “Optional”. Because no
live key was supplied, its live behaviour remains unverified.

### 2.2 Art Institute of Chicago removal (recorded 2026-10-21)

* The metadata API (`https://api.artic.edu/api/v1/artworks/search`) returned usable records.
* Every IIIF preview request to
  `https://www.artic.edu/iiif/2/<id>/full/400,/0/default.jpg` returned **HTTP 403** with
  `cf-mitigated: challenge` (Cloudflare), including attempts with a browser User-Agent and referer.
* In the packaged app, **12 of 12** AIC thumbnails failed; the grid showed broken previews.
* The adapter, registry entry, default provider list, browser mock and welcome copy were removed.
  Old persisted settings may still contain `"artic"` in `enabledProviders`; the registry ignores
  unknown ids safely.

### 2.3 Pixabay policy (recorded 2026-10-21)

* The official API is `https://pixabay.com/api/`; a free key is required, the documented limit is
  **100 requests per 60 seconds**, and API responses must be **cached for 24 hours**. Pixabay permits
  hotlinking only temporarily, so thumbnails go through Stockcraft's local proxy/cache.
* `https://pixabay.com/robots.txt` disallows the filter query strings a public search adapter would
  need. Stockcraft therefore has no Pixabay page scraper.
* The adapter maps the UI filters to native Pixabay parameters: `image_type`, `orientation`,
  `min_width`, `min_height`, `colors`, `safesearch` and `order`. It maps the UI palette to Pixabay's
  colour names (`teal → turquoise`, `purple → lilac`, `grey → gray`).
* The search orchestrator forces a **1,440-minute** metadata TTL for Pixabay even when the global
  metadata TTL is lower or disabled. This is required by Pixabay's API terms; the provider card
  states it.
* Basic API keys may expose only scaled images (up to 1280 px). Full-resolution URLs require Pixabay
  approval. The adapter prefers `imageURL` / `fullHDURL` when present, falls back to `largeImageURL`,
  and records the scaled fallback in provider metadata and the provider notes.
* Status: optional, disabled until a key is saved. Parser, key-gate and colour-mapping tests pass;
  the 24-hour cache rule is implemented in the search orchestrator but is not separately unit-tested.
  An unauthenticated endpoint probe returned **HTTP 400** (missing key). **No live key was tested.**

### 2.4 Freepik / Magnific policy (recorded 2026-10-21)

* Freepik rebranded its product line as **Magnific** in April 2026. The documented stock endpoint is
  `GET https://api.magnific.com/v1/resources`, authenticated with `X-Magnific-API-Key`. API access is
  aimed at paid/enterprise plans and keys are created in the Magnific dashboard.
* `https://www.freepik.com/robots.txt` disallows `/search?`, `/*/search?`, `/download/`,
  `/download-file/`, `/trk-download/` and `/api/video/*`. No public-page adapter exists.
* The API returns licensed **preview** images, not free downloads. The adapter therefore always sets
  `download_allowed = false`, exposes no download URL, and directs the user to the Freepik source
  page and licence. It never downloads Freepik assets.
* Status: optional, disabled until a key is saved. Search-response parsing, key gating and
  preview-only enforcement are fixture-tested. An unauthenticated endpoint probe returned **HTTP 401**
  (missing key). **No live key was tested.** The `filters[ids]` detail lookup has not been exercised
  live.

### 2.5 Google Images exclusion (recorded 2026-10-21)

* `https://www.google.com/robots.txt` contains `Disallow: /search` and `Disallow: /imgres` for
  `User-agent: *`. Scraping Google Images search results is therefore not permitted.
* Google's Custom Search JSON API is **closed to new customers** and is scheduled to be discontinued
  on **2027-01-01**, so it is not a viable new-provider route for Stockcraft.
* Google Images results also carry heterogeneous, often unknown licences; a download-oriented adapter
  would need per-result licence verification that the search page does not provide.
* Stockcraft contains **no Google scraping code and no Google provider**. The Settings panel explains
  the exclusion rather than presenting a non-working control.

### 2.6 Policy rules implemented in code

* Every adapter returns a `ProviderPolicy` record (`access_method`, `docs_url`, `robots_url`,
  `terms_url`, `scraping_permitted`, `anonymous_access`, `rate_limit`, `notes`, `checked_on`).
  Settings displays it verbatim.
* Only documented public APIs are used. No undocumented endpoints, proxies, identity rotation,
  CAPTCHA solving or robots.txt circumvention.
* Descriptive User-Agent, conservative timeouts, per-provider minimum request intervals.
* HTTP `403`, `451` and other policy responses are classified as **policy blocks**, are not retried,
  and open the provider circuit immediately. `429` honours a capped `Retry-After` and also opens the
  circuit.
* Downloads use only direct URLs published by the provider; the item licence is stored and shown.

---

## 3. Architecture (actual paths)

```
src/                              Vite + TypeScript frontend (no UI framework)
  main.ts                         bootstrap, global shortcuts, event subscriptions
  components/
    icons.ts                      inline SVG icon set; raster brand mark from public/icon.png
    ui.ts                         modal, toast, skeleton, button, field primitives
  features/
    search/
      chrome.ts                   header, sidebar, category and filter chrome
      controller.ts               search submission, suggestions, pagination, view switching
      options.ts                  categories, sorts, filters, local matching, sorting
      results.ts                  results grid, empty/error states, welcome copy
      tile.ts                     asset card, thumbnail proxy, favourite action
    preview/preview.ts            asset detail modal
    collections/collections.ts    favourites + named collections views
    downloads/downloads.ts        download list, progress, cancel, reveal
    settings/settings.ts          settings, provider policy/health, cache, diagnostics
  lib/
    api.ts                        typed Tauri IPC bridge
    dom.ts                        html`` escaping template + render helpers
    format.ts                     byte/date/plural formatting, safe filename helpers
    mock.ts                       browser-only 7-provider mock (4 anonymous + 3 optional keyed)
    store.ts                      tiny reactive store
    types.ts                      shared wire types
  styles/
    tokens.css                    colour, spacing and typography tokens
    app.css                       layout and component styles
src-tauri/                        Rust backend: all networking, filesystem and SQLite work
  src/
    main.rs                       Windows entry point
    lib.rs                        Tauri builder, paths, rotating log, service wiring
    state.rs                      AppState, provider snapshots, thumbnail URI scheme
    commands.rs                   single Tauri command surface (NOT a commands/ directory)
    error.rs                      typed AppError -> { code, message, detail }
    models.rs                     Asset, SearchRequest, AppSettings, statuses, diagnostics
    util.rs                       URL safety, safe filenames, HTML stripping, colour naming
    providers/
      mod.rs                      ImageProvider trait, registry, capabilities, policy records
      http.rs                     shared reqwest client, retries, policy/rate-limit classification
      rate_limit.rs               per-provider minimum interval limiter
      health.rs                   circuit breaker, cooldown, half-open probe, manual reset
      openverse.rs
      wikimedia.rs
      nasa.rs
      cleveland.rs
      pexels.rs                   optional keyed adapter only
      pixabay.rs                  optional keyed adapter; forced 24 h metadata cache
      freepik.rs                  optional keyed, preview-only Magnific adapter
    search/
      mod.rs                      sessions, cancellation, cache, events, pagination, filtering
      dedup.rs                    canonical id/page/image duplicate detection
    downloads/mod.rs              streaming, progress, cancel, atomic write, sidecars, limits
    database/mod.rs               rusqlite schema, migrations and repositories (no repos.rs)
    cache/mod.rs                  thumbnail proxy/cache, LRU eviction, metadata TTL helpers
  build.rs                        Windows Common-Controls v6 resource linking for test binaries
  tests/resource_manifest.rs      asserts the v6 manifest is embedded
tests/frontend.test.ts            19 Vitest + jsdom tests
assets/                           supplied gorilla source art, legacy SVG mascot, generated app icons
scripts/                          icon regeneration wrapper (`npm run icons`)
```

Design rules:

* The frontend performs no unrestricted networking. It calls Rust commands and listens to Tauri
  events. Remote images are fetched by Rust through a caching proxy:
  `http://thumb.localhost/...` on Windows, `thumb://localhost/...` on other platforms.
* Providers implement the shared `ImageProvider` trait. Provider-specific JSON stays inside the
  adapter; the orchestrator only sees `Asset` and `ProviderPage`.
* Errors cross IPC as `{ code, message, detail }` and are surfaced per provider.
* No accounts, analytics, telemetry or Stockcraft server.

---

## 4. Feature checklist

### 4.1 Foundation

* `[x]` Toolchain inspected and verified: Rust `stable-x86_64-pc-windows-gnu`, MinGW-w64 at
  `C:\mingw64`, Node 24, npm 12, WebView2 Runtime 155.
* `[x]` Tauri 2 + Vite + TypeScript scaffold with dev/build scripts.
* `[x]` `npm run typecheck`, `npm run test`, `npm run build`, `cargo test`, `cargo fmt --check` and
  `cargo clippy --all-targets -- -D warnings` all pass.
* `[x]` `feat.md` maintained as the status record (this rewrite).
* `[x]` Production NSIS bundle built and install/uninstall verified.

### 4.2 Application shell & branding

* `[x]` Design tokens and a blue-primary palette; light/dark/system theme support is present.
* `[-]` Theme switch and theme persistence were not separately exercised in the recorded runs.
  Persistence was verified for grid density, reduced motion and results-per-source.
* `[x]` Branding: Stockcraft wordmark and the supplied hand-inked, cross-hatched gorilla-with-picture-
  cards artwork. `npm run icons` regenerates PNG/ICO/ICNS and mobile sets from
  `assets/stockcraft-icon-source.png`, then copies the 256 px icon to `public/icon.png`. The same ICO
  is configured as the NSIS `installerIcon` and `uninstallerIcon`. The new mark was verified in the
  release title bar, header, welcome screen, installed app and setup-file icon.
* `[x]` Header: branding, global search field, submit, clear, favourites/collections/downloads/
  settings buttons, theme switch.
* `[x]` Category navigation renders “All images” plus 12 categories. **Vectors is deliberately
  disabled** with `n/a` because none of the four anonymous default providers offers vector search.
  The optional keyed Pixabay/Freepik adapters can return vectors, but the fixed category list does not
  yet expose a vector-only view (recorded under limitations).
* `[x]` Filter/sort semantics are unit-tested: orientation, minimum width/height, media type,
  licence, colour, category, active-filter count, relevance/newest/largest sorting.
* `[-]` Filter-toolbar wiring and active-filter chips were not separately recorded beyond the
  underlying unit tests.
* `[x]` Status bar reports per-provider state, result counts, errors and cancellation during real
  searches.
* `[x]` Settings drawer at 900 × 600 fits the viewport, scrolls vertically and has no horizontal
  overflow.
* `[x]` Keyboard shortcuts verified: `/`, `Ctrl+K`, `Ctrl+H`, `Ctrl+Shift+C`, `Ctrl+D`, `Ctrl+,`,
  `Ctrl+F`, `Escape`.
* `[-]` `Ctrl+R` (load more) and bare `F` (toggle sidebar) are implemented but were not part of the
  recorded shortcut pass.
* `[-]` Focus-visible styling exists, but no full keyboard or screen-reader audit was performed.

### 4.3 Search

* `[x]` Explicit search submission (Enter/button); suggestions are separate and debounced.
* `[-]` Clear-search button and its state reset were not separately recorded.
* `[x]` Search cancellation works while a request is active and immediately after submission; the UI
  recovers and accepts the next search.
* `[x]` Concurrent multi-provider search with a global cap of **4** in-flight provider calls.
* `[x]` Progressive per-provider results streamed over Tauri events.
* `[x]` Per-provider pagination: “Load more” increased the recorded search from 48 to 85 results.
* `[x]` Stable asset ids `provider:providerAssetId`; canonical page/image dedup is unit-tested,
  including tracking-parameter removal.
* `[-]` Cross-provider dedup was not observed with a real duplicate image; only fixture tests exist.
* `[x]` Local post-filtering for orientation, size, media type, licence and colour, with provider
  capability reporting so unsupported filters are not implied to be server-side.
* `[x]` Local newest/largest sorting unit-tested; relevance order is left untouched.
* `[x]` Search history storage, cap and deletion are unit-tested in the database layer.
* `[-]` History UI interactions (reuse/delete/clear) were not separately recorded.
* `[-]` Safe-search is implemented: Openverse receives `mature=false`, Pixabay receives
  `safesearch=true`, and assets marked `mature=true` are filtered locally. Its live effect was not
  specifically exercised.
* `[-]` Empty/error copy exists, but only provider failure parsing is covered by tests.

### 4.4 Results grid

* `[x]` Results grid renders real provider thumbnails with provider badge, creator, title and
  licence/source actions (observed in release and installed builds).
* `[x]` Aspect-ratio metadata is used for tile sizing; screenshots show a justified grid.
* `[-]` Lazy loading, skeleton shimmer and the broken-thumbnail fallback are implemented but were not
  forced in a recorded run (the installed search happened to have 0 broken thumbnails).
* `[x]` Favourite toggle, preview and download actions are present and were used during manual runs.
* `[-]` No oversized card borders and restrained transitions are design intentions; reduced motion
  was persisted, but no visual regression test exists.

### 4.5 Preview

* `[x]` Detail modal renders large preview, title, creator, source, dimensions, format, licence,
  attribution and action buttons (verified by screenshot and manual use).
* `[x]` All remote text is escaped or rendered as text; `components/dom.ts` escaping and nested
  template behaviour are covered by 4 frontend tests.
* `[x]` Original source page links are validated before opening.
* `[-]` The explicit “thumbnail is a preview, not the original file” label was not separately
  recorded.

### 4.6 Downloads

* `[x]` Streaming download with progress events, cancellation, temp file and atomic rename.
* `[x]` Download cancellation verified with a **35 MB NASA original**: progress stopped and no
  completed file was produced.
* `[x]` Safe filenames: path separators, traversal, Windows reserved names, control characters,
  trailing dots/spaces and length are handled; a regression test prevents `.jpg.jpg`.
* `[x]` Duplicate-image extensions are stripped before the correct extension is appended.
* `[x]` Extension selection from MIME type, then URL path, then `jpg`, is unit-tested.
* `[x]` Overwrite protection adds ` (1)`, ` (2)`, … when “Never overwrite” is on; unit-tested.
  When the setting is off, an existing file is replaced.
* `[x]` Optional `-attribution.txt` and JSON sidecar are written next to the image; the test asserts
  source, licence and the warning to check the licence before reuse.
* `[x]` Limits and validation enforced in code: 200 MB per download, 32 MB free-space reserve and an
  `image/*` content-type requirement. Thumbnail caching adds a 12 MB cap and magic-byte signature
  checks; HTML disguised as an image is rejected by a unit test. The 200 MB download path was not
  reached live.
* `[x]` Download records are stored in SQLite and listed in the downloads view; storage/clearing is
  unit-tested.
* `[-]` Native folder picker (`rfd::FileDialog`) was not exercised interactively.
* `[-]` Persisted custom download folder was not verified across restart.
* `[-]` “Reveal in folder” is path-restricted and implemented, but the OS file-manager launch was not
  part of the recorded pass.
* `[x]` UI copy never claims a download grants unrestricted reuse.

### 4.7 Favourites & collections

* `[x]` SQLite-backed favourites and named collections with membership; round-trip and duplicate-name
  rejection are unit-tested.
* `[x]` Manual collection flow verified: create, add image, open, rename, remove item, delete with
  confirmation.
* `[x]` Browse favourites and collection contents in the main grid layout; remove items.
* `[-]` Text filtering of saved assets was not separately recorded.
* `[x]` Stable `asset_id` and canonical page URL are stored, not just the thumbnail URL.
* `[-]` Graceful degradation for a deleted remote asset was not forced; the fallback code exists.

### 4.8 Caching & performance

* `[x]` Thumbnail cache on disk with LRU eviction and configurable size cap; eviction and stats are
  unit-tested and cache clearing was verified live.
* `[x]` Metadata cache with expiry; TTL behaviour is unit-tested and clearing was verified live.
* `[x]` Cache statistics and scoped clearing (thumbnails / metadata / all) report freed bytes.
  Recorded run: metadata **15 → 0 rows**; thumbnails **16,029,613 bytes / 80 files → 0**.
* `[x]` Provider concurrency cap of **4** is honoured in the recorded 4-source search; the thumbnail
  proxy also has a cap of 8 concurrent fetches (implemented, not load-tested). Obsolete searches are
  cancelled.
* **Note:** the metadata TTL is a **single global value**, not per provider. Default **30 minutes**;
  the settings UI offers 5, 15, 30, 60 and 240 minutes; the backend treats `0` as “caching disabled”.
  **Pixabay is the exception:** its API terms require a 24-hour cache, so the backend forces a
  1,440-minute TTL for Pixabay even when global caching is disabled or shorter.

### 4.9 Settings

* `[x]` Settings validation and patch handling are unit-tested, including clamping and unknown-key
  rejection.
* `[x]` Persistence across restart verified for **density = dense**, **reduced motion = on** and
  **12 results per source**.
* `[x]` Enabled-provider list is honoured: the release build searched 4 anonymous sources.
* `[x]` Provider health and policy panel; manual **Reset and retry** returned Openverse to `idle`
  with 0 failures.
* `[x]` Optional key fields for Pexels, Pixabay and Freepik (Magnific) render in the Sources panel;
  the Pixabay and Freepik cards show their policy records and preview-only/24-hour-cache notes
  (verified in `screenshot-new-settings-sources.png`).
* `[-]` Saving a key and running a keyed search was not tested, because no live keys were supplied.
* `[x]` Cache controls, statistics and freed-byte reporting verified live.
* `[x]` Diagnostics panel and **Copy diagnostics** verified (6,815 characters copied; API keys are
  not included).
* `[x]` Privacy panel states exactly which enabled providers receive the query.
* `[-]` Theme, default-folder picker, safe-search toggle, history toggle, overwrite toggle, sidecar
  toggle and “open folder after download” were not individually exercised beyond their backend
  validation/tests.

---

## 5. Backend modules

* `[x]` `error.rs` — typed `AppError` with stable codes (`validation`, `policy`, `rate_limited`,
  `timeout`, `not_found`, `io`, `internal`, `cancelled`, `malformed`) serialised for the frontend.
* `[x]` `models.rs` — `Asset`, `SearchRequest`, `AssetFilters`, `ProviderPage`, `ProviderInfo`,
  `ProviderStatusInfo`, `AppSettings`, `DownloadRecord`, `CacheStats`, `Diagnostics`; camelCase wire
  format unit-tested.
* `[x]` `providers/mod.rs` — `ImageProvider` trait, registry, capability/policy constructors; unique
  ids and “no scraping” policy records unit-tested.
* `[x]` `providers/http.rs` — one shared `reqwest` client, descriptive UA, connect timeout 8 s, total
  timeout 15 s, bounded retries with backoff/jitter, `Retry-After` handling and policy
  classification. Also supports a provider-specific auth header for the Magnific API.
* `[x]` `providers/rate_limit.rs` — per-provider minimum interval limiter; unit-tested.
* `[x]` `providers/health.rs` — circuit breaker (3 transient failures, 60 s initial cooldown doubling
  to 15 min), immediate block on policy/429, half-open probe, manual reset; unit-tested.
* `[x]` `providers/openverse.rs` — documented API, orientation/licence/media filters, colour from
  provider data, safe-search parameter; fixture parser tests.
* `[x]` `providers/wikimedia.rs` — Action API, `extmetadata` licence/artist extraction, 50-item
  pages, 1 req/s; fixture parser tests and HTML stripping test.
* `[x]` `providers/nasa.rs` — search API, asset-manifest resolution, canonical original download;
  fixture tests; non-image/video items skipped.
* `[x]` `providers/cleveland.rs` — open-access API, CC0-only download eligibility; fixture tests
  including a non-downloadable copyrighted record.
* `[x]` `providers/pexels.rs` — optional keyed adapter only; requires a key and is disabled by
  default. Parser and key gate are unit-tested; **live API access was not tested**. No scraping.
* `[x]` `providers/pixabay.rs` — optional keyed official API; native media/orientation/size/colour/
  safe-search parameters; 24-hour cache rule; fixture parser, key-gate and colour-mapping tests.
  **Live API access was not tested.**
* `[x]` `providers/freepik.rs` — optional keyed Magnific Resources API; search/preview only, never
  downloads; parser, key-gate and preview-only tests. **Live API access was not tested.**
* `[x]` `search/mod.rs` — sessions, cancellation tokens, 4-provider semaphore, progressive events,
  metadata cache (including Pixabay's forced 24-hour TTL), safe-search and local filtering,
  load-more.
* `[x]` `search/dedup.rs` — duplicate detection by provider id, canonical page URL and canonical
  image URL; tracking parameters removed.
* `[x]` `downloads/mod.rs` — manager, progress events, cancellation, size/space checks, atomic write,
  safe filenames, sidecars.
* `[x]` `database/mod.rs` — rusqlite (bundled SQLite), versioned schema, settings, library,
  collections, history, downloads, metadata cache; unit tests for all repositories.
* `[x]` `cache/mod.rs` — thumbnail proxy/cache, 12 MB image cap, image signature checks, LRU
  eviction, stats and clear.
* `[x]` `commands.rs` — a **single** command file covering search, providers, downloads, library,
  collections, history, settings, cache and diagnostics. There is no `commands/` directory.
* `[x]` `lib.rs` — app paths, daily rotating `tracing` log in the app-data directory, service wiring.
* `[x]` `build.rs` + `tests/resource_manifest.rs` — Windows Common-Controls v6 manifest linked into
  test binaries; integration test asserts its presence.

---

## 6. Database schema

```sql
assets_library(
  asset_id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  provider_asset_id TEXT NOT NULL,
  title TEXT,
  json TEXT NOT NULL,
  is_favorite INTEGER NOT NULL DEFAULT 0,
  saved_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

collections(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL COLLATE NOCASE UNIQUE,
  description TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

collection_items(
  collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
  asset_id TEXT NOT NULL REFERENCES assets_library(asset_id) ON DELETE CASCADE,
  added_at TEXT NOT NULL,
  PRIMARY KEY(collection_id, asset_id)
);

search_history(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  query TEXT NOT NULL,
  providers TEXT NOT NULL,
  result_count INTEGER NOT NULL DEFAULT 0,
  searched_at TEXT NOT NULL
);

settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);

downloads(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  asset_id TEXT NOT NULL,
  provider TEXT NOT NULL,
  title TEXT NOT NULL,
  file_path TEXT NOT NULL,
  source_url TEXT,
  license_name TEXT,
  license_url TEXT,
  attribution TEXT,
  bytes INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL,
  error TEXT,
  thumbnail_url TEXT,
  created_at TEXT NOT NULL
);

metadata_cache(
  cache_key TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  payload TEXT NOT NULL,
  fetched_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL
);
```

* Migrations are versioned in `PRAGMA user_version` (`1` at present) and applied on startup.
* Image bytes are never stored in SQLite, only paths and metadata.
* Search responses are cached as metadata only. Thumbnails are cached as display previews; original
  downloads are written only to the user's download folder.

---

## 7. Error handling & fallback

* `[x]` Provider isolation: each provider call is wrapped; failures become a `ProviderStatusInfo`
  and never stop other providers.
* `[x]` Timeouts: connect 8 s, request 15 s (download client: connect 10 s, total 600 s). Two retries
  with exponential backoff and jitter for transient errors only.
* `[x]` Circuit breaker: 3 consecutive transient failures open the circuit for 60 s, doubling to a
  15-minute maximum; `429`/policy responses open it immediately; one half-open probe; manual reset.
* `[x]` Malformed or empty payloads are errors, never converted into fake results. Fixture tests
  assert this for each parser.
* `[-]` All-providers-failed state shows per-provider reasons and a retry action; the code exists,
  but every recorded search had at least one working provider.
* `[x]` Cancellation is cooperative (`CancellationToken`); a cancelled search does not record a
  provider failure.

---

## 8. Security & privacy

* `[x]` CSP is configured in `tauri.conf.json`:
  `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: https: http://thumb.localhost thumb:; connect-src 'self' ipc: http://ipc.localhost; font-src 'self' data:; object-src 'none'; base-uri 'self'; form-action 'none'`.
  No remote scripts or fonts.
* `[x]` No accounts, analytics, telemetry or Stockcraft server.
* `[x]` Downloads use only http(s) and require an `image/*` content type; cached thumbnails
  additionally require a valid image signature and are capped at 12 MB. Files are written to the
  chosen folder or the default Pictures folder.
* `[x]` `open_path` is restricted to managed download folders or previously recorded download paths.
* `[x]` External URLs are validated (http/https only, no credentials, no localhost/private ranges)
  before opening; unit-tested.
* `[x]` Remote metadata is rendered as text. `dom.ts` escaping and nested templates are unit-tested.
* `[x]` The optional Pexels, Pixabay and Freepik/Magnific keys are stored locally in SQLite settings,
  are not included in diagnostics, and are never logged. Freepik results expose no download URL.
* `[x]` Thumbnail proxy requests are restricted to http(s) remote URLs and cached by hash.

---

## 9. Build & test commands

```powershell
# frontend
npm install
npm run typecheck        # tsc --noEmit
npm run test             # vitest run (19 tests)
npm run build            # tsc --noEmit && vite build -> dist/
npm run icons            # regenerate icons from assets/stockcraft-icon-source.png

# backend (from src-tauri)
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test               # 65 unit + 1 integration, offline-safe

# run / package
npm run tauri:dev        # development window
npm run tauri:build      # production NSIS bundle
```

Environment notes:

* The Rust default toolchain on this machine is `stable-x86_64-pc-windows-gnu`.
  `C:\mingw64\bin` must be on `PATH` for linking.
* Tauri's NSIS tool download timed out twice during packaging, so the NSIS tool cache at
  `C:\Users\Anumit_Pc\AppData\Local\tauri\NSIS` was populated manually. The build itself then
  succeeded.
* PowerShell may report exit code `1` for successful Cargo/npm commands that write progress to
  stderr; the final summary text is authoritative.
* There is **no live-provider test suite**. All provider tests use static fixtures. Live evidence
  comes from the packaged-app runs recorded below.
* The app icon source is `assets/stockcraft-icon-source.png` (1254 × 1254). `npm run icons` invokes
  Tauri's icon generator and copies the 256 px PNG to `public/icon.png`; no hand-drawn fallback is
  used.

---

## 10. Test & packaging results

### 10.1 Automated

| Check | Result |
| --- | --- |
| `npm run test` | **19 passed** (1 file) |
| `npm run typecheck` | Passed |
| `npm run build` | Passed |
| `cargo test` | **65 unit + 1 integration = 66 passed, 0 failed** |
| `cargo fmt --check` | Passed |
| `cargo clippy --all-targets -- -D warnings` | Passed |
| Doc-tests | 0 (none defined) |

Rust coverage includes: provider fixture parsing (including the new Pixabay and Freepik parsers),
key gates, Pixabay colour mapping, HTML stripping, asset id/wire format, settings validation (including
the new key fields), local filters, sorting helpers, dedup, circuit breaker, rate limiter, URL/path
safety, safe filenames, overwrite suffixes, extensions, download records, sidecars, cache eviction,
metadata TTL, and the Windows manifest.

### 10.2 Manual packaged-app runs

| Run | Result |
| --- | --- |
| Release search “sunrise” (current build) | **81 results from 4 sources**; all provider chips reported counts; visible thumbnails loaded |
| Sources panel (current build) | Pixabay and Freepik (Magnific) cards, policy records and key fields visible; both disabled |
| Icon (current build) | New gorilla artwork visible in title bar, header, welcome screen, installed app and NSIS setup-file icon |
| Current installer | Silent install succeeded; installed app launched; uninstall removed the install directory |
| Release “Load more” (earlier build) | 48 → 85 results, 4 providers |
| Installed app search “aurora” (earlier build) | 36 results, 0 broken thumbnails |
| Collections | Create, add image, open, rename, remove item, delete with confirmation |
| Pagination | 48 → 85 results before and after the AIC removal |
| Cancellation | Cancelled during and immediately after submission; UI recovered |
| Settings persistence | `dense`, reduced motion and 12 results/source survived restart |
| Cache clear | Metadata 15 → 0 rows; thumbnails 16,029,613 bytes / 80 files → 0 |
| Diagnostics copy | 6,815 characters copied with clipboard fallback |
| Download cancel | 35 MB NASA original; no completed file produced |
| Filename fix | `.jpg.jpg` prevented; regression test present |
| Shortcuts | `/`, `Ctrl+K`, `Ctrl+H`, `Escape`, `Ctrl+Shift+C`, `Ctrl+D`, `Ctrl+,`, `Ctrl+F` |
| Provider health reset | Openverse returned to `idle`, 0 failures |
| Narrow layout | 900 × 600 settings drawer fits, scrolls, no horizontal overflow |

### 10.3 Artifacts

| Artifact | Size | SHA-256 |
| --- | --- | --- |
| `src-tauri/target/release/stockcraft.exe` | 7,689,216 bytes | `42250EBD5A75A8BB42C93A00F62C705D680599CFC6DDF0D1E5657A8D71DC75F9` |
| `src-tauri/target/release/bundle/nsis/Stockcraft_0.1.0_x64-setup.exe` | 3,159,695 bytes | `6FE59EBF99854D65DA2BC8B45B527BB9BEBA1E5B2E6EED1038DFAC7C83BE6C26` |

### 10.4 Screenshots

* `screenshot-new-launch.png` — current release, new gorilla icon and welcome screen.
* `screenshot-new-results.png` — current release, “sunrise”: 81 results from 4 sources.
* `screenshot-new-settings-sources.png` — current release, Pixabay and Freepik (Magnific) cards with
  policy records and key fields.
* `screenshot-new-installed.png` — current installed build with the new icon.
* `screenshot-installer-icon.png` — extracted 32 px icon from the current NSIS setup file.
* `screenshot-installed-icon.png` — extracted 32 px icon from the installed executable.
* `screenshot-packaged.png` — earlier release search, 85 results.
* `screenshot-settings-900.png` — earlier settings drawer at 900 × 600.
* `screenshot-launch.png`, `screenshot-search*.png`, `screenshot-preview.png`,
  `screenshot-favourites.png`, `screenshot-downloads.png` — earlier manual passes.

---

## 11. Known limitations, bugs and blocked items

* `[!]` **Pexels public pages** — blocked by `robots.txt` (`Disallow: *q=*`, filter/paging/download
  paths). Not implemented by design.
* `[!]` **Freepik public pages** — blocked by `robots.txt` (`/search?`, `/*/search?`, download paths).
  Not implemented by design.
* `[!]` **Google Images** — `robots.txt` disallows `/search` and `/imgres`; the Custom Search JSON
  API is closed to new customers and ends **2027-01-01**. No scraping or Google provider exists.
* `[-]` **Pexels official API** — optional keyed adapter, disabled. No live key was tested.
* `[-]` **Pixabay official API** — optional keyed adapter, disabled. No live key was tested. Its
  metadata cache is forced to 24 hours by the API terms.
* `[-]` **Freepik / Magnific Resources API** — optional keyed adapter, disabled and preview-only.
  No live key was tested; the `filters[ids]` detail lookup is unverified.
* `[!]` **Art Institute of Chicago** — removed after 12/12 IIIF previews returned Cloudflare 403
  (`cf-mitigated: challenge`).
* `[ ]` **Unsplash / Flickr / Smithsonian / Rijksmuseum / Europeana** — require API keys and/or
  OAuth; not implemented.
* `[-]` **Native folder picker** — implemented via `rfd`, but the dialog was not exercised in the
  recorded runs.
* `[!]` **macOS / Linux** — no package was built or launched. The Tauri build emits a Linux warning
  that the binary may fail to start without a suitable WebKitGTK/WebView runtime.
* `[!]` **Live provider test suite** — none. Parsers use fixtures; outages or schema changes are
  detected only by real searches.
* `[x]` Openverse anonymous limits are tight (20/min burst, 200/day sustained) — mitigated by a
  per-provider limiter, metadata caching, a circuit breaker and clear status reporting.
* `[x]` Wikimedia `extmetadata` contains HTML — stripped and entity-decoded before display.
* `[x]` **Colour filtering is exact provider-data matching**, not thumbnail analysis. It compares the
  provider-published dominant colour (normalised to the UI palette); assets without colour data are
  hidden while a colour filter is active.
* `[x]` **Metadata cache TTL is global**, default 30 minutes. The UI offers 5–240 minutes; the
  backend treats 0 as disabled. **Pixabay is forced to 1,440 minutes** to satisfy its API terms.
* `[x]` **`confirmOverwrite` means “never overwrite”**: when on, a numbered suffix is added; when
  off, the existing file is replaced.
* `[x]` **Vectors** are disabled because none of the four anonymous default providers supports vector
  search. The optional keyed Pixabay/Freepik adapters can return vector assets, but the fixed category
  list does not yet expose a vector-only view.
* `[x]` NASA and Cleveland rate limits are undocumented; the app uses 1 req/s and 2 req/s
  respectively.
* `[ ]` No Openverse OAuth, no perceptual-hash duplicate detection, no collection export.
* `[x]` Old persisted settings may still list `"artic"`; the registry ignores unknown provider ids.
* `[x]` The workspace was **not** a git repository; it has now been initialised and published to
  <https://github.com/gggff123/Stockcraft> (public, default branch `main`). See section 13.
* `[x]` Persistent app data lives under
  `C:\Users\Anumit_Pc\AppData\Roaming\app.stockcraft.desktop`. The installed application directory
  was removed by the uninstaller.

---

## 12. Future improvements

* `[ ]` Metropolitan Museum of Art adapter (anonymous API verified; object fetch not implemented).
* `[ ]` Optional Openverse OAuth for higher limits.
* `[ ]` Enable a vector-only category view when a keyed Pixabay/Freepik source is active.
* `[ ]` Per-collection export to JSON/CSV.
* `[ ]` Perceptual-hash duplicate detection across providers.
* `[ ]` Live-provider integration suite gated behind an environment variable.
* `[ ]` macOS/Linux packaging.
* `[!]` **CI** — deliberately not configured at the owner's instruction. The Windows build is produced
  on a developer machine with the MinGW/GNU toolchain; the manual release procedure is documented in
  the README. No GitHub Actions workflow exists, so no CI badge is published.
* `[ ]` Full accessibility audit (screen reader, focus order, contrast).

---

## 13. Publishing record (2026-10-21)

Everything in this section was executed and verified during the publishing pass.

### 13.1 Repository

| Item | Value |
| --- | --- |
| Repository | <https://github.com/gggff123/Stockcraft> |
| Owner | `gggff123` |
| Visibility | **Public** |
| Default branch | `main` |
| Description | `A free stock image discovery desktop app built with Tauri 2 and Rust, featuring multi-source image search and automatic provider fallback.` |
| Pre-existing repository | **None** — `gggff123/Stockcraft` did not exist and was created; no remote was replaced and no existing content was overwritten. |
| Git history | The workspace was **not** a git repository. Git was initialised (`git init -b main`) and the source committed as the initial commit. Nothing was force-pushed and no history was rewritten. |

### 13.2 Documentation

* `[x]` Root `README.md` created and substantially rewritten: introduction, screenshot section,
  feature list, download & installation, provider & licensing guide, privacy, developer setup,
  project structure, contributing, security, known limitations, release process and acknowledgements.
* `[x]` `LICENSE` added — **MIT**, chosen by the owner.
* `[x]` `docs/screenshots/` created. Six screenshots of the **current** build are published; the ten
  earlier-build captures were moved to `docs/screenshots/legacy/` and are explicitly labelled as
  outdated, because they still show the removed "Art Institute of Chicago" source.
* `[x]` `01-welcome.png` was re-captured from the freshly built release executable in this pass.
* `[x]` `.gitignore` reviewed and extended: `node_modules/`, `dist/`, `src-tauri/target/`,
  `src-tauri/gen/schemas/` (Tauri regenerates these), `*.log`, `.env*`, key/credential files and
  editor/OS noise. The committed `src-tauri/gen/schemas/` was removed from the index to match the
  official Tauri v2 template.
* `[x]` Secret scan over all 132 staged files found **no** token, key or private-key patterns.

### 13.3 Release configuration

| Item | Value |
| --- | --- |
| Bundle target | NSIS (`targets: ["nsis"]`) — unchanged from the existing project configuration |
| Product name / version | `Stockcraft` / `0.1.0` — unchanged |
| Identifier | `app.stockcraft.desktop` — unchanged |
| Install mode | `currentUser` → `%LOCALAPPDATA%\Stockcraft`, no administrator rights required |
| Installer / uninstaller icon | `icons/icon.ico` — the Stockcraft brand mark |
| Resources | `assets/stockcraft-mascot.svg` only; no dev files, secrets or caches are packaged |
| Version consistency | `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` and `feat.md` all read `0.1.0` |
| Portable executable | **Not published.** See 13.6. |

### 13.4 Build and test results (this pass)

| Check | Result |
| --- | --- |
| `npm run typecheck` | **Passed** |
| `npm test` | **19 passed** (1 file) |
| `npm run build` | **Passed** (25 modules) |
| `cargo fmt --check` | **Passed** |
| `cargo clippy --all-targets -- -D warnings` | **Passed** |
| `cargo test` | **65 unit + 1 integration = 66 passed, 0 failed** |
| `npm run tauri:build` | **Succeeded** in 3 m 20 s → `Stockcraft_0.1.0_x64-setup.exe` (3.01 MiB) |

The only build diagnostic is the **cosmetic** MinGW linker warning
`.rsrc merge failure: multiple non-default manifests`, caused by `src-tauri/build.rs` deliberately
linking the Windows resource archive into every target. The build succeeds and the executable
contains exactly one usable manifest.

### 13.5 Artefacts, verification and install test

| Artefact | Size | SHA-256 |
| --- | --- | --- |
| `src-tauri/target/release/bundle/nsis/Stockcraft_0.1.0_x64-setup.exe` | 3,159,592 bytes | `3BE4D7B4B294A8C0F6EA536B773258D435B3C7F2A40456F7C02F737D4334E4B3` |
| `src-tauri/target/release/stockcraft.exe` | 7,689,216 bytes | `BF10DF92ACEF6F5D5EC2D802130B06A61C92DB2E3A3FC64E87E05B05C64688A6` |

* `[x]` Both artefacts exist, are non-empty, report `ProductName = Stockcraft`,
  `ProductVersion = 0.1.0` and are x64.
* `[x]` **Authenticode: `NotSigned`.** The release is unsigned, so SmartScreen will warn on first
  launch. This is stated honestly in the README and the release notes.
* `[x]` **Live run of the freshly built release executable:** it started, opened its window
  ("Stockcraft — free image foundry"), and a live search for "sunrise" returned **81 results from 4
  sources** with all four provider chips reporting counts and thumbnails rendering.
* `[x]` **Install test:** the NSIS installer ran with `/S` and exited `0`. It deployed
  `stockcraft.exe`, `WebView2Loader.dll` and `uninstall.exe` into `%LOCALAPPDATA%\Stockcraft`.
* `[x]` The **installed** application was launched and ran correctly.
* `[x]` The uninstaller ran with `/S`, exited `0`, and removed the installation directory.

### 13.6 Portable executable — not published (verified, not an omission)

`stockcraft.exe` imports **`WebView2Loader.dll`**, which the NSIS bundle deploys next to it. Copying
the exe alone to an empty directory and launching it failed with exit code
**`-1073741515` (`0xC0000135` = `STATUS_DLL_NOT_FOUND`)**; the same exe with `WebView2Loader.dll`
beside it started normally. The bare executable is therefore **not** a portable build and was
**not** published as one. Only the NSIS installer is offered, which is the correct distribution for a
Tauri app on Windows.

### 13.7 GitHub Release

| Item | Value |
| --- | --- |
| Tag | **`v0.1.0`** |
| Release URL | <https://github.com/gggff123/Stockcraft/releases/latest> |
| Asset 1 | `Stockcraft_0.1.0_x64-setup.exe` — the NSIS installer |
| Asset 2 | `Stockcraft_0.1.0_x64-setup.exe.sha256` — SHA-256 checksum |
| Checksums | **Published** |
| Portable executable | **Not attached** (see 13.6) |

### 13.8 Known limitations of this publishing pass

* `[!]` **No CI.** At the owner's instruction no GitHub Actions workflow was added, so the Windows
  build is not reproducible on a hosted runner. The manual procedure is documented in the README's
  "Releasing" section. The GitHub runner images do provide MinGW-w64 at `C:\mingw64` with it on the
  machine `PATH`, which matches the toolchain this project is verified against, so a future workflow
  can use the same GNU target.
* `[x]` Everything else in the publishing checklist is complete: repository created and public, README
  written, licence added, screenshots organised, release configuration verified, production build
  run, tests and lints run, artefacts verified, install/uninstall tested, release published with
  checksums, and the source pushed to `main`.

---

## Development log

| Date | Milestone | Notes |
| --- | --- | --- |
| 2026-10-21 | Inspect & plan | Toolchain verified. Pexels `robots.txt` inspected and public-page adapter excluded. Openverse/Commons/NASA/Artic/Cleveland APIs probed. `feat.md` created. |
| 2026-10-21 | Core build | Frontend shell, Rust providers, search orchestration, downloads, SQLite library, cache and settings implemented. |
| 2026-10-21 | Verification pass | Collections, pagination, cancellation, settings persistence, cache controls, diagnostics, download cancellation and shortcuts exercised manually. |
| 2026-10-21 | AIC removal | 12/12 IIIF previews failed with Cloudflare 403. Adapter, registry entry and UI copy removed; provider list reduced to 4 anonymous sources plus optional Pexels. |
| 2026-10-21 | Packaging | Production NSIS build succeeded after manually populating Tauri's NSIS cache. Release app searched “sunrise” (48 → 85 results). Installer installed, app searched “aurora” (36 results), then uninstalled cleanly. |
| 2026-10-21 | Earlier final record | `cargo fmt`, `cargo clippy`, 57 Rust tests, 19 frontend tests, typecheck and production build confirmed. This file was rewritten to match the shipped build and its unverified areas. |
| 2026-10-21 | Icon + provider expansion | Supplied cross-hatched gorilla artwork replaced the procedural icon across Windows/macOS/iOS/Android output. Added optional keyed Pixabay and Freepik/Magnific adapters; Freepik is preview-only. Google Images and Freepik public pages documented as excluded after robots/API checks. |
| 2026-10-21 | Re-verification | 65 unit + 1 integration Rust tests and 19 frontend tests passed; release search “sunrise” returned 81 results from 4 sources; current NSIS installer (with gorilla setup/uninstaller icon) installed, launched with the new icon, and uninstalled cleanly. Live keyed providers remain untested. |
| 2026-10-21 | Publishing pass | Git initialised; public repository `gggff123/Stockcraft` created; `README.md` written; MIT `LICENSE` added; screenshots reorganised into `docs/screenshots/` (+ `legacy/`); `.gitignore` extended and `src-tauri/gen/schemas/` untracked; production build re-run (3 m 20 s); all tests/lints re-run green; live search re-verified (81 results, 4 sources); installer install → launch → uninstall cycle re-verified; bare exe confirmed **not** portable (`STATUS_DLL_NOT_FOUND` without `WebView2Loader.dll`) so no portable artefact was published; release `v0.1.0` created with the NSIS installer and a SHA-256 checksum file; source pushed to `main`. No CI workflow, at the owner's instruction. |
