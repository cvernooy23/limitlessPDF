# limitlessPDF

A modern, glassy, cross-platform PDF **reader and editor**. Built with Tauri 2
(Rust core) + Svelte 5 (web UI), using **pdf.js** for display and **PDFium** for
editing/export.

---

## Features

- **View & navigate** — fast pdf.js rendering, thumbnail sidebar, page reorder / rotate / delete, zoom, text search
- **Annotate** — highlight, freehand draw, sticky notes, comments panel
- **Edit text** — click-to-edit existing text runs, add new text boxes
- **Fill forms** — detect and fill AcroForm fields (text, checkbox, radio, select)
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

- **Node** >= 20 and **npm**
- **Rust** (stable) via [rustup](https://rustup.rs)
- Tauri OS dependencies -- see the
  [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/)
  (Windows: WebView2 + MSVC build tools; macOS: Xcode CLT; Linux: WebKitGTK).

## Develop

```bash
npm install
npm run app:dev      # launches the Tauri window with hot-reload
```

Frontend only (in a browser, backend disabled):

```bash
npm run dev
```

## Build

```bash
npm run app:build    # produces installers for the current OS
```

Outputs land in `src-tauri/target/release/bundle/`:

- **Windows:** `.msi` + NSIS `.exe`
- **macOS:** `.app` / `.dmg`
- **Linux:** `.AppImage`, `.deb`, `.rpm`

## Icons

Generated icons live in `src-tauri/icons/` (a `source.png` master is included).
To regenerate the full platform set from the master:

```bash
npm run tauri icon src-tauri/icons/source.png
```

## PDFium binaries

`pdfium-render` loads the PDFium dynamic library **at runtime** -- it is _not_
linked at build time, so the project compiles without it. The status badge in
the app reads "PDFium not found" until you provide the library.

**Easiest -- run the helper script** (auto-detects your OS + CPU, downloads the
right binary, and places it in `src-tauri/`):

```bash
npm run get-pdfium
```

Then start the app (`npm run app:dev`) and the badge should read "PDFium ready".

The script writes the library to two places: `src-tauri/` (used by `tauri dev`)
and `src-tauri/resources/` (bundled into installers). **`npm run app:build`
runs this automatically**, so release builds ship PDFium next to the executable
and the engine finds it via the executable/resource directory at runtime -- no
manual step for end users.

**Manual alternative:** download a prebuilt PDFium for your platform from the
[`pdfium-binaries` releases](https://github.com/bblanchon/pdfium-binaries/releases)
and place the library (`pdfium.dll` / `libpdfium.dylib` / `libpdfium.so`) next to
the executable, or anywhere on the system library path. The release/packaging
step bundles the correct binary per platform.

## Project layout

```
limitlessPDF/
+-- src/                   # Svelte + TS frontend
|  +-- lib/                # UI components, pdf.js wrapper, annotation model,
|  |                       # IPC wrappers, export pipeline
|  +-- app.css             # Tailwind v4 + glass design tokens
|  +-- App.svelte          # Main application shell
|  +-- main.ts
+-- src-tauri/             # Rust core
|  +-- src/
|  |  +-- main.rs
|  |  +-- lib.rs           # builder + window vibrancy
|  |  +-- commands.rs      # IPC surface
|  |  +-- engine/mod.rs    # PDFium binding (probe, render, edit)
|  |  +-- annotate.rs      # annotation + save pipeline
|  |  +-- content.rs       # text content editing via PDFium
|  |  +-- form.rs          # AcroForm read/write
|  +-- capabilities/       # Tauri 2 permissions
|  +-- icons/
|  +-- tauri.conf.json
|  +-- Cargo.toml
+-- scripts/get-pdfium.mjs # PDFium downloader
+-- .github/workflows/build.yml
+-- package.json
```

## License

MIT
