#!/usr/bin/env node
/**
 * Downloads the correct prebuilt PDFium dynamic library for the current
 * platform/arch and drops it next to the Rust crate (`src-tauri/`), which is
 * where the dev build looks for it.
 *
 * Usage:  npm run get-pdfium
 *
 * No dependencies — uses Node's built-in fetch (Node 18+) and the `tar` CLI
 * that ships with Windows 10+, macOS, and Linux.
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

// Map Node's platform/arch onto the release asset + the library file inside it.
const platform = process.platform; // 'win32' | 'darwin' | 'linux'
const arch = process.arch === "arm64" ? "arm64" : "x64";

const matrix = {
  win32: { asset: `pdfium-win-${arch}.tgz`, inner: "bin/pdfium.dll", out: "pdfium.dll" },
  darwin: { asset: `pdfium-mac-${arch}.tgz`, inner: "lib/libpdfium.dylib", out: "libpdfium.dylib" },
  linux: { asset: `pdfium-linux-${arch}.tgz`, inner: "lib/libpdfium.so", out: "libpdfium.so" },
};

const target = matrix[platform];
if (!target) {
  console.error(`Unsupported platform: ${platform}`);
  process.exit(1);
}

const url = `https://github.com/${REPO}/releases/latest/download/${target.asset}`;

async function main() {
  console.log(`→ Platform: ${platform} ${arch}`);
  console.log(`→ Downloading ${target.asset} ...`);

  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) {
    throw new Error(`Download failed (HTTP ${res.status}) from ${url}`);
  }

  const work = mkdtempSync(join(tmpdir(), "pdfium-"));
  const tgz = join(work, target.asset);
  writeFileSync(tgz, Buffer.from(await res.arrayBuffer()));

  console.log("→ Extracting ...");
  const extractDir = join(work, "out");
  mkdirSync(extractDir, { recursive: true });
  execFileSync("tar", ["-xzf", tgz, "-C", extractDir], { stdio: "inherit" });

  const libSrc = join(extractDir, target.inner);
  if (!existsSync(libSrc)) {
    throw new Error(`Expected library not found in archive: ${target.inner}`);
  }

  for (const dir of destDirs) {
    mkdirSync(dir, { recursive: true });
    const libDest = join(dir, target.out);
    copyFileSync(libSrc, libDest);
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
