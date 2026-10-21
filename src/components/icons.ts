/** Stockcraft icon set: uniform 24x24 grid, 1.7 stroke, round caps. */
import { raw, type Raw } from "../lib/dom";

export type IconName =
  | "search"
  | "close"
  | "heart"
  | "heart-filled"
  | "layers"
  | "download"
  | "sliders"
  | "sun"
  | "moon"
  | "filter"
  | "chevron-down"
  | "chevron-left"
  | "chevron-right"
  | "external"
  | "info"
  | "warning"
  | "check"
  | "plus"
  | "trash"
  | "pencil"
  | "image"
  | "camera"
  | "brush"
  | "grid"
  | "refresh"
  | "eye"
  | "panel"
  | "clock"
  | "shield"
  | "folder"
  | "globe"
  | "sparkle"
  | "vector"
  | "business"
  | "technology"
  | "food"
  | "travel"
  | "people"
  | "abstract"
  | "architecture"
  | "nature"
  | "background";

const PATHS: Record<IconName, string> = {
  search: '<circle cx="11" cy="11" r="6.5"/><path d="M16 16l4.5 4.5"/>',
  close: '<path d="M6 6l12 12M18 6L6 18"/>',
  heart: '<path d="M12 20s-7-4.4-7-9.4A4.1 4.1 0 0 1 12 8a4.1 4.1 0 0 1 7 2.6c0 5-7 9.4-7 9.4z"/>',
  "heart-filled":
    '<path fill="currentColor" stroke="none" d="M12 20.3l-1.1-.7C6.2 16.5 3 13.9 3 10.5A4.6 4.6 0 0 1 7.6 6c1.7 0 3.2.9 4.4 2.3C13.2 6.9 14.7 6 16.4 6A4.6 4.6 0 0 1 21 10.5c0 3.4-3.2 6-7.9 9.1z"/>',
  layers: '<path d="M12 3.5l8.5 4.6L12 12.7 3.5 8.1z"/><path d="M4.6 12.5L12 16.5l7.4-4"/><path d="M4.6 16.4L12 20.4l7.4-4"/>',
  download: '<path d="M12 4v10"/><path d="M7.7 10.2L12 14.5l4.3-4.3"/><path d="M4.5 18.5h15"/>',
  sliders: '<path d="M5 7h9M18 7h1.5M5 17h3M11.5 17H19"/><circle cx="16" cy="7" r="2.1"/><circle cx="9.5" cy="17" r="2.1"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 3v2.2M12 18.8V21M3 12h2.2M18.8 12H21M5.6 5.6l1.6 1.6M16.8 16.8l1.6 1.6M18.4 5.6l-1.6 1.6M7.2 16.8l-1.6 1.6"/>',
  moon: '<path d="M20 14.5A8.2 8.2 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5z"/>',
  filter: '<path d="M4 6h16M7 12h10M10 18h4"/>',
  "chevron-down": '<path d="M6 9.5l6 6 6-6"/>',
  "chevron-left": '<path d="M14.5 6l-6 6 6 6"/>',
  "chevron-right": '<path d="M9.5 6l6 6-6 6"/>',
  external: '<path d="M14 4h6v6"/><path d="M20 4l-8.5 8.5"/><path d="M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"/>',
  info: '<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5.5"/><circle cx="12" cy="7.9" r="0.9" fill="currentColor" stroke="none"/>',
  warning: '<path d="M12 4.5l8.5 15h-17z"/><path d="M12 10v4.2"/><circle cx="12" cy="16.8" r="0.9" fill="currentColor" stroke="none"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  trash: '<path d="M4.5 7h15"/><path d="M9.5 7V5.2h5V7"/><path d="M6.5 7l.9 12.2h9.2L17.5 7"/><path d="M10.5 10.5v6M13.5 10.5v6"/>',
  pencil: '<path d="M4.8 19.2l4.4-1.1L19 8.3a2 2 0 0 0-2.8-2.8L6.4 15.2z"/><path d="M14.5 6.5l3 3"/>',
  image: '<rect x="3.5" y="4.5" width="17" height="15" rx="2.2"/><circle cx="8.7" cy="9.6" r="1.6"/><path d="M4 17l4.6-4.4 3.3 3 3-2.6 5.1 4.6"/>',
  camera: '<path d="M4 8.5h3l1.3-2h7.4l1.3 2h3v10H4z"/><circle cx="12" cy="13.4" r="3.4"/>',
  brush: '<path d="M5 19.5c3.2.6 5-1.4 5-3.4 0-1.6-1-2.4-2.3-2.4-2 0-2.7 2-2.7 5.8z"/><path d="M9.8 13.2l8.2-8.2a2 2 0 0 1 2.8 2.8l-8.2 8.2"/>',
  grid: '<rect x="4" y="4" width="7" height="7" rx="1.6"/><rect x="13" y="4" width="7" height="7" rx="1.6"/><rect x="4" y="13" width="7" height="7" rx="1.6"/><rect x="13" y="13" width="7" height="7" rx="1.6"/>',
  refresh: '<path d="M20 12a8 8 0 1 1-2.6-5.9"/><path d="M20 4.5V10h-5.4"/>',
  eye: '<path d="M2.8 12S6.4 6.5 12 6.5 21.2 12 21.2 12 17.6 17.5 12 17.5 2.8 12 2.8 12z"/><circle cx="12" cy="12" r="2.7"/>',
  panel: '<rect x="3.5" y="4.5" width="17" height="15" rx="2.2"/><path d="M9.5 4.5v15"/>',
  clock: '<circle cx="12" cy="12" r="8.5"/><path d="M12 7.6V12l3.2 2"/>',
  shield: '<path d="M12 3.5l7 2.6v5.6c0 4.3-3 7.6-7 8.8-4-1.2-7-4.5-7-8.8V6.1z"/><path d="M9.2 12.2l2 2 3.6-4"/>',
  folder: '<path d="M3.6 6.6h5.2l1.8 2.2h9.8v9.6H3.6z"/><path d="M3.6 8.8V6.6"/>',
  globe: '<circle cx="12" cy="12" r="8.5"/><path d="M3.6 12h16.8"/><path d="M12 3.5c2.6 2.4 3.9 5.3 3.9 8.5s-1.3 6.1-3.9 8.5c-2.6-2.4-3.9-5.3-3.9-8.5S9.4 5.9 12 3.5z"/>',
  sparkle: '<path d="M12 3.5l1.9 5.1 5.1 1.9-5.1 1.9L12 17.5l-1.9-5.1L5 10.5l5.1-1.9z"/><path d="M18.6 16.4l.8 2 2 .8-2 .8-.8 2-.8-2-2-.8 2-.8z"/>',
  vector: '<path d="M6 6.5h4.5v4.5H6z"/><path d="M13.5 13H18v4.5h-4.5z"/><path d="M10.5 6.5h3a2 2 0 0 1 2 2V13"/>',
  business: '<path d="M4 8.5h16v11H4z"/><path d="M9 8.5V6.2h6v2.3"/><path d="M4 13.4h16"/>',
  technology: '<rect x="4" y="5" width="16" height="11" rx="2"/><path d="M9.5 19.5h5M12 16v3.5"/>',
  food: '<path d="M6 4v7.5a2.4 2.4 0 0 0 4.8 0V4"/><path d="M8.4 4v16"/><path d="M16.5 4c2 1.2 2.6 3.4 2.6 5.6 0 1.7-.8 2.6-2.6 2.8V20"/>',
  travel: '<path d="M4 13.2l16-6.2-4.6 13-3.2-4.4z"/><path d="M12.2 15.6L20 7"/>',
  people: '<circle cx="9" cy="9" r="3"/><path d="M3.6 19.4c0-3 2.4-5 5.4-5s5.4 2 5.4 5"/><path d="M15.5 6.6a3 3 0 0 1 0 5.3"/><path d="M17 14.9c2.1.4 3.6 1.9 3.6 4"/>',
  abstract:
    '<path d="M4.5 15.5c3.4-7 5.6-7.4 8.2-3.2 1.9 3 4 3.2 6.8-1.3"/><circle cx="8" cy="7.5" r="2.2"/><path d="M15.5 5.5l4.5 4.5"/>',
  architecture: '<path d="M5 20V9.5L12 4l7 5.5V20z"/><path d="M9.5 20v-5h5v5"/><path d="M5 20h14"/>',
  nature: '<path d="M12 20V9.5"/><path d="M12 12.5C12 8.6 9 6 5 6c0 4 2.6 6.8 7 6.8z"/><path d="M12 14.4c0-3.4 2.6-5.6 6.4-5.6 0 3.5-2.6 5.9-6.4 5.9z"/>',
  background:
    '<rect x="3.5" y="4.5" width="17" height="15" rx="2.2"/><path d="M3.5 15l5-4.6 4.4 4 2.7-2.4 4.9 4.4"/>',
};

export function icon(name: IconName, size = 24): Raw {
  const path = PATHS[name] ?? PATHS.image;
  return raw(
    `<svg viewBox="0 0 24 24" width="${size}" height="${size}" fill="none" stroke="currentColor" ` +
      `stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">${path}</svg>`,
  );
}

export function brandMark(size = 26): Raw {
  // Original cross-hatched gorilla artwork supplied for Stockcraft, shipped as the app icon.
  const radius = Math.max(6, Math.round(size * 0.2));
  return raw(
    `<img class="brand-img" src="/icon.png" width="${size}" height="${size}" ` +
      `alt="" aria-hidden="true" focusable="false" style="border-radius:${radius}px" />`,
  );
}
