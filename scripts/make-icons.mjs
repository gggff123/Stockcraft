/**
 * Regenerates the Stockcraft icon set from the supplied gorilla artwork.
 *
 * Source: assets/stockcraft-icon-source.png (1254 x 1254, opaque blue square)
 * Run:    npm run icons
 *
 * Tauri's own icon generator creates the Windows ICO, macOS ICNS, iOS/Android
 * variants and the PNG sizes used by the bundle. The 256 px Windows tile is also
 * copied to public/icon.png for the header, welcome screen and favicon.
 */
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const source = join(root, "assets", "stockcraft-icon-source.png");

if (!existsSync(source)) {
  console.error(`Icon source not found: ${source}`);
  process.exit(1);
}

const cli = join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
const result = spawnSync(process.execPath, [cli, "icon", source], {
  cwd: root,
  stdio: "inherit",
});
if (result.status !== 0) {
  process.exit(result.status ?? 1);
}

copyFileSync(
  join(root, "src-tauri", "icons", "128x128@2x.png"),
  join(root, "public", "icon.png"),
);
console.log("Copied 256 px icon to public/icon.png");
