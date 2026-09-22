# Changelog

All notable changes to limitlessPDF will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.0.3] - 2026-09-21

### Added

- Split PDF feature - save selected page ranges as separate files
- Redaction tool for blacking out sensitive content in documents

### Changed

- Moved redact button into the shapes toolbar group to prevent overflow on smaller windows

## [0.0.2] - 2026-09-20

### Added

- Shapes annotation tool (rectangles, circles, arrows, lines)

### Fixed

- Shell injection vulnerability in the build workflow release step

### Changed

- Set internal version number to 0.0.1 to match the release tag
- Added status badges to README and updated Node prerequisite to 22

## [0.0.1] - 2026-09-19

### Added

- PDF reader and editor with multi-document support
- OCR for scanned PDFs via Tesseract integration
- Digital signature display panel
- Underline and strikethrough annotation tools
- Highlight, freehand ink, and text box annotations
- Text search with match navigation
- Page reorder via drag-and-drop thumbnail rail
- Form field detection and filling
- Print support with per-page layout
- Dark theme

### Security

- Hardened signature parser against crafted PDFs
- ASH security scanner added to pre-commit hooks
- All GitHub Actions pinned to commit SHAs

### Infrastructure

- CI pipeline with lint, ASH, and CodeQL scanning
- Nightly build workflow
- Cross-platform build workflow (Windows, macOS universal, Linux)
- Pre-commit hooks: cargo fmt, clippy, eslint, prettier, svelte-check
- Unit tests for Rust backend and TypeScript frontend
