/** Asset detail modal: preview, metadata, licence, attribution and actions. */
import { api } from "../../lib/api";
import { copyText, html, qs, raw, render, registerActions } from "../../lib/dom";
import { formatDimensions, formatMegapixels, hostLabel, mediaTypeLabel, shorten } from "../../lib/format";
import { emit, state } from "../../lib/store";
import type { Asset } from "../../lib/types";
import { button, iconButton, notice, selectField, toast } from "../../components/ui";
import { previewUrl, thumbUrl } from "../search/tile";
import { downloadAsset, openSource } from "../search/controller";

export async function openPreview(id: string, provider: string): Promise<void> {
  const existing = state.assets.find((asset) => asset.id === id);
  state.previewId = id;
  state.preview = existing ?? null;
  state.previewLoading = !existing;
  state.panel = "none";
  emit("preview", "header");

  if (existing) return;
  try {
    const asset = await api.getAssetDetails(provider, id);
    if (state.previewId !== id) return;
    state.preview = asset;
    state.previewLoading = false;
    emit("preview");
  } catch (error) {
    if (state.previewId !== id) return;
    state.previewId = null;
    state.preview = null;
    state.previewLoading = false;
    emit("preview");
    toast({
      title: "Could not load image details",
      message: error instanceof Error ? error.message : String(error),
      tone: "error",
    });
  }
}

export function closePreview(): void {
  state.previewId = null;
  state.preview = null;
  state.previewLoading = false;
  emit("preview");
}

function relatedFromCreator(asset: Asset): Asset[] {
  if (!asset.creator) return [];
  return state.assets
    .filter((candidate) => candidate.creator === asset.creator && candidate.id !== asset.id && candidate.thumbnailUrl)
    .slice(0, 6);
}

