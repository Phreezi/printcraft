# PeDeeFe app icon

<img src="printcraft.svg" alt="PeDeeFe app icon: an indigo tile with a dog-eared amber corner and a white p/d ligature" width="128">

The app icon the builds and packages use: the PeDeeFe icon from the brand kit in
[`assets/brand/pedeefe/`](../brand/pedeefe/README.md), copied here under the file names the packaging
scripts expect (`printcraft.*`, and `ai.storyteller.printcraft` in `hicolor/`, the app id). The kit's
`logo/pedeefe-icon.svg` is the master; regenerate the kit with its `render.sh`, then copy its
`app-icon/` files here (or run `packaging/icons.sh`, which renders the same files from `printcraft.svg`).

**Tile:** `viewBox="0 0 512 512"`, a rounded square with `rx=112`. Windows and Linux icons use the
full-bleed tile. macOS icons put it on Apple's grid (an 824 px body centred on a transparent 1024 px
canvas).

Licence: [LICENSE.txt](LICENSE.txt) (`MIT OR Apache-2.0`, like the repo). PeDeeFe no longer uses
PrintCraft's lion icon.

## Files

| File | What it is |
|---|---|
| `printcraft.svg` | the master vector (traced at 2048 px); every PNG, `.ico` and `.icns` is rendered from it |
| ~~`printcraft-small.svg`~~ (removed) | a lighter vector (traced at 1024 px) for places where size matters, such as this README |
| `printcraft-1024.png` | 1024 px on Apple's grid; also the runtime Dock icon on macOS |
| `printcraft.icns` | macOS icon (16–1024 px) |
| `printcraft.ico` | Windows icon (16–256 px), embedded in `printcraft.exe` by `apps/printcraft/build.rs` |
| `hicolor/<n>x<n>/apps/ai.storyteller.printcraft.png` | Linux hicolor theme, 16–512 px; the 256 px one is the runtime icon on Windows and Linux |
| `hicolor/scalable/apps/ai.storyteller.printcraft.svg` | Linux scalable icon (copy of the master) |

Where it shows: `apps/printcraft/src/main.rs` sets the window icon (Dock, taskbar, Alt-Tab, launcher) and the
Wayland app id `ai.storyteller.printcraft`; `packaging/linux/ai.storyteller.printcraft.desktop` names the
hicolor icon.

## Regenerate

```sh
packaging/icons.sh        # needs resvg and python3; iconutil (macOS) for the .icns
cargo xtask assets        # then update the sha256 values in ATTRIBUTION.toml and run with --write
```
