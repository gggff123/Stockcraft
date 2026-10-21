import { describe, expect, it } from "vitest";

import { esc, html, raw } from "../src/lib/dom";
import { safeFilename, filenameFromUrl, formatBytes, pluralize } from "../src/lib/format";
import { state } from "../src/lib/store";
import type { Asset, AssetFilters } from "../src/lib/types";
import {
  activeFilterCount,
  licenseMatches,
  matchesFilters,
  orientationOf,
  sortAssets,
} from "../src/features/search/options";
import { thumbUrl, previewUrl } from "../src/features/search/tile";

function asset(overrides: Partial<Asset> = {}): Asset {
  return {
    id: "openverse:1",
    provider: "openverse",
    providerAssetId: "1",
    title: "Mountains",
    tags: [],
    mediaType: "photo",
    downloadAllowed: true,
    providerMetadata: {},
    ...overrides,
  };
}

const filters = (overrides: Partial<AssetFilters> = {}): AssetFilters => ({
  orientation: "any",
  minWidth: 0,
  minHeight: 0,
  mediaType: "any",
  license: "any",
  color: "any",
  category: "all",
  ...overrides,
});

describe("dom escaping", () => {
  it("escapes every HTML-significant character", () => {
    expect(esc(`<img src="x" onerror='alert(1)'> & more`)).toBe(
      "&lt;img src=&quot;x&quot; onerror=&#39;alert(1)&#39;&gt; &amp; more",
    );
  });

  it("escapes interpolations but keeps Raw and Raw arrays verbatim", () => {
    const title = `<script>alert("x")</script>`;
    const markup = html`<h1>${title}</h1><p>${raw("<b>safe</b>")}</p><ul>${[raw("<li>a</li>"), "<li>b</li>"]}</ul>`.value;
    expect(markup).not.toContain("<script>");
    expect(markup).toContain("&lt;script&gt;");
    expect(markup).toContain("<b>safe</b>");
    expect(markup).toContain("<li>a</li>");
    expect(markup).toContain("&lt;li&gt;b&lt;/li&gt;");
  });

  it("composes nested templates without double-escaping them", () => {
    const inner = html`<b>${"<safe>"}</b>`;
    const outer = html`<p>${inner}</p><p>${[inner]}</p>`;
    expect(outer.value).toBe("<p><b>&lt;safe&gt;</b></p><p><b>&lt;safe&gt;</b></p>");
  });

  it("renders null and undefined as empty strings", () => {
    expect(esc(null)).toBe("");
    expect(esc(undefined)).toBe("");
  });
});

describe("safe filenames", () => {
  it("replaces path separators and trailing dots", () => {
    expect(safeFilename("../../etc/passwd")).toBe("etc-passwd");
    expect(safeFilename("  A/B: C*? D  ")).toBe("A-B-C-D");
  });

  it("keeps a usable fallback for empty or symbol-only titles", () => {
    expect(safeFilename("")).toBe("stockcraft-image");
    expect(safeFilename("...", "fallback")).toBe("fallback");
  });

  it("extracts a filename from a URL and tolerates garbage", () => {
    expect(filenameFromUrl("https://example.org/a%20b.png?x=1")).toBe("a b.png");
    expect(filenameFromUrl("not a url")).toBe("");
    expect(filenameFromUrl(null)).toBe("");
  });
});

describe("formatting", () => {
  it("formats byte sizes and plurals", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(pluralize(1, "image")).toBe("1 image");
    expect(pluralize(2, "image")).toBe("2 images");
  });
});

