# PeDeeFe logo and icons

<img src="logo/pedeefe-logo.svg" alt="PeDeeFe logo: an indigo tile with a dog-eared amber corner and a white p/d ligature, next to the word PeDeeFe" width="420">

The brand kit for **PeDeeFe** (said "pê-dê-éfe", like the letters PDF). The app, the Windows installer
and the packages use it: see [Incorporating](#incorporating).

## The mark

- **Ligature:** a lowercase **p** and **d** share one bowl. The p's stem drops on the left and the d's stem
  rises on the right, so you read "pê-dê".
- **Tile:** an indigo rounded square (`rx = 112` on 512, like the other app icons) with its top-right corner
  folded over in amber, so the tile is also a sheet of paper.
- **Wordmark:** "PeDeeFe" in Inter SemiBold, converted to outlines, so no font is needed. The capitals
  P, D and F are indigo and spell the format; the e's are ink.

| Colour | Hex | Used for |
|---|---|---|
| Indigo, top of the tile gradient | `#5B4BFF` | tile |
| Indigo, bottom of the tile gradient | `#3A2BD8` | tile, PDF label |
| Indigo, solid | `#4433E8` | wordmark capitals, the symbol on its own, `theme_color` |
| Amber | `#FFB547` | the folded corner |
| Ink | `#1A1838` | wordmark e's on light backgrounds |
| Lavender | `#B9B0FF` | wordmark capitals on dark backgrounds |
| White | `#FFFFFF` | the ligature, wordmark e's on dark backgrounds |

Clear space: keep at least a quarter of the tile's height free around the logo. Don't recolour the tile,
stretch it, or put the colour logo on busy photos. Use `pedeefe-logo-white.svg` on dark backgrounds and
`pedeefe-logo-mono.svg` where only one ink is available.

## Files

| File | What it is |
|---|---|
| `logo/pedeefe-icon.svg` | **master app icon** (512 px tile); every app, web and PNG icon is rendered from it |
| `logo/pedeefe-symbol.svg` | the ligature alone, indigo, transparent background |
| `logo/pedeefe-logo.svg` | horizontal logo (icon + wordmark) for light backgrounds |
| `logo/pedeefe-logo-white.svg` | horizontal logo for dark backgrounds |
| `logo/pedeefe-logo-mono.svg` | horizontal logo in black only (fax, stamps, engraving); recolour freely |
| `logo/pedeefe-logo-stacked.svg`, `logo/pedeefe-logo-stacked-white.svg` | icon above the wordmark |
| `logo/pedeefe-wordmark.svg`, `logo/pedeefe-wordmark-white.svg` | the word alone |
| `png/` | PNG exports: `pedeefe-logo.png` and `pedeefe-logo-white.png` (1600 px wide), `pedeefe-logo-stacked.png` (1000 px), `pedeefe-icon-1024.png` |
| `app-icon/pedeefe.ico` | Windows app icon, 16–256 px |
| `app-icon/pedeefe.icns` | macOS app icon, 16–1024 px, on Apple's grid (824 px body on 1024) |
| `app-icon/pedeefe-1024.png` | 1024 px on Apple's grid (runtime Dock icon) |
| `app-icon/hicolor/<n>x<n>/apps/pedeefe.png`, `app-icon/hicolor/scalable/apps/pedeefe.svg` | Linux hicolor theme, 16–512 px plus scalable |
| `document/pedeefe-document.svg`, `.ico`, `-256.png` | file-type icon for `.pdf` files when PeDeeFe is the default PDF app |
| `web/favicon.ico` (16, 32, 48 px), `web/favicon.svg` | browser tab icon |
| `web/apple-touch-icon.png` | 180 px, iPhone/iPad home screen (full bleed; iOS rounds the corners) |
| `web/icon-192.png`, `web/icon-512.png`, `web/icon-maskable-512.png`, `web/site.webmanifest` | Android / installed web app |
| `web/pedeefe-fullbleed.svg` | master for the full-bleed icons (no fold; the ligature stays inside the maskable safe zone) |
| `social/pedeefe-social.svg`, `.png` | 1280 × 640 card for the GitHub social preview and link previews |

## Incorporating

Where the kit is used. The app icon lives in `assets/app-icon/` too, copied under the file names the
packaging scripts expect (`printcraft.*`, and the app id `ai.storyteller.printcraft` in `hicolor/`), so
every build and package already shows the PeDeeFe icon:

- **Window, Dock and taskbar icon:** `apps/printcraft/src/main.rs` (`APP_ICON_PNG`), from `assets/app-icon/`.
- **Windows .exe and installer icon:** `apps/printcraft/build.rs` and `packaging/windows/package.ps1`, from
  `assets/app-icon/printcraft.ico`.
- **Windows file-type icon:** the installer installs `document/pedeefe-document.ico` and shows it for
  `.pdf` files opened with PeDeeFe (`packaging/windows/printcraft.wxs`).
- **About dialog and home screen:** `logo/pedeefe-icon.svg` (`crates/ui-egui/src/widgets.rs`).
- **macOS bundle, Linux and FreeBSD packages:** the packaging scripts copy `assets/app-icon/`.
- **README header:** `logo/pedeefe-logo.svg`, and `logo/pedeefe-logo-white.svg` in dark mode.
- **GitHub social preview:** upload `social/pedeefe-social.png` in the repository's Settings ▸ General.
- **Web build (not wired yet):** copy `web/` next to the page and add to its `<head>`:

  ```html
  <link rel="icon" href="favicon.ico" sizes="any">
  <link rel="icon" href="favicon.svg" type="image/svg+xml">
  <link rel="apple-touch-icon" href="apple-touch-icon.png">
  <link rel="manifest" href="site.webmanifest">
  <meta name="theme-color" content="#4433E8">
  ```

Every file is listed in `ATTRIBUTION.toml`. If you move or change one, update its entry (path and
SHA-256) and run `cargo xtask assets --write`.

## Regenerate

The SVGs are the masters. After editing one, re-render everything derived from it:

```sh
assets/brand/pedeefe/render.sh   # needs resvg and python3; writes the PNG, .ico and .icns files
cargo xtask assets               # then update the sha256 values in ATTRIBUTION.toml and run with --write
```

Licence: [LICENSE.txt](LICENSE.txt) (`MIT OR Apache-2.0`, like the repo). The wordmark is set in Inter
(OFL-1.1, `assets/fonts/OFL-Inter.txt`).
