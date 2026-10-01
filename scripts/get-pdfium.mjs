#!/usr/bin/env node
/**
 * Downloads the correct prebuilt PDFium dynamic library for the current
 * platform/arch and drops it next to the Rust crate (`src-tauri/`) and into
 * `src-tauri/resources/`, where release builds pick it up via tauri.conf.json.
 *
 * Usage:  npm run get-pdfium
 *
 * On macOS this fetches BOTH arm64 and x86_64 and `lipo`s them into a single
 * universal `libpdfium.dylib`, so the universal app bundle works on Apple
 * Silicon and Intel alike regardless of which runner built it.
 *
 * No dependencies — uses Node's built-in fetch (Node 18+), the `tar` CLI that
 * ships with Windows 10+, macOS, and Linux, and `lipo` (macOS only).
 */
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, copyFileSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = "bblanchon/pdfium-binaries";
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const srcTauri = join(root, "src-tauri");
// Two destinations:
//   src-tauri/            → found by `tauri dev` (cwd lookup)
//   src-tauri/resources/  → bundled into release builds via tauri.conf.json
const destDirs = [srcTauri, join(srcTauri, "resources")];

const platform = process.platform; // 'win32' | 'darwin' | 'linux'
const arch = process.arch === "arm64" ? "arm64" : "x64";

// Each platform lists one or more release assets; when more than one is given
// their inner libraries are merged into a universal binary (macOS/lipo).
const matrix = {
  win32: { assets: [`pdfium-win-${arch}.tgz`], inner: "bin/pdfium.dll", out: "pdfium.dll" },
  linux: { assets: [`pdfium-linux-${arch}.tgz`], inner: "lib/libpdfium.so", out: "libpdfium.so" },
  darwin: {
    assets: ["pdfium-mac-arm64.tgz", "pdfium-mac-x64.tgz"],
    inner: "lib/libpdfium.dylib",
    out: "libpdfium.dylib",
  },
};

const target = matrix[platform];
if (!target) {
  console.error(`Unsupported platform: ${platform}`);
  process.exit(1);
}

async function fetchWithRetry(url, attempts = 5) {
  let lastErr;
  for (let i = 1; i <= attempts; i++) {
    try {
      const res = await fetch(url, { redirect: "follow" });
      if (res.ok) return res;
      // GitHub's release CDN intermittently returns 5xx/429; retry those.
      if (res.status >= 500 || res.status === 429) {
        lastErr = new Error(`HTTP ${res.status}`);
      } else {
        throw new Error(`Download failed (HTTP ${res.status}) from ${url}`);
      }
    } catch (e) {
      lastErr = e;
    }
    if (i < attempts) {
      const delay = 1000 * 2 ** (i - 1); // 1s, 2s, 4s, 8s
      console.log(`  ! ${lastErr.message} — retry ${i}/${attempts - 1} in ${delay}ms`);
      await new Promise((r) => setTimeout(r, delay));
    }
  }
  throw new Error(`Download failed after ${attempts} attempts from ${url}: ${lastErr.message}`);
}

async function download(asset, work) {
  const url = `https://github.com/${REPO}/releases/latest/download/${asset}`;
  console.log(`→ Downloading ${asset} ...`);
  const res = await fetchWithRetry(url);
  const tgz = join(work, asset);
  writeFileSync(tgz, Buffer.from(await res.arrayBuffer()));

  const extractDir = join(work, asset.replace(/[^a-zA-Z0-9]/g, "_"));
  mkdirSync(extractDir, { recursive: true });
  execFileSync("tar", ["-xzf", tgz, "-C", extractDir], { stdio: "inherit" });

  const libSrc = join(extractDir, target.inner);
  if (!existsSync(libSrc)) {
    throw new Error(`Expected library not found in ${asset}: ${target.inner}`);
  }
  return libSrc;
}

async function main() {
  console.log(`→ Platform: ${platform} ${arch}`);
  const work = mkdtempSync(join(tmpdir(), "pdfium-"));

  const libs = [];
  for (const asset of target.assets) {
    libs.push(await download(asset, work));
  }

  // Produce the final library (merging to universal when >1 slice).
  let finalLib;
  if (libs.length > 1) {
    console.log("→ Merging into a universal binary with lipo ...");
    finalLib = join(work, target.out);
    execFileSync("lipo", ["-create", ...libs, "-output", finalLib], { stdio: "inherit" });
  } else {
    finalLib = libs[0];
  }

  for (const dir of destDirs) {
    mkdirSync(dir, { recursive: true });
    const libDest = join(dir, target.out);
    copyFileSync(finalLib, libDest);
    console.log(`✓ Installed ${target.out} → ${libDest}`);
  }

  console.log('  Dev: run `npm run app:dev` — the badge should read "PDFium ready".');
  console.log("  Build: `npm run app:build` bundles it into the installer automatically.");
}

main().catch((err) => {
  console.error(`✗ ${err.message}`);
  console.error('  You can install PDFium manually — see README "PDFium binaries".');
  process.exit(1);
});
