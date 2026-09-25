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

- **View & navigate** — fast pdf.js rendering, thumbnail sidebar, page reorder / rotate / delete, zoom, text search
- **Annotate** — highlight, freehand draw, sticky notes, comments panel
- **Edit text** — click-to-edit existing text runs, add new text boxes
- **Fill forms** — detect and fill AcroForm fields (text, checkbox, radio, select)
- **Insert** — add pages from other PDFs or insert images as new pages
- **Digital signatures** — sign documents with hand-drawn or uploaded signatures, verify existing signatures
- **OCR** — extract text from scanned/image-based PDFs
- **Undo / redo** — full operation history across all editing actions
- **Merge & assemble** — open multiple PDFs and combine pages
- **Export** — save as annotated PDF with optional AES-128 encryption; export to TXT, Markdown, HTML, DOCX, XLSX
- **Glassmorphism UI** — frameless transparent window with native OS blur (Mica on Windows, vibrancy on macOS)

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
│   │   ├── digsig.rs          # digital signature verification
│   │   ├── form.rs            # AcroForm read/write
│   │   ├── insert.rs          # page/image insertion
│   │   ├── ocr.rs             # OCR text extraction
│   │   └── signature.rs       # signature placement
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