export function renderPreview(): void {
  const host = qs("#preview-host");
  if (!host) return;
  if (!state.previewId) {
    render(host, "");
    return;
  }

  const asset = state.preview;
  if (!asset) {
    render(
      host,
      html`<div class="modal-scrim" data-action="close-preview">
        <div class="modal" role="dialog" aria-modal="true" aria-label="Loading image details" data-stop-click>
          <div class="modal-stage"><span class="spinner" aria-hidden="true"></span></div>
          <div class="modal-side"><div class="modal-side-body"><p class="muted">Loading details…</p></div></div>
        </div>
      </div>`,
    );
    return;
  }

  const favorite = state.favoriteIds.has(asset.id);
  const related = relatedFromCreator(asset);
  const image = previewUrl(asset);
  const collections = state.collections;

  render(
    host,
    html`<div class="modal-scrim" data-action="close-preview">
      <div class="modal" role="dialog" aria-modal="true" aria-label="${asset.title ?? "Image details"}" data-stop-click>
        <div class="modal-stage">
          ${image
            ? raw(
                `<img id="preview-image" src="${image.replace(/"/g, "&quot;")}" alt="${(asset.title ?? "Image preview").replace(/"/g, "&quot;")}" />`,
              )
            : html`<div class="state-block"><p>No preview image is available for this asset.</p></div>`}
          <span class="spinner" id="preview-spinner" aria-hidden="true"></span>
          <div style="position:absolute;bottom:10px;left:12px;right:12px;display:flex;gap:8px;justify-content:space-between">
            <span class="badge">${asset.sourceName ?? asset.provider}</span>
            <span class="badge">preview — not the original file</span>
          </div>
        </div>
        <div class="modal-side">
          <div class="modal-side-head">
            <div class="grow">
              <h2 class="modal-title">${asset.title || "Untitled image"}</h2>
              <div class="tiny muted">
                ${asset.creator ? html`<span>${asset.creator}</span>` : html`<span>creator not published</span>`}
                ${hostLabel(asset.pageUrl) ? html`<span> · ${hostLabel(asset.pageUrl)}</span>` : ""}
              </div>
            </div>
            ${iconButton({
              action: "toggle-favorite",
              iconName: favorite ? "heart-filled" : "heart",
              label: favorite ? "Remove from favourites" : "Add to favourites",
              pressed: favorite,
              data: { id: asset.id, provider: asset.provider },
            })}
            ${iconButton({ action: "close-preview", iconName: "close", label: "Close preview" })}
          </div>
          <div class="modal-side-body">
            <div class="modal-actions">
              ${asset.downloadAllowed && asset.downloadUrl
                ? button({ action: "preview-download", label: "Download image", variant: "primary", iconName: "download", data: { id: asset.id } })
                : button({
                    action: "preview-open-page",
                    label: "Open original page",
                    variant: "primary",
                    iconName: "external",
                    data: { url: asset.pageUrl ?? "" },
                  })}
              ${asset.pageUrl
                ? button({ action: "preview-open-page", label: "Original page", variant: "ghost", iconName: "external", data: { url: asset.pageUrl } })
                : ""}
              ${asset.creatorUrl
                ? button({ action: "preview-open-page", label: "Creator", variant: "ghost", iconName: "globe", data: { url: asset.creatorUrl } })
                : ""}
              ${asset.licenseUrl
                ? button({ action: "preview-open-page", label: "Licence", variant: "ghost", iconName: "shield", data: { url: asset.licenseUrl } })
                : ""}
            </div>

            <div class="section">
              <h3>Save</h3>
              ${collections.length
                ? html`<div class="row">
                    <div class="grow">
                      ${selectField(
                        "Add to collection",
                        String(collections[0]?.id ?? ""),
                        collections.map((c) => ({ value: String(c.id), label: `${c.name} (${c.itemCount})` })),
                        "preview-collection-select",
                      )}
                    </div>
                    ${button({ action: "preview-add-to-collection", label: "Add", variant: "ghost", iconName: "plus", data: { id: asset.id } })}
                  </div>`
                : html`<div class="tiny muted">No collections yet.</div>`}
              ${button({
                action: "preview-new-collection",
                label: collections.length ? "New collection…" : "Create a collection…",
                variant: "ghost",
                size: "sm",
                iconName: "plus",
                data: { id: asset.id },
              })}
              ${button({ action: "preview-copy-attribution", label: "Copy attribution text", variant: "ghost", size: "sm", iconName: "check" })}
            </div>

            <div class="section">
              <h3>Metadata</h3>
              <dl class="meta-grid">
                <dt>Source</dt>
                <dd>${asset.sourceName ?? asset.provider}</dd>
                <dt>Asset id</dt>
                <dd class="mono">${asset.id}</dd>
                <dt>Dimensions</dt>
                <dd>${formatDimensions(asset)} ${asset.width && asset.height ? html`<span class="muted">· ${formatMegapixels(asset)}</span>` : ""}</dd>
                <dt>Format</dt>
                <dd>${asset.mimeType ?? "not published"}</dd>
                <dt>Type</dt>
                <dd>${mediaTypeLabel(asset.mediaType)}</dd>
                <dt>Licence</dt>
                <dd>
                  ${asset.licenseName ?? "not published"}
                  ${asset.licenseUrl ? html` · <a href="${asset.licenseUrl}" data-action="preview-open-page" data-url="${asset.licenseUrl}">licence text</a>` : ""}
                </dd>
                <dt>Original</dt>
                <dd>${asset.pageUrl ? html`<a href="${asset.pageUrl}" data-action="preview-open-page" data-url="${asset.pageUrl}">${hostLabel(asset.pageUrl)}</a>` : "not published"}</dd>
                <dt>Download</dt>
                <dd>${asset.downloadAllowed && asset.downloadUrl ? html`direct file available` : html`not offered by this source`}</dd>
              </dl>
            </div>

            ${asset.attribution
              ? html`<div class="section">
                  <h3>Attribution</h3>
                  <div class="attribution">${shorten(asset.attribution, 600)}</div>
                  <div class="tiny muted">Required by the licence terms. Keep it with any reuse of this image.</div>
                </div>`
              : html`<div class="section">
                  <h3>Attribution</h3>
                  <div class="tiny muted">This source does not publish an attribution string. Check the original page before reuse.</div>
                </div>`}

            ${asset.description
              ? html`<div class="section"><h3>Description</h3><div class="tiny">${shorten(asset.description, 700)}</div></div>`
              : ""}

            ${asset.tags.length
              ? html`<div class="section">
                  <h3>Tags</h3>
                  <div class="chip-row">
                    ${asset.tags.slice(0, 14).map(
                      (tag) => raw(html`<button type="button" class="chip" data-action="quick-search" data-query="${tag}">${tag}</button>`),
                    )}
                  </div>
                </div>`
              : ""}

            ${raw(
              notice(
                `Downloading does not grant unrestricted reuse. ${
                  asset.licenseName ? `This image is offered under “${asset.licenseName}”` : "This source does not state a licence"
                } — always check the original page.`,
                "warn",
              ).value,
            )}

            ${related.length
              ? html`<div class="section">
                  <h3>More from this creator (loaded results)</h3>
                  <div style="display:grid;grid-template-columns:repeat(3,1fr);gap:6px">
                    ${related.map(
                      (item) => raw(
                        html`<button
                          type="button"
                          data-action="open-preview"
                          data-id="${item.id}"
                          data-provider="${item.provider}"
                          style="border:1px solid var(--border);border-radius:6px;overflow:hidden;padding:0;background:none;cursor:pointer"
                          aria-label="Preview ${item.title ?? "image"}"
                        >
                          <img src="${thumbUrl(item)}" alt="" style="width:100%;height:64px;object-fit:cover;display:block" loading="lazy" />
                        </button>`,
                      ),
                    )}
                  </div>
                </div>`
              : ""}

            ${asset.providerMetadata && Object.keys(asset.providerMetadata).length
              ? html`<div class="section">
                  <h3>Source metadata</h3>
                  <dl class="meta-grid">
                    ${Object.entries(asset.providerMetadata).map(
                      ([key, value]) => raw(html`<dt>${key}</dt><dd>${shorten(value, 220)}</dd>`),
                    )}
                  </dl>
                </div>`
              : ""}
          </div>
        </div>
      </div>
    </div>`,
  );

  const image_ = qs<HTMLImageElement>("#preview-image");
  const spinner = qs("#preview-spinner");
  if (image_) {
    const clear = () => spinner?.remove();
    if (image_.complete) clear();
    image_.addEventListener("load", clear);
    image_.addEventListener("error", () => {
      clear();
      image_.style.opacity = "0.25";
      toast({ title: "Preview image unavailable", message: "The source did not return a preview. Try the original page.", tone: "warn" });
    });
    if (asset.width && asset.height && asset.width > asset.height) {
      image_.style.maxHeight = "min(78vh, 860px)";
    }
  }

  qs<HTMLButtonElement>("[data-action='close-preview']")?.focus();
}

