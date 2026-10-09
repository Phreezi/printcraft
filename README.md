<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/pedeefe/logo/pedeefe-logo-white.svg">
    <img alt="PeDeeFe" src="assets/brand/pedeefe/logo/pedeefe-logo.svg" width="360">
  </picture>
</p>

# PeDeeFe

**An open-source PDF workbench written in Rust, with printing that works on Windows.**

PeDeeFe is based on [PdfCraft](https://github.com/storytold/pdfcraft) (formerly PrintCraft) by
the ArtCraft team: a modified version that keeps PdfCraft's engine and app and adds the changes
listed below. For the original, go to [storytold/pdfcraft](https://github.com/storytold/pdfcraft).
PeDeeFe is not made, sponsored or endorsed by the ArtCraft Team, and it carries none of the
ArtCraft names or logos (see [NOTICE](NOTICE)).

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
- **Quick Print from Outlook and Explorer**: PeDeeFe answers Windows' *Print* command for PDF
  files, so Outlook's *Quick Print* on a PDF attachment prints it straight away, with no window
  ([below](#print-from-outlook-and-explorer)).
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

## Print from Outlook and Explorer

The installer registers PeDeeFe's **print** and **printto** commands for PDF files. They print
with the default settings, show no window and close when the job has reached the printer:

- every page, **Fit** (the page fills the sheet, keeping its proportions; an A4 page prints at
  100 % on A4), **automatic orientation** (each sheet turns to its page), one copy, one-sided, in
  colour;
- on **A4**, or on **A3** when a page is larger than A4 (by more than 5 %: a US Letter page still
  prints on A4, an A3 or larger drawing on A3);
- at the **print quality** last chosen in the Print dialog (Standard, 300 dpi, until then);
- on Windows' **default printer** (*print*), or on the printer named by the caller (*printto*).

To use it from **Outlook**, PeDeeFe must be the app that opens PDF files: in Windows **Settings ▸
Apps ▸ Default apps**, set *.pdf* to PeDeeFe (or right-click a PDF ▸ *Open with* ▸ *Choose another
app* ▸ PeDeeFe, *Always*). Then right-click a PDF attachment in Outlook (in the message, or the
attachment list) and choose **Quick Print**. In Explorer, right-click a PDF ▸ *Show more options*
(Windows 11) ▸ **Print** does the same.

The same from a command prompt (also with the portable `.zip`):

```bat
pedeefe.exe --print "C:\Docs\invoice.pdf"
pedeefe.exe --print-to "EPSON ET-16650 Series" "C:\Docs\drawing.pdf"
```

A file that can't be printed (unreadable, password-protected, printing not allowed, the printer
unavailable) is reported in a message box and logged in `logs\quick-print.log` in PeDeeFe's
settings folder (`%APPDATA%\PeDeeFe\data`); the app never crashes over it. To choose other
settings (pages, paper, two-sided, Poster, a window of the page), open the PDF and use
**File ▸ Print**.

## Interface language

PeDeeFe speaks **English** and **European Portuguese** (Português de Portugal). The first time it
starts it asks, in both languages, "Choose your language / Escolha o idioma"; the choice applies at
once and is saved. Change it later in **Menu → Edit → Preferences… → Interface language**
(Command-comma on macOS, Ctrl-comma elsewhere, also with no document open; see
[docs/localization.md](docs/localization.md)). The Portuguese catalog covers the whole interface:
menus, toolbars, panels, every tool and dialog (the Print dialog included), edit mode, notices,
Preferences, About and the home screen. Command search accepts the translated label, the English
label and the stable command id; filenames, PDF contents, author names, custom action names and
error details from the engine or the operating system keep their own text. PdfCraft's other
catalogs (Japanese, Chinese, Czech, Brazilian Portuguese, Spanish) stay in the source, so merges
stay simple, but aren't offered.

## Build from source

```sh
git clone https://github.com/Phreezi/printcraft
cd printcraft
cargo run --release -p pdfcraft -- some.pdf     # the desktop app
cargo test --workspace                            # the tests
```

On Windows you need [Rust](https://rustup.rs) with the MSVC toolchain (the Visual Studio Build
Tools). The crates keep PdfCraft's names (`pdfcraft-*`), so changes from PdfCraft merge in
cleanly.

Japanese fonts come from [craft-fonts](https://github.com/storytold/craft-fonts), an optional build
input that every test build includes. To build with them (Japanese interface text, and Japanese text
in edited PDFs):

```sh
git clone https://github.com/storytold/craft-fonts ../craft-fonts
CRAFT_FONTS_DIR=../craft-fonts cargo run --release -p pdfcraft -- some.pdf
```

Logs, environment variables and other development notes are in [docs/development.md](docs/development.md).

## Em português

O PeDeeFe é uma versão modificada do [PdfCraft](https://github.com/storytold/pdfcraft) (antes
PrintCraft), da equipa ArtCraft, com impressão a sério no Windows, papel A4/A3,
impressão por janela (como no AutoCAD) e uma pré-visualização nítida. Para testar: em
[Releases](https://github.com/Phreezi/printcraft/releases) descarrega o `.msi` mais recente e
instala-o. Fica ao lado do PdfCraft oficial, sem o substituir. Na app, **Ajuda ▸ Procurar
atualizações** avisa quando há uma versão nova.

A **Impressão Rápida** do Outlook também funciona: com o PeDeeFe como aplicação predefinida para
PDF (Definições ▸ Aplicações ▸ Aplicações predefinidas), clica com o botão direito num anexo PDF
e escolhe **Impressão Rápida**. Imprime na impressora predefinida, em Ajustar, A4 (ou A3 se a
página for maior), orientação automática e com a qualidade usada da última vez, sem abrir janela.

Na primeira vez que abre, o PeDeeFe pergunta o idioma (English ou Português de Portugal) e
guarda a escolha. Toda a interface está em português de Portugal, incluindo a janela Imprimir.
Para mudar mais tarde: **Menu ▸ Editar ▸ Preferências… ▸ Idioma da interface**.

## License

MIT OR Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
PdfCraft is Copyright (c) 2026 ArtCraft Team and the PdfCraft contributors; third-party
material is listed in [NOTICE](NOTICE) and [ATTRIBUTION.md](ATTRIBUTION.md).
