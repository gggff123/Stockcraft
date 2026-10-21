import type { Asset, ProviderState } from "./types";

export function formatBytes(bytes: number | null | undefined): string {
  const value = Number(bytes ?? 0);
  if (!Number.isFinite(value) || value <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const exponent = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
  const scaled = value / 1024 ** exponent;
  return `${scaled >= 10 || exponent === 0 ? Math.round(scaled) : scaled.toFixed(1)} ${units[exponent]}`;
}

export function formatDimensions(asset: Asset): string {
  if (!asset.width || !asset.height) return "unknown size";
  return `${asset.width} × ${asset.height} px`;
}

export function formatMegapixels(asset: Asset): string {
  if (!asset.width || !asset.height) return "";
  const mp = (asset.width * asset.height) / 1_000_000;
  return `${mp.toFixed(mp >= 10 ? 0 : 1)} MP`;
}

export function orientationOf(asset: Asset): "landscape" | "portrait" | "square" | "unknown" {
  if (!asset.width || !asset.height) return "unknown";
  const ratio = asset.width / asset.height;
  if (ratio > 1.08) return "landscape";
  if (ratio < 0.92) return "portrait";
  return "square";
}

export function hostLabel(url: string | null | undefined): string {
  if (!url) return "";
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return "";
  }
}

export function formatRelative(iso: string | null | undefined): string {
  if (!iso) return "";
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "";
  const diff = Date.now() - then;
  const minutes = Math.round(diff / 60000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days} d ago`;
  return new Date(then).toLocaleDateString();
}

export function licenseShort(asset: Asset): string {
  const name = (asset.licenseName ?? "").trim();
  if (!name) return "licence unknown";
  return name.replace(/\s*(1\.0|2\.0|2\.5|3\.0|4\.0)$/, " $1").trim();
}

export function mediaTypeLabel(mediaType: string): string {
  switch (mediaType) {
    case "photo":
      return "Photo";
    case "illustration":
      return "Illustration";
    case "artwork":
      return "Artwork";
    case "diagram":
      return "Diagram";
    case "map":
      return "Map";
    case "vector":
      return "Vector";
    default:
      return mediaType ? mediaType.charAt(0).toUpperCase() + mediaType.slice(1) : "Image";
  }
}

export function stateLabel(state: ProviderState, count: number): string {
  switch (state) {
    case "idle":
      return "idle";
    case "searching":
      return "searching…";
    case "ok":
      return `${count} result${count === 1 ? "" : "s"}`;
    case "empty":
      return "no results";
    case "unavailable":
      return "temporarily unavailable";
    case "timeout":
      return "timed out";
    case "rate_limited":
      return "rate limited";
    case "circuit_open":
      return "paused after failures";
    case "restricted":
      return "restricted by provider policy";
    case "disabled":
      return "disabled";
    case "malformed":
      return "unexpected response format";
    default:
      return String(state);
  }
}

export function safeFilename(input: string, fallback = "stockcraft-image"): string {
  const cleaned = (input || "")
    .normalize("NFKD")
    .replace(/[^\w.\- ]+/g, " ")
    .replace(/\s+/g, "-")
    .replace(/^[-.]+|[-.]+$/g, "")
    .slice(0, 80);
  return cleaned || fallback;
}

export function filenameFromUrl(url: string | null | undefined): string {
  if (!url) return "";
  try {
    const parsed = new URL(url);
    const last = parsed.pathname.split("/").filter(Boolean).pop() ?? "";
    return decodeURIComponent(last);
  } catch {
    return "";
  }
}

export function shorten(text: string | null | undefined, max = 160): string {
  const value = (text ?? "").replace(/\s+/g, " ").trim();
  return value.length > max ? `${value.slice(0, max - 1)}…` : value;
}

export function pluralize(count: number, singular: string, plural?: string): string {
  const word = count === 1 ? singular : (plural ?? `${singular}s`);
  return `${count} ${word}`;
}

export function summarizeProviders(statuses: { name: string; state: ProviderState }[]): string {
  const ok = statuses.filter((s) => s.state === "ok" || s.state === "empty").length;
  const failed = statuses.filter((s) => ["unavailable", "timeout", "rate_limited", "circuit_open", "malformed"].includes(s.state));
  const parts: string[] = [];
  if (ok) parts.push(`${ok} source${ok === 1 ? "" : "s"} responded`);
  if (failed.length) parts.push(`${failed.length} unavailable (${failed.map((f) => f.name).join(", ")})`);
  return parts.join(" · ");
}
