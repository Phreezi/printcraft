# PeDeeFe: the fork, for agents (read this first)

This repository (`Phreezi/printcraft`) is **PeDeeFe**, a personal fork of PdfCraft
(`storytold/pdfcraft`, formerly PrintCraft). Everything in `AGENTS.md` comes from PdfCraft and
still applies to the code, **except where this file says otherwise**: this file wins for the
fork's own workflow (branches, releases, CI, branding, languages).

The owner is João (GitHub `Phreezi`). He writes in informal European Portuguese: answer in
**Portuguese from Portugal**, take the most likely reading when a request is loose, and keep going.
He uses PeDeeFe on Windows PCs; the improvement list he reads is `MELHORIAS.md` (Portuguese).

## Where the work is

- `main` of this fork is **stale** (it is PdfCraft as of 2026-10-07) and is not where PeDeeFe
  lives. Never push to it.
- The current PeDeeFe is the branch **`claude/lucid-goodall-tnwl5g`** (draft PR
  Phreezi/printcraft#3): the merge of PdfCraft 0.18.1 (upstream `main` at `5644d56c`, 2026-11-07)
  into the fork, plus fixes. The previous round is `claude/awesome-darwin-igr6t6` (0.7.1-test.20).
- New work: branch from the latest PeDeeFe branch (not from `main`), commit, push, and build a test
  release (below). Each session usually gets its own `claude/...` branch name; start it from the
  last PeDeeFe branch.
- `upstream` remote: `https://github.com/storytold/pdfcraft` (`git remote add upstream …` in a
  fresh clone).

## Releases and updates (how João gets a new version)

- `.github/workflows/test-build.yml` ("Test build (Windows)") runs **only by hand**
  (`workflow_dispatch`, on the branch to build). It builds the Windows x64 MSI and portable zip
  with `packaging/windows/package.ps1`, writes `SHA256SUMS`, and publishes a GitHub Release
  `v<workspace version>-test.<run number>` marked *latest* (e.g. `v0.18.1-test.22`). It keeps the
  newest 10 test releases.
- The installed app updates itself from those releases: `crates/ui-egui/src/platform/updates.rs`
  (UI: the prompt at every start, Preferences ▸ Updates, version compare) and
  `apps/pdfcraft/src/updates.rs` (asks `api.github.com/repos/Phreezi/printcraft/releases/latest`,
  downloads only from this repo's releases, checks `SHA256SUMS`, runs the MSI after the app
  closes, then reopens the files that were open). **Never break this chain**: asset names
  `pedeefe-<version>-windows-x64.msi`, `SHA256SUMS` in every release, the release marked latest,
  and a version higher than the installed one (semver; `0.18.1-test.N` > `0.7.1-test.N`).
- The version comes from `[workspace.package] version` in `Cargo.toml` (PdfCraft's) and reaches the
  build as `PDFCRAFT_VERSION`; `Cargo.toml` is not edited for test builds.
- Done means: the test build is green and the release is published; then tell João to update from
  the app (Preferências ▸ Atualizações or the prompt at start).
- GitHub Actions must be enabled on the fork (Settings ▸ Actions ▸ General ▸ Allow all actions).
  If dispatching says "Actions has been disabled", ask João to check that setting; it can take a
  few minutes to apply.
- The inherited PdfCraft workflows (`ci.yml`, `release.yml`, …) are not how PeDeeFe ships. Heavy
  or secret-needing ones are limited to `storytold/pdfcraft` or to manual runs. House rule 11
  (shared CI) is PdfCraft's; the fork keeps `test-build.yml` as its own job.

## Identity (keep it when merging)

- Name **PeDeeFe** everywhere people see it: `pdfcraft_engine::links::APP_NAME`,
  `crate::branded()` in ui-egui (replaces "PdfCraft"/"PrintCraft" in translated strings at display
  time, so catalog keys stay upstream's), window titles, About, home screen, `--help`/`--version`
  (`pedeefe …`), MCP server title.
- Links: `crates/engine/src/links.rs` points to this fork (issues, releases) and credits PdfCraft
  in plain text (`CREDIT`, Help ▸ "Based on PdfCraft"). **No ArtCraft marks, logos, Discord or
  community links** (PdfCraft's brand licence requires forks to remove them; `docs/brand/` is gone).
- Icons: PeDeeFe's own artwork in `assets/brand/pedeefe/`, copied over `assets/app-icon/*`
  (same file names as upstream so packaging works); `ATTRIBUTION.toml` describes them as PeDeeFe's.
- Windows installer (`packaging/windows/pdfcraft.wxs`, `installer-ui.wxs`, `package.ps1`,
  `test-msi.ps1`): PeDeeFe's own UpgradeCode, component GUIDs, `pedeefe.exe`, Program
  Files\PeDeeFe, ProgID, per-machine install, `print`/`printto` verbs. These files are the fork's:
  when merging, keep the fork's versions and port only what the build needs (2026-11: the OCR
  models became `.onnx`). Changing the UpgradeCode or install scope breaks updates.
- Settings live in the `PeDeeFe` folder (`eframe::storage_dir("PeDeeFe")`, `settings_dir()` in
  `apps/pdfcraft/src/main.rs`), JSON key `pdfcraft` in `app.ron`; never touch PdfCraft's folders.

## The fork's features (where they are)

- **Updates**: see above. PdfCraft's `craft-update` is not used (removed from ui-egui and the app).
- **Print**: PeDeeFe's own dialog `crates/ui-egui/src/workflows/print_ui.rs` (replaces upstream's
  `print_ui` + `body.rs` + `draft.rs`): A4/A3 only, Window print area (`Settings::region`),
  poster with tile grid, "Print as image" (Windows, PowerShell + System.Drawing:
  `crates/print/src/spool/windows.rs`, `crates/print/src/raster.rs`) **on by default on Windows**,
  Enter prints, page choice reset per document, prefs saved under `"print"`. Print crate additions
  in `crates/print/src/{settings,sheets,error}.rs`: `A4`, `A3`, `region`, `Tile`, `page_view`,
  `printed_size`, Fit with **no margin**, poster tiles clipped to the page. Automation `doc_print`
  defaults to A4 and takes `region`. Print as image shrinks fitted sheets (Fit, Shrink, Quick
  Print) into the printer's printable area (`ImageJob::fit_printable`, `PDFCRAFT_FIT` in the
  script); exact sizes and posters draw from the paper's corner.
- **Quit while printing**: print threads hold `spool::job_started()`; after `run_native` returns
  (windows gone) `finish_printing()` in `apps/pdfcraft/src/main.rs` waits (≤31 min) with
  `spool::wait_for_jobs`, after dropping the single-instance server. The update installer waits
  up to 32 min for the old process.
- **Quick Print** (`--print FILE…`, `--print-to PRINTER FILE…`; Explorer/Outlook verbs):
  `apps/pdfcraft/src/quick_print.rs`, `crates/ui-egui/src/workflows/quick_print.rs`; parsed in
  `apps/pdfcraft/src/startup.rs`.
- **One app per user + windows**: the fork's `apps/pdfcraft/src/single_instance.rs` (TCP loopback
  + HMAC token, `instance.json` in `%LOCALAPPDATA%\PeDeeFe`), and `crates/ui-egui/src/windows.rs`
  (tabs torn off into new windows, moved between windows; `enter_window`, `draw_window`,
  `show_other_windows`, `tidy_windows`; `view_of_mut` for results landing in another window).
  `--new-instance`/`--new-window` runs separately.
