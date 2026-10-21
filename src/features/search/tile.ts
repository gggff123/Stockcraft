import { html, raw, type Raw } from "../../lib/dom";
import { formatDimensions, licenseShort, mediaTypeLabel, orientationOf } from "../../lib/format";
import { state } from "../../lib/store";
import type { Asset } from "../../lib/types";
import { iconButton } from "../../components/ui";

/**
 * Resolves the display URL for an asset thumbnail.
 *
 * In the desktop app thumbnails are fetched through the Rust `thumb://` scheme, which caches bytes
 * on disk, caps concurrency and keeps provider CDN traffic inside one audited place. The data URL
 * fallback (browser preview) is never used in the packaged app.
 */
export function thumbUrl(asset: Asset): string {
  const source = asset.thumbnailUrl ?? asset.previewUrl ?? "";
  if (!source) return "";
  if (!state.thumbnailBase) return source;
  return `${state.thumbnailBase}/fetch?u=${encodeURIComponent(source)}`;
}

export function previewUrl(asset: Asset): string {
  const source = asset.previewUrl ?? asset.thumbnailUrl ?? "";
  if (!source) return "";
  if (!state.thumbnailBase) return source;
  return `${state.thumbnailBase}/fetch?u=${encodeURIComponent(source)}`;
}

function aspectStyle(asset: Asset): string {
  if (asset.width && asset.height) return `aspect-ratio:${asset.width} / ${asset.height};`;
  return "aspect-ratio:4 / 3;";
}

export function tile(asset: Asset, options: { removable?: boolean } = {}): Raw {
  const favorite = state.favoriteIds.has(asset.id);
  const title = asset.title?.trim() || "Untitled image";
  const url = thumbUrl(asset);
  const orientation = orientationOf(asset);
  const media = mediaTypeLabel(asset.mediaType);
  const license = asset.licenseName ? licenseShort(asset) : "";

  return raw(html`
    <article class="tile" data-tile="${asset.id}" data-provider="${asset.provider}">
      <div class="tile-media" data-state="loading" style="${raw(aspectStyle(asset))}">
        <div class="skeleton" aria-hidden="true"></div>
        ${url
          ? raw(
              `<img src="${url.replace(/"/g, "&quot;")}" alt="${title.replace(/"/g, "&quot;")}" loading="lazy" decoding="async" draggable="false" />`,
            )
          : ""}
        <button
          type="button"
          class="tile-preview"
          data-action="open-preview"
          data-id="${asset.id}"
          data-provider="${asset.provider}"
          aria-label="Preview ${title} (${orientation}, ${media})"
          title="Preview details"
        ></button>
        <div class="tile-badges">
          <span class="badge">${asset.sourceName ?? asset.provider}</span>
          ${license ? html`<span class="badge" data-kind="license">${license}</span>` : ""}
          ${media !== "Photo" ? html`<span class="badge" data-kind="type">${media}</span>` : ""}
        </div>
        <div class="tile-actions">
          ${iconButton({
            action: "toggle-favorite",
            iconName: favorite ? "heart-filled" : "heart",
            label: favorite ? `Remove ${title} from favourites` : `Add ${title} to favourites`,
            title: favorite ? "Remove from favourites" : "Add to favourites",
            pressed: favorite,
            size: 15,
            data: { id: asset.id, provider: asset.provider },
          })}
          ${asset.downloadAllowed && asset.downloadUrl
            ? iconButton({
                action: "quick-download",
                iconName: "download",
                label: `Download ${title}`,
                title: "Download image",
                size: 15,
                data: { id: asset.id, provider: asset.provider },
              })
            : ""}
          ${asset.pageUrl
            ? iconButton({
                action: "open-source",
                iconName: "external",
                label: `Open ${title} on ${asset.sourceName ?? asset.provider}`,
                title: "Open original page",
                size: 15,
                data: { url: asset.pageUrl },
              })
            : ""}
          ${options.removable
            ? iconButton({
                action: "remove-from-active-collection",
                iconName: "trash",
                label: `Remove ${title} from this collection`,
                title: "Remove from collection",
                size: 15,
                data: { id: asset.id },
              })
            : ""}
        </div>
      </div>
      <div class="tile-meta">
        <div class="tile-title" title="${title}">${title}</div>
        <div class="tile-sub">
          <span class="clamp" title="${asset.creator ?? ""}">${asset.creator ?? asset.sourceName ?? asset.provider}</span>
          <span class="sep">·</span>
          <span>${formatDimensions(asset)}</span>
        </div>
      </div>
    </article>
  `);
}

export function skeletonTile(aspect = "4 / 3"): Raw {
  return raw(html`
    <div class="tile" aria-hidden="true">
      <div class="tile-media" style="aspect-ratio:${raw(aspect)}">
        <div class="skeleton"></div>
      </div>
      <div class="tile-meta">
        <div class="tile-title">&nbsp;</div>
        <div class="tile-sub"><span>&nbsp;</span></div>
      </div>
    </div>
  `);
}
