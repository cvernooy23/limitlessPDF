#!/usr/bin/env node
/**
 * publish-update.mjs
 *
 * Generates the update manifest JSON for a given channel and writes it to the
 * gh-pages branch directory structure. Run from CI after a successful build.
 *
 * Usage:
 *   node scripts/publish-update.mjs <channel> <version> <notes> <artifacts-dir>
 *
 * The artifacts-dir should contain the Tauri update bundles (.sig files next to
 * their installers). The script reads the .sig files to populate the signature
 * field in the manifest.
 *
 * Output: update/<channel>.json
 */

import { readFileSync, writeFileSync, mkdirSync, readdirSync } from "fs";
import { join, basename } from "path";

const [channel, version, notes, artifactsDir] = process.argv.slice(2);

if (!channel || !version || !artifactsDir) {
  console.error(
    "Usage: node scripts/publish-update.mjs <channel> <version> <notes> <artifacts-dir>",
  );
  process.exit(1);
}

// Map artifact filenames to Tauri platform keys.
// Tauri uses the format: <target>-<arch>
const PLATFORM_MAP = [
  { match: /\.msi\.zip$/, target: "windows", arch: "x86_64", key: "windows-x86_64" },
  { match: /\.nsis\.zip$/, target: "windows", arch: "x86_64", key: "windows-x86_64" },
  { match: /_amd64\.AppImage\.tar\.gz$/, target: "linux", arch: "x86_64", key: "linux-x86_64" },
  { match: /\.app\.tar\.gz$/, target: "darwin", arch: "aarch64", key: "darwin-aarch64" },
  { match: /_x64\.app\.tar\.gz$/, target: "darwin", arch: "x86_64", key: "darwin-x86_64" },
];

const GITHUB_REPO = "cvernooy23/limitlessPDF";

function findPlatformArtifacts(dir) {
  const files = readdirSync(dir);
  const platforms = {};

  for (const file of files) {
    // Only process .sig files — each one corresponds to an update bundle.
    if (!file.endsWith(".sig")) continue;

    const bundleName = file.replace(/\.sig$/, "");
    const sig = readFileSync(join(dir, file), "utf-8").trim();

    for (const { match, key } of PLATFORM_MAP) {
      if (match.test(bundleName)) {
        // The download URL points to the GitHub release asset.
        const downloadUrl = `https://github.com/${GITHUB_REPO}/releases/download/v${version}/${bundleName}`;
        platforms[key] = { signature: sig, url: downloadUrl };
        break;
      }
    }
  }

  return platforms;
}

const platforms = findPlatformArtifacts(artifactsDir);

if (Object.keys(platforms).length === 0) {
  console.error("No platform artifacts found in", artifactsDir);
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
console.log(`Wrote ${outPath} with ${Object.keys(platforms).length} platform(s)`);
