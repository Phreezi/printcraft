<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/pedeefe/logo/pedeefe-logo-white.svg">
    <img alt="PeDeeFe" src="assets/brand/pedeefe/logo/pedeefe-logo.svg" width="360">
  </picture>
</p>

# PeDeeFe

**An open-source PDF workbench written in Rust, with printing that works on Windows.**

PeDeeFe is a modified version of [PdfCraft](https://github.com/storytold/pdfcraft) by the
ArtCraft team. It keeps PdfCraft's engine and app and adds the changes listed below. PeDeeFe is
not made, sponsored or endorsed by the ArtCraft Team, and it carries none of the ArtCraft names or
logos (see [NOTICE](NOTICE)).

For the full tour of what the app can do (viewing, search, organizing pages, combining and
splitting, comments, forms, signatures, redaction, the CLI and the MCP server), read
[PdfCraft's README](https://github.com/storytold/pdfcraft#readme): it all applies here.

## What PeDeeFe changes

- **Printing to real printers on Windows.** The Print dialog lists the printers Windows knows and
  sends the job straight to the one you pick, with copies, collation, two-sided printing, grayscale
  and print quality. PdfCraft could only save a print-ready PDF on Windows.
- **A4 and A3 only** in the Print dialog's paper list, A4 by default.
- **Window printing**, like AutoCAD's plot window: drag a rectangle over the page and print just
  that area, exactly as drawn, on one sheet at the largest size it fits or as a poster over
  several. Hold Shift while dragging to keep the A4 or A3 sheet's proportions, so the area fills
  the sheet; drag a corner to resize it.
- **A clearer Print dialog**: each section in its own coloured panel, the sizing modes as a
  segmented control, and an Acrobat-like preview with the scale and the number of sheets above
  it and the sheet with the page's printed size below it (e.g. "A4 - 210 × 297 mm
  [1188,04 × 1680,13 mm]"). A poster previews the whole page with its tiles over it.
- **A sharp print preview**, drawn at the screen's resolution instead of from thumbnails.
- **Select and delete several boxes at once in Edit text & images**, as in Acrobat: drag a
  rectangle over the page (every text box and image it touches is selected), Shift- or
  Ctrl-click a box to add or remove it, or press Ctrl+A for every box on the page. Delete (or
  Backspace) removes them all in one step, and one Undo brings them all back. Esc leaves Edit
  text & images and keeps the text you were typing (it used to discard it).
- **The window remembers its size and position**, and whether it was maximized. It opens straight
  in place, already drawn, instead of resizing itself after it appears.
- Its own identity: its own name, logo and icons ([brand kit](assets/brand/pedeefe/README.md)); it
  installs next to an official PdfCraft instead of replacing it, keeps its own settings, and checks
  this repository for updates.

## Install a test build (Windows)

Every change pushed to this repository builds a Windows installer on GitHub Actions:

1. Open [Releases](https://github.com/Phreezi/printcraft/releases) and download the newest
   `pedeefe-…-windows-x64.msi`.
2. Run it. Windows SmartScreen may warn that the installer isn't signed: choose
   *More info ▸ Run anyway*. Each new build replaces the previous one.
3. Later, **Help ▸ Check for updates** in the app says when a newer build is out.

There is also a portable `.zip` (no installation: unzip and run `pedeefe.exe`).

## Build from source

```sh
git clone https://github.com/Phreezi/printcraft
cd pdfcraft
cargo run --release -p pdfcraft -- some.pdf     # the desktop app
cargo test --workspace                            # the tests
```

On Windows you need [Rust](https://rustup.rs) with the MSVC toolchain (the Visual Studio Build
Tools). The crates keep PdfCraft's names (`pdfcraft-*`), so changes from PdfCraft merge in
cleanly.

## Em português

O PeDeeFe é uma versão modificada do PdfCraft, com impressão a sério no Windows, papel A4/A3,
impressão por janela (como no AutoCAD) e uma pré-visualização nítida. Para testar: em
[Releases](https://github.com/Phreezi/printcraft/releases) descarrega o `.msi` mais recente e
instala-o. Fica ao lado do PdfCraft oficial, sem o substituir. Na app, **Help ▸ Check for
updates** avisa quando há uma versão nova.

## License

MIT OR Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
PdfCraft is Copyright (c) 2026 ArtCraft Team and the PdfCraft contributors; third-party
material is listed in [NOTICE](NOTICE) and [ATTRIBUTION.md](ATTRIBUTION.md).
