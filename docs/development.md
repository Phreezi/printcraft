# Developing PeDeeFe

PeDeeFe is based on [PdfCraft](https://github.com/storytold/pdfcraft); its crates keep PdfCraft's
`pdfcraft*` names.

Working instructions for agents and contributors are in `AGENTS.md` and `CLAUDE.md`. This page
collects what the desktop app reads from its environment and where it writes its diagnostics.

## Logs

The desktop app writes its `log` records to standard error and to `logs/pdfcraft.log` in its
settings folder, next to `app.ron`: Linux and FreeBSD `~/.local/share/pedeefe/logs/` (or
`$XDG_DATA_HOME/pedeefe/logs/`), macOS `~/Library/Application Support/PeDeeFe/logs/`, Windows
`%APPDATA%\PeDeeFe\data\logs\`. PeDeeFe never reads or moves the folders of an installed
PrintCraft or PdfCraft. A start from a desktop menu, Finder or the Start menu has no
terminal, so this file is what to attach to a bug report: a failed autosave, a page that would not
render, a render worker that could not start and the report of an internal error all land there.
Each launch moves the previous log to `pdfcraft.1.log` (and that one to `pdfcraft.2.log`), so the
log of a run that crashed survives the next start. The file stops growing at 16 MiB. `--version`
writes no file.

By default the app's own crates (`pdfcraft*`) log at `info` and everything else at `warn`.
`RUST_LOG` replaces that with env_logger-style directives, for example `RUST_LOG=debug`,
`RUST_LOG=warn,pdfcraft_render=trace` or `RUST_LOG=info,wgpu_core=warn`; a directive ending in `*`
covers every target starting with it (`pdfcraft*=debug`). The logger is
`apps/pdfcraft/src/logging.rs`. It never records the control-channel token or document passwords.

## One app per user

A launch while the app runs (a double-clicked PDF, Open With, a shortcut) hands its files to the
running app, which opens them as tabs in the window used last and brings it to the front, and
exits. The running app listens on a random loopback port and writes the port and a random token to
`instance.json` in the settings folder (readable by the user only); a launch that can't reach it
(the app crashed and left the file behind) replaces it and runs normally. `--new-instance`, and any
launch with options (`--control`, `--create-images`, view options), runs a separate app. The code
and the protocol are in `apps/pdfcraft/src/single_instance.rs`.

## Environment variables

| Variable | Effect |
|---|---|
| `RUST_LOG` | Log levels for standard error and the log file (see [Logs](#logs)) |
| `RUST_BACKTRACE` | `1` adds a backtrace to the report of an internal error |
| `XDG_DATA_HOME` | Linux/FreeBSD: base of the settings folder (`pedeefe/`), the log folder and crash recovery |
| `WGPU_POWER_PREF` | GPU choice; by default PeDeeFe prefers the low-power (integrated) GPU |
| `WGPU_BACKEND` | Graphics backend; by default Windows uses Direct3D 12, falling back to OpenGL |
| `CRAFT_FONTS_DIR` | Build time: a [craft-fonts](https://github.com/storytold/craft-fonts) checkout to embed (Japanese fonts) |