- **Viewer**: single page (not continuous) = one page per window **still scrolling through every
  page** (`DocView::paged()` is true only in full screen); Fit page against the largest page with
  a gap; Space / Shift+Space and Page Down/Up go to the next/previous page top (`flip_page`,
  `page_flip_keys` in `canvas/navigation.rs`; `space_pages` in `app/keys.rs`). Middle-button
  autoscroll latches on every platform (`canvas/autoscroll.rs`).
- **Edit text & images**: Delete removes several paragraphs in one pass
  (`Edit::DeleteTextBlocks`, `pdfcraft_edit::delete_blocks`, automation `text_delete`), Ctrl+A
  selects every box (`edit_text_keys`, `select_all_boxes` in `tools/edit_text_ui.rs`), a plain
  click opens a paragraph (`keep_on_click` in `object_ui.rs`), Esc applies typed text and selects
  its box, a second Esc leaves the tool.
  **Select area** button in the hotbar (`DocView::area_select`, `hotbar.rs`): every press on the
  page goes to `object_ui::modified_hit` as `Toggle(None)`, so a drag starting on stacked boxes
  draws a selection rectangle (non-additive unless Shift/Ctrl).
- **Panels**: the desktop app sets `left_open = false` and `pages_panel_only = true` (only the page
  thumbnails of multi-page documents open by themselves). Library defaults stay upstream's so
  upstream tests hold.
