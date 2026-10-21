/** Collections drawer: create, rename, delete and open named collections. */
import { api } from "../../lib/api";
import { html, qs, raw, render, registerActions } from "../../lib/dom";
import { formatRelative } from "../../lib/format";
import { emit, set, state } from "../../lib/store";
import { icon } from "../../components/icons";
import { button, confirmDialog, emptyState, iconButton, toast } from "../../components/ui";

export function renderCollectionsPanel(): void {
  const host = qs("#panel-host");
  if (!host) return;

  render(
    host,
    html`<div class="drawer-scrim" data-action="close-panel"></div>
      <aside class="drawer" role="dialog" aria-modal="true" aria-label="Collections" data-stop-click>
        <div class="drawer-head">
          <span style="color:var(--brand)">${icon("layers", 20)}</span>
          <h2>Collections</h2>
          ${iconButton({ action: "close-panel", iconName: "close", label: "Close collections" })}
        </div>
        <div class="drawer-body">
          <div class="section">
            <h3>New collection</h3>
            <form class="row" data-action="create-collection-form">
              <input
                type="text"
                id="new-collection-name"
                data-action="new-collection-name"
                value="${state.newCollectionName}"
                placeholder="e.g. Web hero images"
                aria-label="New collection name"
                maxlength="80"
              />
              <button type="submit" class="btn btn-primary" data-action="create-collection-form">Create</button>
            </form>
            ${state.pendingCollectionAssetId
              ? html`<div class="tiny muted">The image you selected will be added to the collection you create next.</div>`
              : ""}
          </div>

          ${state.collections.length
            ? html`<div class="section">
                <h3>${state.collections.length} collection(s)</h3>
                ${state.collections.map(
                  (collection) => raw(html`
                    <div class="card">
                      <div class="row between">
                        ${state.renamingId === collection.id
                          ? html`<input
                              class="grow"
                              type="text"
                              id="rename-input"
                              data-action="rename-draft"
                              data-collection="${collection.id}"
                              value="${state.renameDraft}"
                              aria-label="Rename collection"
                            />`
                          : html`<div class="grow">
                              <div class="tile-title">${collection.name}</div>
                              <div class="tiny muted">
                                ${collection.itemCount} image${collection.itemCount === 1 ? "" : "s"} · updated ${formatRelative(collection.updatedAt)}
                              </div>
                            </div>`}
                        <div class="row">
                          ${state.renamingId === collection.id
                            ? button({ action: "rename-collection-save", label: "Save", variant: "primary", size: "sm", data: { collection: collection.id } })
                            : button({ action: "rename-collection", label: "Rename", variant: "ghost", size: "sm", iconName: "pencil", data: { collection: collection.id } })}
                          ${button({ action: "open-collection", label: "Open", variant: "ghost", size: "sm", iconName: "image", data: { collection: collection.id } })}
                          ${iconButton({
                            action: "delete-collection",
                            iconName: "trash",
                            label: `Delete collection ${collection.name}`,
                            title: "Delete collection",
                            size: 15,
                            data: { collection: collection.id, name: collection.name },
                          })}
                        </div>
                      </div>
                      ${collection.description ? html`<div class="tiny muted">${collection.description}</div>` : ""}
                    </div>
                  `),
                )}
              </div>`
            : raw(emptyState({ title: "No collections yet", message: "Create a collection to group images for a project. Collections are stored locally in SQLite.", iconName: "layers" }).value)}

          ${raw(
            html`<div class="tiny muted">
              Collections store a reference to each image (asset id and original page URL) plus a local metadata snapshot.
              Downloaded files stay in your download folder and are never copied into the database.
            </div>`,
          )}
        </div>
      </aside>`,
  );
}

export function registerCollectionActions(): void {
  registerActions({
    "show-collections": {
      click: () => {
        state.panel = "collections";
        state.renamingId = null;
        emit("panel", "header");
      },
    },
    "new-collection-name": { input: (element) => set({ newCollectionName: (element as HTMLInputElement).value }) },
    "create-collection-form": {
      submit: () => void createCollection(),
      click: () => void createCollection(),
    },
    "rename-collection": {
      click: (element) => {
        const id = Number(element.dataset.collection);
        const collection = state.collections.find((c) => c.id === id);
        set({ renamingId: id, renameDraft: collection?.name ?? "" }, "panel");
        qs<HTMLInputElement>("#rename-input")?.focus();
      },
    },
    "rename-draft": { input: (element) => set({ renameDraft: (element as HTMLInputElement).value }) },
    "rename-collection-save": {
      click: async (element) => {
        const id = Number(element.dataset.collection);
        const name = state.renameDraft.trim();
        if (!name) {
          toast({ title: "Name cannot be empty", tone: "warn" });
          return;
        }
        try {
          const collection = await api.renameCollection(id, name);
          state.collections = state.collections.map((c) => (c.id === id ? collection : c));
          set({ renamingId: null }, "panel");
          toast({ title: "Collection renamed", message: collection.name, tone: "success", timeoutMs: 2500 });
        } catch (error) {
          toast({ title: "Could not rename collection", message: error instanceof Error ? error.message : String(error), tone: "error" });
        }
      },
    },
    "delete-collection": {
      click: async (element) => {
        const id = Number(element.dataset.collection);
        const name = element.dataset.name ?? "collection";
        const confirmed = await confirmDialog({
          title: `Delete “${name}”?`,
          message: "The collection and its membership list will be removed. Downloaded files and favourites are not affected.",
          confirmLabel: "Delete collection",
          danger: true,
        });
        if (!confirmed) return;
        try {
          await api.deleteCollection(id);
          state.collections = state.collections.filter((c) => c.id !== id);
          if (state.view === "collection" && state.activeCollectionId === id) {
            state.view = "search";
            state.activeCollectionId = null;
          }
          toast({ title: "Collection deleted", message: name, tone: "info", timeoutMs: 3000 });
          emit("panel", "header", "toolbar", "results");
        } catch (error) {
          toast({ title: "Could not delete collection", message: error instanceof Error ? error.message : String(error), tone: "error" });
        }
      },
    },
  });
}

async function createCollection(): Promise<void> {
  const name = state.newCollectionName.trim();
  if (!name) {
    toast({ title: "Enter a collection name first", tone: "warn" });
    qs<HTMLInputElement>("#new-collection-name")?.focus();
    return;
  }
  try {
    const collection = await api.createCollection(name);
    state.collections = [...state.collections, collection];
    state.newCollectionName = "";
    let message = `${collection.name} created.`;
    if (state.pendingCollectionAssetId) {
      const asset = state.assets.find((a) => a.id === state.pendingCollectionAssetId) ?? state.preview;
      if (asset) {
        const updated = await api.addToCollection(collection.id, asset);
        state.collections = state.collections.map((c) => (c.id === updated.id ? updated : c));
        state.pendingCollectionAssetId = null;
        message = `${collection.name} created and the selected image was added.`;
      }
    }
    toast({ title: "Collection created", message, tone: "success", timeoutMs: 3500 });
    emit("panel", "header");
  } catch (error) {
    toast({ title: "Could not create collection", message: error instanceof Error ? error.message : String(error), tone: "error" });
  }
}
