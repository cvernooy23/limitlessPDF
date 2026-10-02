#!/usr/bin/env node
/**
 * Downloads the correct prebuilt PDFium dynamic library for the build target.
 *
 * Desktop (default): drops the lib next to the Rust crate (`src-tauri/`) and
 * into `src-tauri/resources/`, where release builds pick it up via
 * tauri.conf.json. On macOS it fetches BOTH arches and `lipo`s them into one
 * universal `libpdfium.dylib`.
 *
 * Android: when invoked by Tauri's build hook with TAURI_ENV_PLATFORM=android,
 * it instead fetches the Android `libpdfium.so` per ABI and places each into
 * `src-tauri/gen/android/app/src/main/jniLibs/<abi>/`, where it is packaged
 * into the APK and found by the system loader at runtime. ABIs come from
 * PDFIUM_ANDROID_ABIS (comma-separated); default: arm64-v8a.
 *
 * No dependencies — uses Node's built-in fetch (Node 18+), the `tar` CLI, and
 * `lipo` (macOS only).
 */
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, copyFileSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = "bblanchon/pdfium-binaries";
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const srcTauri = join(root, "src-tauri");

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

/** Download `asset`, extract it, and return the path to `inner` inside it. */
async function fetchLib(asset, inner, work) {
  const url = `https://github.com/${REPO}/releases/latest/download/${asset}`;
  console.log(`→ Downloading ${asset} ...`);
  const res = await fetchWithRetry(url);
  const tgz = join(work, asset);
  writeFileSync(tgz, Buffer.from(await res.arrayBuffer()));

  const extractDir = join(work, asset.replace(/[^a-zA-Z0-9]/g, "_"));
  mkdirSync(extractDir, { recursive: true });
  execFileSync("tar", ["-xzf", tgz, "-C", extractDir], { stdio: "inherit" });

  const libSrc = join(extractDir, inner);
  if (!existsSync(libSrc)) {
    throw new Error(`Expected library not found in ${asset}: ${inner}`);
  }
  return libSrc;
}

// ── Android: libpdfium.so per ABI into the APK's jniLibs ─────────────────────
async function installAndroid() {
  const ABI_ASSET = {
    "arm64-v8a": "pdfium-android-arm64.tgz",
    "armeabi-v7a": "pdfium-android-arm.tgz",
    x86_64: "pdfium-android-x64.tgz",
    x86: "pdfium-android-x86.tgz",
  };
  const abis = (process.env.PDFIUM_ANDROID_ABIS || "arm64-v8a")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);

  const jniRoot = join(srcTauri, "gen", "android", "app", "src", "main", "jniLibs");
  if (!existsSync(join(srcTauri, "gen", "android"))) {
    throw new Error(
      "Android project not found (src-tauri/gen/android). Run `tauri android init` first.",
    );
  }

  console.log(`→ Android PDFium for ABIs: ${abis.join(", ")}`);
  const work = mkdtempSync(join(tmpdir(), "pdfium-android-"));
  for (const abi of abis) {
    const asset = ABI_ASSET[abi];
    if (!asset) throw new Error(`Unknown Android ABI: ${abi}`);
    const lib = await fetchLib(asset, "lib/libpdfium.so", work);
    const destDir = join(jniRoot, abi);
    mkdirSync(destDir, { recursive: true });
    const dest = join(destDir, "libpdfium.so");
    copyFileSync(lib, dest);
    console.log(`✓ ${abi}: libpdfium.so → ${dest}`);
  }
}

// ── Desktop: lib next to the crate + into resources/ ─────────────────────────
async function installDesktop() {
  const platform = process.platform; // 'win32' | 'darwin' | 'linux'
  const arch = process.arch === "arm64" ? "arm64" : "x64";

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

  console.log(`→ Platform: ${platform} ${arch}`);
  const work = mkdtempSync(join(tmpdir(), "pdfium-"));
  const libs = [];
  for (const asset of target.assets) {
    libs.push(await fetchLib(asset, target.inner, work));
  }

  let finalLib;
  if (libs.length > 1) {
    console.log("→ Merging into a universal binary with lipo ...");
    finalLib = join(work, target.out);
    execFileSync("lipo", ["-create", ...libs, "-output", finalLib], { stdio: "inherit" });
  } else {
    finalLib = libs[0];
  }

  // src-tauri/ → found by `tauri dev`; src-tauri/resources/ → bundled in builds.
  for (const dir of [srcTauri, join(srcTauri, "resources")]) {
    mkdirSync(dir, { recursive: true });
    const libDest = join(dir, target.out);
    copyFileSync(finalLib, libDest);
    console.log(`✓ Installed ${target.out} → ${libDest}`);
  }
}

async function main() {
  const isAndroid =
    process.env.TAURI_ENV_PLATFORM === "android" || process.env.PDFIUM_TARGET === "android";
  if (isAndroid) {
    await installAndroid();
  } else {
    await installDesktop();
  }
}

main().catch((err) => {
  console.error(`✗ ${err.message}`);
  console.error('  You can install PDFium manually — see README "PDFium binaries".');
  process.exit(1);
});
