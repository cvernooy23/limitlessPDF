#!/usr/bin/env node
/**
 * publish-update.mjs
 *
 * Generates the update manifest JSON for a given channel. Run from CI after a
 * successful build, against the flattened directory of release assets.
 *
 * Usage:
 *   node scripts/publish-update.mjs <channel> <version> <notes> <assets-dir> [release-tag]
 *
 * <assets-dir> must contain the Tauri updater bundles (created by
 * `createUpdaterArtifacts: "v1Compatible"`) with their .sig files next to
 * them. Each bundle must also be uploaded as an asset of the GitHub release
 * named by [release-tag] (default: v<version>), because the manifest points
 * at releases/download/<release-tag>/<bundle>.
 *
 * Output: update/<channel>.json
 */

import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "fs";
import { join } from "path";

const [channel, version, notes, assetsDir, releaseTag = `v${version}`] = process.argv.slice(2);

if (!channel || !version || !assetsDir) {
  console.error(
    "Usage: node scripts/publish-update.mjs <channel> <version> <notes> <assets-dir> [release-tag]",
  );
  process.exit(1);
}

const GITHUB_REPO = "cvernooy23/limitlessPDF";

// Updater bundle filename -> Tauri updater platform keys.
// The updater looks up "<os>-<arch>-<installer>" first, then "<os>-<arch>".
const PLATFORM_MAP = [
  // NSIS is the default Windows installer; MSI installs get their own key.
  { match: /\.nsis\.zip$/, keys: ["windows-x86_64", "windows-x86_64-nsis"] },
  { match: /\.msi\.zip$/, keys: ["windows-x86_64-msi"] },
  { match: /_amd64\.AppImage\.tar\.gz$/, keys: ["linux-x86_64"] },
  // macOS is built as a universal binary, so one bundle serves both arches.
  { match: /\.app\.tar\.gz$/, keys: ["darwin-aarch64", "darwin-x86_64"] },
];

// Every channel must serve these, or users on that platform silently stop
// receiving updates.
const REQUIRED_KEYS = ["windows-x86_64", "linux-x86_64", "darwin-aarch64", "darwin-x86_64"];

const platforms = {};
const errors = [];

for (const file of readdirSync(assetsDir).sort()) {
  if (!file.endsWith(".sig")) continue;
  const bundle = file.slice(0, -".sig".length);
  const entry = PLATFORM_MAP.find(({ match }) => match.test(bundle));
  if (!entry) continue; // signature for a plain installer, not an updater bundle

  if (!existsSync(join(assetsDir, bundle))) {
    errors.push(`${file} found but ${bundle} is missing from ${assetsDir}`);
    continue;
  }

  const signature = readFileSync(join(assetsDir, file), "utf-8").trim();
  const url = `https://github.com/${GITHUB_REPO}/releases/download/${releaseTag}/${encodeURIComponent(bundle)}`;
  for (const key of entry.keys) {
    platforms[key] = { signature, url };
    console.log(`${key} -> ${bundle}`);
  }
}

const missing = REQUIRED_KEYS.filter((key) => !platforms[key]);
if (missing.length > 0) {
  errors.push(`no updater bundle for: ${missing.join(", ")}`);
}
if (errors.length > 0) {
  for (const error of errors) console.error(`error: ${error}`);
  process.exit(1);
}

const manifest = {
  version,
  notes: notes || `${channel} release v${version}`,
  pub_date: new Date().toISOString(),
  platforms,
};

mkdirSync("update", { recursive: true });
const outPath = join("update", `${channel}.json`);
writeFileSync(outPath, JSON.stringify(manifest, null, 2) + "\n");
console.log(
  `Wrote ${outPath} (version ${version}, ${Object.keys(platforms).length} platform keys)`,
);