export function registerPreviewActions(): void {
  registerActions({
    "preview-download": {
      click: (element) => {
        const asset = state.preview;
        if (asset && asset.id === element.dataset.id) void downloadAsset(asset);
      },
    },
    "preview-open-page": { click: (element) => void openSource(element.dataset.url) },
    "preview-copy-attribution": {
      click: async () => {
        const asset = state.preview;
        if (!asset) return;
        const text =
          asset.attribution ??
          `${asset.title ?? "Image"} — ${asset.creator ?? "unknown creator"}, ${asset.sourceName ?? asset.provider}, ${asset.licenseName ?? "licence unknown"}, ${asset.pageUrl ?? ""}`;
        if (await copyText(text)) {
          toast({ title: "Attribution copied", message: shorten(text, 120), tone: "success", timeoutMs: 3500 });
        } else {
          toast({ title: "Could not use the clipboard", message: "You can select the attribution text manually.", tone: "warn" });
        }
      },
    },
    "preview-add-to-collection": {
      click: async () => {
        const asset = state.preview;
        const select = qs<HTMLSelectElement>("[data-action='preview-collection-select']");
        if (!asset || !select) return;
        const collectionId = Number(select.value);
        if (!collectionId) return;
        try {
          const collection = await api.addToCollection(collectionId, asset);
          state.collections = state.collections.map((c) => (c.id === collection.id ? collection : c));
          emit("panel", "header");
          toast({ title: "Added to collection", message: collection.name, tone: "success", timeoutMs: 3000 });
        } catch (error) {
          toast({ title: "Could not add to collection", message: error instanceof Error ? error.message : String(error), tone: "error" });
        }
      },
    },
    "preview-new-collection": {
      click: () => {
        const asset = state.preview;
        state.pendingCollectionAssetId = asset?.id ?? null;
        state.panel = "collections";
        emit("panel", "header");
      },
    },
    "preview-collection-select": { change: () => undefined },
  });
}
