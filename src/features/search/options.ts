import type { IconName } from "../../components/icons";
import type { Asset, AssetFilters } from "../../lib/types";

export interface CategoryOption {
  id: string;
  label: string;
  icon: IconName;
  /** Keyword appended to the query when the category is chosen. */
  term?: string;
  /** Local media-type filter applied when the category is chosen. */
  mediaType?: string;
  note?: string;
  disabled?: boolean;
}

/**
 * Categories are honest about what they do: a `term` category adds a keyword to the search query,
 * a `mediaType` category applies a local filter, and categories no enabled provider can support are
 * disabled with an explanation instead of pretending to work.
 */
export const CATEGORIES: CategoryOption[] = [
  { id: "all", label: "All images", icon: "grid" },
  { id: "photos", label: "Photos", icon: "camera", mediaType: "photo" },
  { id: "illustrations", label: "Illustrations", icon: "brush", mediaType: "illustration" },
  { id: "vectors", label: "Vectors", icon: "vector", disabled: true, note: "no enabled source", mediaType: "vector" },
  { id: "backgrounds", label: "Backgrounds", icon: "background", term: "background" },
  { id: "nature", label: "Nature", icon: "nature", term: "nature" },
  { id: "architecture", label: "Architecture", icon: "architecture", term: "architecture" },
  { id: "technology", label: "Technology", icon: "technology", term: "technology" },
  { id: "business", label: "Business", icon: "business", term: "business" },
  { id: "food", label: "Food", icon: "food", term: "food" },
  { id: "travel", label: "Travel", icon: "travel", term: "travel" },
  { id: "people", label: "People", icon: "people", term: "people" },
  { id: "abstract", label: "Abstract", icon: "abstract", term: "abstract" },
];

export const SORTS = [
  { value: "relevance", label: "Relevance" },
  { value: "newest", label: "Newest (local)" },
  { value: "largest", label: "Largest (local)" },
];

export const MEDIA_TYPES = [
  { value: "any", label: "Any type" },
  { value: "photo", label: "Photo" },
  { value: "illustration", label: "Illustration" },
  { value: "artwork", label: "Artwork" },
  { value: "diagram", label: "Diagram / map" },
];

export const LICENSES = [
  { value: "any", label: "Any licence" },
  { value: "cc0", label: "CC0 / public domain" },
  { value: "by", label: "CC BY" },
  { value: "by-sa", label: "CC BY-SA" },
  { value: "other", label: "Other / unknown" },
];

export const COLORS = [
  { value: "any", label: "Any", swatch: "transparent" },
  { value: "red", label: "Red", swatch: "#d64545" },
  { value: "orange", label: "Orange", swatch: "#e08a2e" },
  { value: "yellow", label: "Yellow", swatch: "#e0c53a" },
  { value: "green", label: "Green", swatch: "#3f9a5a" },
  { value: "teal", label: "Teal", swatch: "#2f9a9a" },
  { value: "blue", label: "Blue", swatch: "#3a6fd8" },
  { value: "purple", label: "Purple", swatch: "#7a4fd8" },
  { value: "pink", label: "Pink", swatch: "#d8569a" },
  { value: "brown", label: "Brown", swatch: "#8a6242" },
  { value: "grey", label: "Grey", swatch: "#8b93a3" },
  { value: "black", label: "Black", swatch: "#1b2130" },
  { value: "white", label: "White", swatch: "#f2f4f8" },
];

export const ORIENTATIONS = [
  { value: "any", label: "Any orientation" },
  { value: "landscape", label: "Landscape" },
  { value: "portrait", label: "Portrait" },
  { value: "square", label: "Square" },
];

export function orientationOf(asset: Asset): "landscape" | "portrait" | "square" | "unknown" {
  if (!asset.width || !asset.height) return "unknown";
  const ratio = asset.width / asset.height;
  if (ratio > 1.08) return "landscape";
  if (ratio < 0.92) return "portrait";
  return "square";
}

export function licenseMatches(filter: string, asset: Asset): boolean {
  if (filter === "any") return true;
  const name = `${asset.licenseName ?? ""} ${asset.licenseUrl ?? ""}`.toLowerCase();
  if (!name.trim()) return filter === "other";
  switch (filter) {
    case "cc0":
      return /cc0|public domain|pdm|pd-|no known copyright/.test(name);
    case "by":
      return /by\b|attribution/.test(name) && !/by-sa|by-nc|by-nd/.test(name);
    case "by-sa":
      return /by-sa|sharealike/.test(name);
    case "other":
      return !/cc0|public domain|pdm|no known copyright|by\b|attribution/.test(name);
    default:
      return true;
  }
}

/** Local filter used for the parts of the filter set that providers do not support natively. */
export function matchesFilters(asset: Asset, filters: AssetFilters): boolean {
  if (filters.orientation !== "any" && orientationOf(asset) !== filters.orientation) return false;
  if (filters.minWidth > 0 && (asset.width ?? 0) < filters.minWidth) return false;
  if (filters.minHeight > 0 && (asset.height ?? 0) < filters.minHeight) return false;
  if (filters.mediaType !== "any") {
    const media = (asset.mediaType ?? "").toLowerCase();
    if (filters.mediaType === "diagram") {
      if (!/diagram|map|chart|drawing/.test(media)) return false;
    } else if (media !== filters.mediaType) return false;
  }
  if (!licenseMatches(filters.license, asset)) return false;
  if (filters.color !== "any") {
    if (!asset.dominantColor) return false;
    if (asset.dominantColor !== filters.color) return false;
  }
  return true;
}

export function activeFilterCount(filters: AssetFilters): number {
  let count = 0;
  if (filters.orientation !== "any") count += 1;
  if (filters.minWidth > 0) count += 1;
  if (filters.minHeight > 0) count += 1;
  if (filters.mediaType !== "any") count += 1;
  if (filters.license !== "any") count += 1;
  if (filters.color !== "any") count += 1;
  if (filters.category !== "all") count += 1;
  return count;
}

export function sortAssets(assets: Asset[], sort: string): Asset[] {
  if (sort === "relevance") return assets;
  const copy = [...assets];
  if (sort === "newest") {
    copy.sort((a, b) => {
      const at = a.createdAt ? Date.parse(a.createdAt) : 0;
      const bt = b.createdAt ? Date.parse(b.createdAt) : 0;
      if (at === bt) return 0;
      if (!at) return 1;
      if (!bt) return -1;
      return bt - at;
    });
  } else if (sort === "largest") {
    copy.sort((a, b) => (b.width ?? 0) * (b.height ?? 0) - (a.width ?? 0) * (a.height ?? 0));
  }
  return copy;
}
