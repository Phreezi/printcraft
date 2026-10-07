# PeDeeFe

**An open-source PDF workbench written in Rust, with printing that works on Windows.**

PeDeeFe is a modified version of [PrintCraft](https://github.com/storytold/printcraft) by the
ArtCraft team. It keeps PrintCraft's engine and app and adds the changes listed below. PeDeeFe is
not made, sponsored or endorsed by the ArtCraft Team, and it carries none of the ArtCraft names or
logos (see [NOTICE](NOTICE)).

For the full tour of what the app can do (viewing, search, organizing pages, combining and
splitting, comments, forms, signatures, redaction, the CLI and the MCP server), read
[PrintCraft's README](https://github.com/storytold/printcraft#readme): it all applies here.

## What PeDeeFe changes

- **Printing to real printers on Windows.** The Print dialog lists the printers Windows knows and
  sends the job straight to the one you pick, with copies, collation, two-sided printing, grayscale
  and print quality. PrintCraft could only save a print-ready PDF on Windows.
- **A4 and A3 only** in the Print dialog's paper list, A4 by default.
- **Window printing**, like AutoCAD's plot window: drag a rectangle over the page and print just
  that area. Locked to the sheet's proportions, the area fills the A4 or A3 sheet at the largest
  size it can. Unlocked, pick any area and print it on one sheet or as a poster over several.
- **A sharp print preview**, drawn at the screen's resolution instead of from thumbnails.
- **The window remembers its size and position**, and whether it was maximized.
- Its own identity: it installs next to an official PrintCraft instead of replacing it, keeps its
  own settings, and checks this repository for updates.

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
cd printcraft
cargo run --release -p printcraft -- some.pdf     # the desktop app
cargo test --workspace                            # the tests
```

On Windows you need [Rust](https://rustup.rs) with the MSVC toolchain (the Visual Studio Build
Tools). The crates keep PrintCraft's names (`printcraft-*`), so changes from PrintCraft merge in
cleanly.

## Em português

O PeDeeFe é uma versão modificada do PrintCraft, com impressão a sério no Windows, papel A4/A3,
impressão por janela (como no AutoCAD) e uma pré-visualização nítida. Para testar: em
[Releases](https://github.com/Phreezi/printcraft/releases) descarrega o `.msi` mais recente e
instala-o. Fica ao lado do PrintCraft oficial, sem o substituir. Na app, **Help ▸ Check for
updates** avisa quando há uma versão nova.

## License

MIT OR Apache-2.0, at your option ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
PrintCraft is Copyright (c) 2026 ArtCraft Team and the PrintCraft contributors; third-party
material is listed in [NOTICE](NOTICE) and [ATTRIBUTION.md](ATTRIBUTION.md).
