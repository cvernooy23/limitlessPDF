# limitlessPDF

[![CI](https://github.com/cvernooy23/limitlessPDF/actions/workflows/ci.yml/badge.svg?branch=latest)](https://github.com/cvernooy23/limitlessPDF/actions/workflows/ci.yml)
[![Build](https://github.com/cvernooy23/limitlessPDF/actions/workflows/build.yml/badge.svg?branch=latest)](https://github.com/cvernooy23/limitlessPDF/actions/workflows/build.yml)
[![Nightly](https://github.com/cvernooy23/limitlessPDF/actions/workflows/nightly.yml/badge.svg)](https://github.com/cvernooy23/limitlessPDF/actions/workflows/nightly.yml)
[![CodeQL](https://github.com/cvernooy23/limitlessPDF/actions/workflows/codeql.yml/badge.svg)](https://github.com/cvernooy23/limitlessPDF/actions/workflows/codeql.yml)
[![GitHub release](https://img.shields.io/github/v/release/cvernooy23/limitlessPDF?include_prereleases&sort=semver)](https://github.com/cvernooy23/limitlessPDF/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)
[![Rust](https://img.shields.io/badge/Rust-stable-orange?logo=rust)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-2.x-24C8D8?logo=tauri)](https://v2.tauri.app/)

A modern, glassy, cross-platform PDF **reader and editor**. Built with Tauri 2
(Rust core) + Svelte 5 (web UI), using **pdf.js** for display and **PDFium** for
editing/export.

---

## Features

**View & navigate** — fast pdf.js rendering with thumbnail sidebar, pinch/Ctrl+scroll zoom
(0.3x–4x), fit-to-width, text search (Ctrl+F), print (Ctrl+P), and drag-and-drop file open.
Opens password-protected/encrypted PDFs with a built-in password prompt.

**Annotate** — highlight, underline, strikethrough, freehand draw, sticky notes with a
collapsible comments panel, text boxes, rectangles, circles, arrows, and redaction. Select,
move, resize, or delete any annotation.

**Edit text** — click-to-edit existing text runs via PDFium, or add new text boxes anywhere on
the page.

**Fill forms** — auto-detects AcroForm fields (text, checkbox, radio, select). Save with fields
kept editable or flattened into the page content.

**Page management** — reorder pages by dragging thumbnails, rotate left/right, delete pages,
insert pages from another PDF, insert blank pages, or insert an image as a new page. Split a
document into separate files by page range.

**Signatures** — sign documents with a platform certificate (Windows CryptoAPI, macOS Keychain,
Linux NSS/OpenSSL) or create a typed-name stamp signature. A verification panel checks existing
digital signatures and reports whether the document was modified after signing.

**OCR** — detect scanned/image-only pages and extract text using Tesseract, with configurable
language and DPI.

**Security** — encrypt saved PDFs with AES-128 password protection.

**Export** — save as annotated PDF (with all annotations, text edits, and form values baked in),
or export to Plain Text, Markdown, HTML, Word (.docx), or Excel (.xlsx).

**Undo / redo** — full operation history (up to 50 snapshots) across annotations, text edits,
form fills, and page operations. Ctrl+Z / Ctrl+Shift+Z.

**Glassmorphism UI** — frameless transparent window with native OS blur (Mica on Windows,
vibrancy on macOS), glass-effect panels, and an adaptive toolbar that collapses with scroll
arrows on narrow windows.

---

## Stack

| Layer              | Tech                                               |
| ------------------ | -------------------------------------------------- |
| Shell / packaging  | Tauri 2.x                                          |
| Core               | Rust (`pdfium-render`, `lopdf`, `window-vibrancy`) |
| UI                 | Svelte 5 + TypeScript + Vite                       |
| Styling            | Tailwind CSS v4 + custom glass tokens              |
| Display engine     | pdf.js                                             |
| Edit/export engine | PDFium                                             |

## Prerequisites

- **Node** >= 22 (LTS) and **npm**
- **Rust** (stable) via [rustup](https://rustup.rs)
- Tauri OS dependencies — see the
  [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/)
  (Windows: WebView2 + MSVC build tools; macOS: Xcode CLT; Linux: WebKitGTK).

## Develop

```bash
npm install
npm run get-pdfium   # downloads the correct PDFium binary for your OS/arch
npm run app:dev      # launches the Tauri window with hot-reload
```

`get-pdfium` auto-detects your platform and CPU architecture, downloads the
matching prebuilt PDFium dynamic library from
[`bblanchon/pdfium-binaries`](https://github.com/bblanchon/pdfium-binaries),
and places it in both `src-tauri/` (for `tauri dev`) and `src-tauri/resources/`
(for bundled builds). You only need to run it once.

Frontend only (in a browser, backend disabled):

```bash
npm run dev
```

## Build

```bash
npm run app:build    # produces installers for the current OS
```

The build step runs `get-pdfium` automatically, so release builds always ship
the PDFium library next to the executable — no manual download needed for end
users.

Outputs land in `src-tauri/target/release/bundle/`:

- **Windows:** `.msi` + NSIS `.exe`
- **macOS:** `.app` / `.dmg`
- **Linux:** `.AppImage`, `.deb`, `.rpm`

## Keyboard shortcuts

| Shortcut           | Action           |
| ------------------ | ---------------- |
| Ctrl+S             | Save             |
| Ctrl+Shift+S       | Save As          |
| Ctrl+F             | Search           |
| Ctrl+P             | Print            |
| Ctrl+Z             | Undo             |
| Ctrl+Shift+Z       | Redo             |
| Ctrl+= / Ctrl+-    | Zoom in / out    |
| Ctrl+0             | Reset zoom       |
| Ctrl+scroll        | Zoom at cursor   |
| Delete / Backspace | Delete selected  |
| Escape             | Deselect / close |

## CI / Code Quality

Every push and pull request runs a comprehensive CI pipeline:

**Linting & formatting**

- `cargo fmt --check` — Rust formatting
- `cargo clippy -D warnings` — Rust lints (warnings are errors)
- `eslint` — TypeScript/Svelte linting
- `prettier --check` — code formatting for all frontend files and config
- `svelte-check` — Svelte component type checking
- `tsc --noEmit` — full TypeScript type checking

**Testing**

- `cargo test` — Rust unit tests
- `vitest` — frontend unit tests

**Security scanning**

- **CodeQL** — GitHub's semantic code analysis for JavaScript/TypeScript, integrated with GitHub Security tab
- **ASH (Automated Security Helper)** — AWS security scanner covering secrets detection (detect-secrets), dependency vulnerabilities, and static analysis; SARIF results uploaded to GitHub code scanning

**Release builds**

- Cross-platform matrix build (Windows, macOS x64/arm64, Linux) on every tagged release
- Nightly builds from the `latest` branch (dispatched daily at 06:00 UTC)
- Version automatically injected from git tags at build time

## Icons

Generated icons live in `src-tauri/icons/` (a `source.png` master is included).
To regenerate the full platform set from the master:

```bash
npm run tauri icon src-tauri/icons/source.png
```

## Project layout

```
limitlessPDF/
├── src/                       # Svelte + TS frontend
│   ├── lib/                   # UI components, pdf.js wrapper, annotation model,
│   │                          # IPC wrappers, export pipeline
│   ├── app.css                # Tailwind v4 + glass design tokens
│   ├── App.svelte             # Main application shell
│   └── main.ts
├── src-tauri/                 # Rust core
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs             # builder + window vibrancy
│   │   ├── commands.rs        # IPC surface
│   │   ├── engine/mod.rs      # PDFium binding (probe, render, edit)
│   │   ├── annotate.rs        # annotation + save pipeline
│   │   ├── content.rs         # text content editing via PDFium
│   │   ├── digsig.rs          # digital signature creation
│   │   ├── form.rs            # AcroForm read/write
│   │   ├── insert.rs          # page/image insertion
│   │   ├── ocr.rs             # OCR text extraction
│   │   └── signature.rs       # signature verification
│   ├── resources/             # PDFium binary + license (bundled into builds)
│   ├── capabilities/          # Tauri 2 permissions
│   ├── icons/
│   ├── tauri.conf.json
│   └── Cargo.toml
├── scripts/get-pdfium.mjs     # PDFium downloader
├── .github/workflows/
│   ├── ci.yml                 # lint, test, security scans
│   ├── build.yml              # release builds
│   └── nightly.yml            # nightly builds
├── LICENSE                    # MIT
├── THIRD-PARTY-NOTICES        # third-party license texts
└── package.json
```

## License

[MIT](LICENSE) — Copyright (c) 2024 Christopher Vernooy

This project uses PDFium (BSD 3-Clause / Apache 2.0), pdf.js (Apache 2.0), and
other open-source libraries. See [THIRD-PARTY-NOTICES](THIRD-PARTY-NOTICES) for
full license texts.