describe("local filters", () => {
  it("classifies orientations from real dimensions", () => {
    expect(orientationOf(asset({ width: 1200, height: 800 }))).toBe("landscape");
    expect(orientationOf(asset({ width: 800, height: 1200 }))).toBe("portrait");
    expect(orientationOf(asset({ width: 1000, height: 1000 }))).toBe("square");
    expect(orientationOf(asset())).toBe("unknown");
  });

  it("matches licence names conservatively", () => {
    expect(licenseMatches("cc0", asset({ licenseName: "CC0 1.0" }))).toBe(true);
    expect(licenseMatches("cc0", asset({ licenseName: "Public domain" }))).toBe(true);
    expect(licenseMatches("by", asset({ licenseName: "CC BY 4.0" }))).toBe(true);
    expect(licenseMatches("by", asset({ licenseName: "CC BY-SA 4.0" }))).toBe(false);
    expect(licenseMatches("by-sa", asset({ licenseName: "CC BY-SA 4.0" }))).toBe(true);
    expect(licenseMatches("other", asset({ licenseName: "All rights reserved" }))).toBe(true);
    expect(licenseMatches("other", asset({}))).toBe(true);
  });

  it("rejects assets missing the requested colour data", () => {
    const teal = asset({ dominantColor: "teal" });
    expect(matchesFilters(teal, filters({ color: "teal" }))).toBe(true);
    expect(matchesFilters(teal, filters({ color: "blue" }))).toBe(false);
    expect(matchesFilters(asset(), filters({ color: "teal" }))).toBe(false);
  });

  it("applies size, media and orientation constraints together", () => {
    const candidate = asset({
      width: 1920,
      height: 1080,
      mediaType: "photo",
      licenseName: "CC BY 4.0",
    });
    expect(matchesFilters(candidate, filters({ minWidth: 1000, minHeight: 1000 }))).toBe(true);
    expect(matchesFilters(candidate, filters({ minWidth: 2000 }))).toBe(false);
    expect(matchesFilters(candidate, filters({ orientation: "portrait" }))).toBe(false);
    expect(matchesFilters(candidate, filters({ mediaType: "photo" }))).toBe(true);
    expect(matchesFilters(candidate, filters({ mediaType: "illustration" }))).toBe(false);
    expect(matchesFilters(candidate, filters({ mediaType: "diagram" }))).toBe(false);
    expect(matchesFilters(asset({ mediaType: "map" }), filters({ mediaType: "diagram" }))).toBe(true);
  });

  it("counts only active filters", () => {
    expect(activeFilterCount(filters())).toBe(0);
    expect(
      activeFilterCount(filters({ orientation: "portrait", minWidth: 800, license: "cc0", category: "nature" })),
    ).toBe(4);
  });
});

describe("sorting", () => {
  it("leaves relevance order untouched and returns a new array for other sorts", () => {
    const input = [asset({ id: "a" }), asset({ id: "b" })];
    expect(sortAssets(input, "relevance")).toBe(input);
    expect(sortAssets(input, "largest")).not.toBe(input);
    expect(input.map((item) => item.id)).toEqual(["a", "b"]);
  });

  it("sorts newest first and pushes undated assets to the end", () => {
    const sorted = sortAssets(
      [
        asset({ id: "old", createdAt: "2020-01-01T00:00:00Z" }),
        asset({ id: "undated" }),
        asset({ id: "new", createdAt: "2024-01-01T00:00:00Z" }),
      ],
      "newest",
    );
    expect(sorted.map((item) => item.id)).toEqual(["new", "old", "undated"]);
  });

  it("sorts by pixel area, treating missing dimensions as zero", () => {
    const sorted = sortAssets(
      [
        asset({ id: "small", width: 400, height: 300 }),
        asset({ id: "missing" }),
        asset({ id: "large", width: 4000, height: 3000 }),
      ],
      "largest",
    );
    expect(sorted.map((item) => item.id)).toEqual(["large", "small", "missing"]);
  });
});

describe("thumbnail proxy URLs", () => {
  it("uses provider URLs when no cache base is configured", () => {
    state.thumbnailBase = "";
    const item = asset({ thumbnailUrl: "https://cdn.example.org/a.jpg" });
    expect(thumbUrl(item)).toBe("https://cdn.example.org/a.jpg");
    expect(previewUrl(item)).toBe("https://cdn.example.org/a.jpg");
  });

  it("routes and encodes through the Rust proxy when caching is enabled", () => {
    state.thumbnailBase = "http://thumb.localhost";
    const item = asset({
      thumbnailUrl: "https://cdn.example.org/a b.jpg?x=1&y=2",
      previewUrl: "https://cdn.example.org/full.jpg",
    });
    expect(thumbUrl(item)).toBe(
      "http://thumb.localhost/fetch?u=https%3A%2F%2Fcdn.example.org%2Fa%20b.jpg%3Fx%3D1%26y%3D2",
    );
    expect(previewUrl(item)).toBe(
      "http://thumb.localhost/fetch?u=https%3A%2F%2Fcdn.example.org%2Ffull.jpg",
    );
  });

  it("falls back from thumbnail to preview and returns empty without either", () => {
    state.thumbnailBase = "";
    expect(thumbUrl(asset({ previewUrl: "https://cdn.example.org/p.jpg" }))).toBe(
      "https://cdn.example.org/p.jpg",
    );
    expect(thumbUrl(asset())).toBe("");
  });
});