- **Tabs**: opening a file already open (same path, or same name and size) selects its tab
  (`tab_showing`); a line between unselected tabs (`chrome.rs`).
- **Language**: only English and Português (Portugal) are offered (`OFFERED`, `Lang::offered`),
  asked on first start (`language_prompt`, `ask_language_if_unset`). See below.

## Translations (pt-PT)

- PdfCraft moved its catalogs to `storytold/craft-i18n` (embedded with `CRAFT_I18N_DIR`), which
  has **no European Portuguese**. PeDeeFe keeps `crates/ui-egui/src/i18n/pt-pt.tsv` in this repo
  and `crates/ui-egui/build.rs` **always embeds it**. Builds don't need craft-i18n.
- The file has João's hand-written rows first, then rows below "Derived from pt-BR", converted by
  rule from craft-i18n's pt-BR catalog (2026-11). Correct rows in place. House style is in the
  file's header (Ficheiro, Guardar, Ecrã, Transferir, Utilizador, palavra-passe, separador…).
- New English UI strings need a pt-PT row there. After editing the file, update its `sha256` in
  `ATTRIBUTION.toml` and run `cargo xtask assets --write` then `cargo xtask assets`.

## Merging PdfCraft again (lessons from 2026-11)

1. `git fetch upstream main`, branch from the latest PeDeeFe branch, `git merge upstream/main`.
2. Upstream moves fast and restructures (in 2026-10/11 ui-egui was split into `app/`, `canvas/`,
   `tools/`, `workflows/`, `persist/`, `platform/`; tests became one binary `tests/it/`). Best
   method per conflicted file: take upstream's file, then re-apply the fork's diff
   (`git diff <old merge base> <old PeDeeFe head> -- file`) and port rejected hunks by hand.
3. Keep the fork's whole files for: print_ui, updates (ui + app), single_instance, quick_print,
   windows.rs, packaging/windows, test-build.yml, README.md, MELHORIAS.md. Prefer upstream's when
   upstream solved the same problem (window state, middle-click close, Ctrl+Tab, …).
4. Expect upstream tests that check PdfCraft behaviour PeDeeFe changes on purpose (single-page
   wheel paging, Discord, "PdfCraft" in titles, Letter as default paper, Fit margins). Adapt them
   to the fork's behaviour or remove them, and say so in the PR.
5. Gates: `cargo fmt`, scoped `cargo clippy -p <crate> --all-targets -- -D warnings`, scoped tests,
   `cargo xtask assets`, `cargo xtask layers`. In cloud sandboxes ~40 render tests fail only for
   lack of a GPU ("No adapter found"). Windows-only code can't be checked on Linux (C deps): the
   test build is the real check.
6. Then run the test build, check the release, update `MELHORIAS.md` (Portuguese), tell João.

## Known open points (2026-11-20)

- `paste_ownership::shared_xobject_original_occurrence_owns_delete` (ui-egui tests) fails after
  the 0.18.1 merge; cause not found.
- PdfCraft's browser printing (`workflows/web_print.rs`) is unused by the fork's print dialog.
- DWG/DXF viewing is postponed (see `MELHORIAS.md`).
