//! Updates (issue #28): ask for the latest release, and on Windows download, check and install
//! it from the app. PeDeeFe asks its own repository ([`RELEASES_PAGE`]), where every pushed
//! change publishes a Windows test build (`.github/workflows/test-build.yml`).
//!
//! The desktop app supplies how to ask ([`PdfCraftApp::update_source`]) and, on Windows, how to
//! download and install ([`PdfCraftApp::update_installer`]), so this crate has no network or
//! process code. Without a source (the web build, tests) checking opens the releases page;
//! without an installer (macOS, Linux, the web) a newer release is offered on its release page.
//!
//! - **Help ▸ Check for updates** and **Preferences ▸ Updates** check when asked.
//! - **Check for updates automatically** (on by default, remembered): the desktop app calls
//!   [`PdfCraftApp::start_automatic_update_check`] at start, which asks at most once a day
//!   ([`check_due`]), in the background. It only tells the user (a notice with a button); it
//!   never downloads or installs anything on its own.
//! - **Download and install**: the installer is downloaded with a progress bar and checked
//!   against the release's checksums (the app crate does that). Then PeDeeFe closes through the
//!   normal close path (unsaved changes are asked about first; cancelling that cancels the
//!   install), starts the installer and opens again once it has finished.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use egui::{Align, Layout};

use crate::{Dialog, PdfCraftApp, theme, widgets};

/// The version people see and updates compare with. Test builds set `PDFCRAFT_VERSION` when
/// they compile (`0.3.0-test.12`, see `.github/workflows/test-build.yml`): the workspace version
/// itself stays plain, because Cargo won't match a pre-release against the crates' `^0.3.0`.
pub const APP_VERSION: &str = match option_env!("PDFCRAFT_VERSION") {
    Some(v) if !v.is_empty() => v,
    _ => env!("CARGO_PKG_VERSION"),
};

/// Where every PeDeeFe build is listed (this fork's releases, not PdfCraft's).
pub const RELEASES_PAGE: &str = pdfcraft_engine::links::RELEASES;

/// The automatic check asks at most this often (seconds).
pub const CHECK_INTERVAL: u64 = 24 * 60 * 60;

/// The latest published release.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Release {
    /// Its version tag, such as `v0.2.0`.
    pub version: String,
    /// Its page on [`RELEASES_PAGE`], where the downloads are.
    pub url: String,
    /// The installer for this computer, when there is one (Windows only; the app crate picks it).
    pub installer: Option<Asset>,
    /// The release's checksum list (`SHA256SUMS`), which the installer is verified against.
    pub checksums: Option<Asset>,
}

/// One downloadable file of a release.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Asset {
    /// Its file name, such as `pedeefe-0.3.0-test.41-windows-x64.msi`.
    pub name: String,
    /// Where it is downloaded from (always under [`RELEASES_PAGE`]`/download/`).
    pub url: String,
    /// Its size in bytes as the release lists it (0 when unknown).
    pub size: u64,
}

/// Asks for the latest release (blocking; it runs on its own thread).
pub type UpdateSource = Arc<dyn Fn() -> Result<Release, String> + Send + Sync>;

/// Why an update couldn't be downloaded or installed. The UI words each one plainly
/// ([`UpdateError::message`]); the technical detail, when there is one, is shown below it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateError {
    /// GitHub couldn't be reached, or the download broke off.
    Network(String),
    /// The release has no installer for this computer.
    NoInstaller,
    /// The release has no checksum for the installer, so it can't be verified.
    NoChecksum,
    /// The downloaded file's SHA-256 doesn't match the release's checksum.
    Mismatch,
    /// The download is bigger than any installer should be.
    TooLarge,
    /// The download address isn't one of this repository's release files.
    NotAllowed,
    /// The file couldn't be written (disk full, no permission).
    Disk(String),
    /// The installer couldn't be started.
    Launch(String),
    /// The user cancelled the download.
    Cancelled,
}

impl UpdateError {
    /// The message people see, in the current language.
    pub fn message(&self) -> String {
        crate::branded(match self {
            Self::Network(_) => tl!("The download didn't finish. Check that you're connected to the internet and try again."),
            Self::NoInstaller => tl!("This release has no installer for this computer. Download it from its release page."),
            Self::NoChecksum => tl!("The download can't be verified: the release has no checksum for it. Nothing was installed."),
            Self::Mismatch => tl!("The downloaded file is damaged or was changed: its checksum doesn't match. Nothing was installed."),
            Self::TooLarge => tl!("The download is much larger than expected. Nothing was installed."),
            Self::NotAllowed => tl!("The download isn't one of PdfCraft's release files on GitHub. Nothing was installed."),
            Self::Disk(_) => tl!("The update couldn't be saved on this computer. Check that there is free disk space."),
            Self::Launch(_) => tl!("The installer couldn't be started."),
            Self::Cancelled => tl!("The download was cancelled."),
        })
    }

    /// The technical detail, for the curious and for bug reports.
    pub fn detail(&self) -> Option<&str> {
        match self {
            Self::Network(d) | Self::Disk(d) | Self::Launch(d) => Some(d.as_str()).filter(|d| !d.is_empty()),
            _ => None,
        }
    }
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(d) => write!(f, "download failed: {d}"),
            Self::NoInstaller => f.write_str("the release has no installer for this computer"),
            Self::NoChecksum => f.write_str("the release has no checksum for the installer"),
            Self::Mismatch => f.write_str("the installer's SHA-256 doesn't match SHA256SUMS"),
            Self::TooLarge => f.write_str("the download is larger than the size limit"),
            Self::NotAllowed => f.write_str("the download address is outside the release files"),
            Self::Disk(d) => write!(f, "couldn't write the download: {d}"),
            Self::Launch(d) => write!(f, "couldn't start the installer: {d}"),
            Self::Cancelled => f.write_str("cancelled"),
        }
    }
}

/// How far a download is, shared between the download thread and the UI.
#[derive(Debug, Default)]
pub struct Progress {
    done: AtomicU64,
    total: AtomicU64,
    cancel: AtomicBool,
}

impl Progress {
    /// Record `done` bytes of `total` (`None` when unknown).
    pub fn set(&self, done: u64, total: Option<u64>) {
        self.done.store(done, Ordering::Relaxed);
        self.total.store(total.unwrap_or(0), Ordering::Relaxed);
    }

    /// Bytes downloaded so far.
    pub fn done(&self) -> u64 {
        self.done.load(Ordering::Relaxed)
    }

    /// The download's size, when known.
    pub fn total(&self) -> Option<u64> {
        Some(self.total.load(Ordering::Relaxed)).filter(|t| *t > 0)
    }

    /// The finished fraction (0–1), when the size is known.
    pub fn fraction(&self) -> Option<f32> {
        self.total().map(|t| (self.done() as f64 / t as f64).clamp(0.0, 1.0) as f32)
    }

    /// Ask the download to stop.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// The user asked the download to stop.
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Downloads and verifies a release's installer (blocking; it runs on its own thread) and returns
/// where the verified file is.
pub type UpdateDownload = Arc<dyn Fn(&Release, &Progress) -> Result<PathBuf, UpdateError> + Send + Sync>;
/// Starts the downloaded installer so that it runs once this process has exited.
pub type UpdateLaunch = Arc<dyn Fn(&Path) -> Result<(), UpdateError> + Send + Sync>;

/// How the desktop app downloads and installs updates (Windows).
#[derive(Clone)]
pub struct UpdateInstaller {
    pub download: UpdateDownload,
    pub launch: UpdateLaunch,
}

/// Whether the automatic check is due: it is on, and the last one was at least
/// [`CHECK_INTERVAL`] ago (or never, or the clock has gone back since). Times are Unix seconds.
pub fn check_due(enabled: bool, last: u64, now: u64) -> bool {
    enabled && (last == 0 || now < last || now - last >= CHECK_INTERVAL)
}

/// Whether release `latest` (a tag such as `v0.2.0`) is newer than version `current` (`0.1.1`).
///
/// Versions compare by semver precedence: a pre-release comes before its release
/// (`0.2.1-test.7` < `0.2.1`), and pre-releases compare identifier by identifier, numbers
/// numerically (`0.2.1-test.9` < `0.2.1-test.10`), so each test build is newer than the last.
/// Build metadata (`+…`) is ignored; a version that doesn't parse is never newer.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse(latest), parse(current)) {
        (Some(l), Some(c)) => l.0.cmp(&c.0).then_with(|| compare_pre(&l.1, &c.1)) == std::cmp::Ordering::Greater,
        _ => false,
    }
}

/// One dot-separated pre-release identifier.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Ident {
    // Declared first: numeric identifiers have lower precedence than alphanumeric ones.
    Num(u64),
    Alpha(String),
}

fn parse(v: &str) -> Option<((u64, u64, u64), Vec<Ident>)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let v = v.split('+').next()?;
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (v, None),
    };
    let mut parts = core.split('.');
    let mut next = |required: bool| match parts.next() {
        Some(p) => p.parse::<u64>().ok(),
        None if required => None,
        None => Some(0),
    };
    let version = (next(true)?, next(false)?, next(false)?);
    if parts.next().is_some() {
        return None;
    }
    let pre = match pre {
        None => Vec::new(),
        Some(p) => p
            .split('.')
            .map(|id| match id.parse::<u64>() {
                Ok(n) => Some(Ident::Num(n)),
                Err(_) if !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') => {
                    Some(Ident::Alpha(id.to_string()))
                }
                Err(_) => None,
            })
            .collect::<Option<Vec<_>>>()?,
    };
    Some((version, pre))
}

/// Semver pre-release precedence; no pre-release (a release) is greatest.
fn compare_pre(a: &[Ident], b: &[Ident]) -> std::cmp::Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        (false, false) => a.cmp(b),
    }
}

/// The version without its `v`.
fn plain(version: &str) -> &str {
    version.trim_start_matches(['v', 'V'])
}

/// Where a check is.
#[derive(Default)]
pub(crate) enum Check {
    #[default]
    Idle,
    /// Asking; `quiet` for the automatic check, which only raises a notice. (The web build has no
    /// threads: it opens the releases page instead.)
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Running {
        rx: std::sync::mpsc::Receiver<Result<Release, String>>,
        quiet: bool,
    },
    Done(Result<Release, String>),
}

/// Where a download is.
#[derive(Default)]
pub(crate) enum Fetch {
    #[default]
    Idle,
    /// Downloading `version`; no receiver for the screenshot state, which never finishes.
    Running {
        version: String,
        progress: Arc<Progress>,
        rx: Option<std::sync::mpsc::Receiver<Result<PathBuf, UpdateError>>>,
    },
    /// Downloaded and verified, waiting to be installed.
    Ready {
        version: String,
        path: PathBuf,
    },
    Failed(UpdateError),
}

pub(crate) struct Updates {
    pub(crate) check: Check,
    /// The Updates dialog is showing.
    pub(crate) open: bool,
    /// Check for updates automatically (Preferences; remembered).
    pub(crate) auto: bool,
    /// When the automatic check last asked (Unix seconds; 0 never).
    pub(crate) last_check: u64,
    /// The automatic check found this newer release: a notice offers it until dismissed.
    pub(crate) notice: Option<Release>,
    pub(crate) fetch: Fetch,
    /// Install this downloaded installer once the app is allowed to close.
    pub(crate) install_on_quit: Option<PathBuf>,
    /// Scroll Preferences to the Updates section in the next frame.
    pub(crate) reveal: bool,
}

impl Default for Updates {
    fn default() -> Self {
        Self { check: Check::Idle, open: false, auto: true, last_check: 0, notice: None, fetch: Fetch::Idle, install_on_quit: None, reveal: false }
    }
}

impl Updates {
    /// The settings kept between runs.
    pub(crate) fn prefs(&self) -> serde_json::Value {
        serde_json::json!({ "auto": self.auto, "last_check": self.last_check })
    }

    /// Restore [`Self::prefs`]; anything malformed keeps the default.
    pub(crate) fn restore_prefs(&mut self, v: &serde_json::Value) {
        if let Some(auto) = v["auto"].as_bool() {
            self.auto = auto;
        }
        if let Some(last) = v["last_check"].as_u64() {
            self.last_check = last;
        }
    }
}

/// What the user asked for in the update status.
enum Action {
    Check,
    Download(Release),
    OpenPage(String),
    Cancel,
    Install,
}

impl PdfCraftApp {
    /// Help ▸ Check for updates: ask for the latest release and show the outcome.
    pub fn check_for_updates(&mut self) {
        self.start_check(true, false);
    }

    /// The automatic check at start (the desktop app calls this once, with the time in Unix
    /// seconds): asks in the background when [`check_due`], and only raises a notice when a newer
    /// release exists. Returns whether it asked.
    pub fn start_automatic_update_check(&mut self, now: u64) -> bool {
        if self.update_source.is_none() || cfg!(target_arch = "wasm32") || !check_due(self.updates.auto, self.updates.last_check, now) {
            return false;
        }
        self.updates.last_check = now;
        self.save_requested = true;
        self.start_check(false, true)
    }

    /// Whether the automatic check is on (Preferences ▸ Updates).
    pub fn auto_update_check(&self) -> bool {
        self.updates.auto
    }

    /// The newer release the automatic check found and offers in its notice.
    pub fn update_notice(&self) -> Option<&Release> {
        self.updates.notice.as_ref()
    }

    /// Start asking; `show` opens the Updates dialog. Returns whether a check runs.
    fn start_check(&mut self, show: bool, quiet: bool) -> bool {
        let Some(source) = self.update_source.clone() else {
            if !quiet {
                self.open_url(RELEASES_PAGE);
            }
            return false;
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.updates.open |= show;
            if let Check::Running { quiet: q, .. } = &mut self.updates.check {
                // Already asking: a request from the user makes it loud.
                *q &= quiet;
                return true;
            }
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = self.ctx.clone();
            std::thread::spawn(move || {
                // The receiver may be gone (the app quit): nothing to report to then.
                let _ = tx.send(source());
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
            });
            self.updates.check = Check::Running { rx, quiet };
            true
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (source, show);
            if !quiet {
                self.open_url(RELEASES_PAGE);
            }
            false
        }
    }

    /// Download and verify `release`'s installer, then install it. Without an installer (other
    /// systems, a release without one) its release page opens instead.
    fn start_download(&mut self, release: Release) {
        let installer = self.update_installer.clone().filter(|_| release.installer.is_some() && !cfg!(target_arch = "wasm32"));
        let Some(installer) = installer else {
            self.open_url(&release.url);
            return;
        };
        if matches!(self.updates.fetch, Fetch::Running { .. }) {
            return;
        }
        self.updates.notice = None;
        let progress = Arc::new(Progress::default());
        let (tx, rx) = std::sync::mpsc::channel();
        let version = release.version.clone();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let ctx = self.ctx.clone();
            let shared = progress.clone();
            std::thread::spawn(move || {
                // Repaint while it runs, so the progress bar moves.
                let done = Arc::new(AtomicBool::new(false));
                if let Some(ctx) = ctx.clone() {
                    let done = done.clone();
                    std::thread::spawn(move || {
                        while !done.load(Ordering::Relaxed) {
                            ctx.request_repaint();
                            std::thread::sleep(std::time::Duration::from_millis(150));
                        }
                    });
                }
                let result = (installer.download)(&release, &shared);
                done.store(true, Ordering::Relaxed);
                let _ = tx.send(result);
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
            });
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (installer, tx);
        self.updates.fetch = Fetch::Running { version, progress, rx: Some(rx) };
    }

    /// Close (asking about unsaved changes first) and install the downloaded update.
    pub fn install_downloaded_update(&mut self) {
        let Fetch::Ready { path, .. } = &self.updates.fetch else { return };
        if self.update_installer.is_none() {
            return;
        }
        self.updates.install_on_quit = Some(path.clone());
        self.updates.open = false;
        if self.dialog == Some(Dialog::Preferences) {
            self.dialog = None;
        }
        self.windows.quitting = true;
        if let Some(ctx) = &self.ctx {
            ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
        }
    }

    /// The app is about to close: start the pending installer, if any. `false` when it couldn't
    /// start, in which case the close is cancelled and the error shown.
    pub(crate) fn launch_pending_update(&mut self, ctx: &egui::Context) -> bool {
        let Some(path) = self.updates.install_on_quit.take() else { return true };
        let Some(installer) = self.update_installer.clone() else { return true };
        match (installer.launch)(&path) {
            Ok(()) => {
                log::info!("starting the update installer {} after exit", path.display());
                true
            }
            Err(e) => {
                log::warn!("{e}");
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.allow_quit = false;
                self.windows.quitting = false;
                self.updates.fetch = Fetch::Failed(e);
                self.updates.open = true;
                false
            }
        }
    }

    /// The close that would have installed the update was cancelled (at the unsaved-changes
    /// question): the update stays downloaded, ready to install.
    pub(crate) fn cancel_pending_update(&mut self) {
        if self.updates.install_on_quit.take().is_some() {
            self.notify(crate::branded(tl!("The update wasn't installed. Install it from Preferences ▸ Updates when you're ready.")));
        }
    }

    /// Pick up a finished check or download (each frame).
    pub(crate) fn poll_updates(&mut self) {
        if let Check::Running { rx, quiet } = &self.updates.check {
            let quiet = *quiet;
            let result = match rx.try_recv() {
                Ok(r) => Some(r),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("the update check stopped unexpectedly".into())),
            };
            if let Some(result) = result {
                if let Ok(r) = &result
                    && quiet
                    && !self.updates.open
                    && is_newer(&r.version, APP_VERSION)
                {
                    self.updates.notice = Some(r.clone());
                }
                self.updates.check = Check::Done(result);
            }
        }
        if let Fetch::Running { rx: Some(rx), version, .. } = &self.updates.fetch {
            let result = match rx.try_recv() {
                Ok(r) => r,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Err(UpdateError::Network("the download stopped unexpectedly".into())),
            };
            let version = version.clone();
            match result {
                Ok(path) => {
                    self.updates.fetch = Fetch::Ready { version, path };
                    // The user asked to download *and install*.
                    self.install_downloaded_update();
                }
                Err(UpdateError::Cancelled) => self.updates.fetch = Fetch::Idle,
                Err(e) => {
                    log::warn!("update {version}: {e}");
                    self.updates.fetch = Fetch::Failed(e);
                }
            }
        }
    }

    /// Put the Updates status in a fixed state, for screenshots and the control channel
    /// (`--update-state available`).
    pub(crate) fn set_update_state(&mut self, value: &str) -> Result<(), String> {
        // The next patch version, so it is always newer.
        let ((major, minor, patch), _) = parse(APP_VERSION).unwrap_or(((0, 0, 0), Vec::new()));
        let number = format!("{major}.{minor}.{}", patch.saturating_add(1));
        let version = format!("v{number}");
        let msi = format!("pedeefe-{number}-windows-x64.msi");
        let release = || Release {
            version: version.clone(),
            url: format!("{RELEASES_PAGE}/tag/{version}"),
            installer: Some(Asset { name: msi.clone(), url: format!("{RELEASES_PAGE}/download/{version}/{msi}"), size: 40 << 20 }),
            checksums: None,
        };
        if self.update_installer.is_none() {
            // Shows the Windows buttons; nothing behind them downloads or installs.
            let unavailable = || UpdateError::Launch("not available in this state".into());
            self.update_installer =
                Some(UpdateInstaller { download: Arc::new(move |_, _| Err(unavailable())), launch: Arc::new(move |_| Err(unavailable())) });
        }
        self.updates.fetch = Fetch::Idle;
        self.updates.reveal = true;
        self.updates.check = match value {
            "idle" => Check::Idle,
            "up-to-date" => Check::Done(Ok(Release { version: format!("v{APP_VERSION}"), ..release() })),
            "available" | "downloading" | "ready" | "failed" => Check::Done(Ok(release())),
            "error" => Check::Done(Err("couldn't reach GitHub (timed out)".into())),
            _ => return Err("update-state must be idle, up-to-date, available, downloading, ready, failed or error".into()),
        };
        match value {
            "downloading" => {
                let progress = Arc::new(Progress::default());
                progress.set(19 << 20, Some(40 << 20));
                self.updates.fetch = Fetch::Running { version, progress, rx: None };
            }
            "ready" => self.updates.fetch = Fetch::Ready { version, path: PathBuf::from(msi) },
            "failed" => self.updates.fetch = Fetch::Failed(UpdateError::Mismatch),
            _ => {}
        }
        Ok(())
    }
}

/// The update status (shared by the Updates dialog and Preferences ▸ Updates): what is known,
/// with the button that fits.
fn status(ui: &mut egui::Ui, app: &PdfCraftApp, t: &theme::Tokens) -> Option<Action> {
    let mut action = None;
    let current = APP_VERSION;
    let muted = |s: String| egui::RichText::new(s).color(t.text_muted);
    match &app.updates.fetch {
        Fetch::Running { version, progress, .. } => {
            ui.label(egui::RichText::new(crate::i18n::fmt(&crate::branded(tl!("Downloading PdfCraft {v}…")), &[("v", plain(version))])).strong());
            let size = |n: u64| crate::panels::human_size(usize::try_from(n).unwrap_or(usize::MAX));
            let text = match progress.total() {
                Some(total) => crate::i18n::fmt(tl!("{done} of {total}"), &[("done", &size(progress.done())), ("total", &size(total))]),
                None => size(progress.done()),
            };
            ui.horizontal(|ui| {
                let bar = egui::ProgressBar::new(progress.fraction().unwrap_or(0.0)).text(text).desired_width(ui.available_width() - 110.0);
                ui.add(if progress.total().is_none() { bar.animate(true) } else { bar });
                if widgets::pill_button(ui, tl!("Cancel"), false).clicked() {
                    action = Some(Action::Cancel);
                }
            });
            ui.label(muted(tl!("The file is checked against the release's checksum before anything is installed.").to_string()).small());
            return action;
        }
        Fetch::Ready { version, .. } => {
            ui.label(
                egui::RichText::new(crate::i18n::fmt(&crate::branded(tl!("PdfCraft {v} is downloaded and verified.")), &[("v", plain(version))]))
                    .strong(),
            );
            ui.label(muted(crate::branded(tl!("PdfCraft will close (asking about unsaved changes first), install it and open again."))));
            ui.horizontal(|ui| {
                if widgets::pill_button(ui, tl!("Install and restart"), true).clicked() {
                    action = Some(Action::Install);
                }
            });
            return action;
        }
        Fetch::Failed(e) => {
            ui.label(egui::RichText::new(e.message()).color(ui.visuals().error_fg_color));
            if let Some(d) = e.detail() {
                ui.label(muted(d.to_string()).small());
            }
            ui.add_space(4.0);
        }
        Fetch::Idle => {}
    }
    match &app.updates.check {
        Check::Idle => {
            ui.label(tl!("No check has run yet."));
        }
        Check::Running { .. } => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tl!("Checking for a newer version…"));
            });
        }
        Check::Done(Ok(r)) if is_newer(&r.version, current) => {
            ui.label(egui::RichText::new(crate::i18n::fmt(&crate::branded(tl!("PdfCraft {v} is available.")), &[("v", plain(&r.version))])).strong());
            let installable = r.installer.is_some() && app.update_installer.is_some();
            if installable {
                ui.label(muted(crate::i18n::fmt(tl!("You have version {c}."), &[("c", current)])));
            } else {
                ui.label(muted(crate::i18n::fmt(tl!("You have version {c}. Download the new version from its release page."), &[("c", current)])));
            }
            ui.horizontal(|ui| {
                if installable {
                    let label = if matches!(app.updates.fetch, Fetch::Failed(_)) { tl!("Try again") } else { tl!("Download and install") };
                    if widgets::pill_button(ui, label, true).clicked() {
                        action = Some(Action::Download(r.clone()));
                    }
                    if widgets::pill_button(ui, tl!("Release page"), false).clicked() {
                        action = Some(Action::OpenPage(r.url.clone()));
                    }
                } else if widgets::pill_button(ui, tl!("Download"), true).clicked() {
                    action = Some(Action::OpenPage(r.url.clone()));
                }
            });
        }
        Check::Done(Ok(_)) => {
            ui.label(crate::i18n::fmt(&crate::branded(tl!("You're using the latest version ({c}).")), &[("c", current)]));
        }
        Check::Done(Err(e)) => {
            ui.label(
                egui::RichText::new(tl!("Couldn't check for updates. Check that you're connected to the internet and try again."))
                    .color(ui.visuals().error_fg_color),
            );
            if !e.is_empty() {
                ui.label(muted(e.clone()).small());
            }
            ui.label(
                muted(crate::i18n::fmt(tl!("You have version {c}. All releases are listed at {page}."), &[("c", current), ("page", RELEASES_PAGE)]))
                    .small(),
            );
        }
    }
    action
}

/// Act on what the user chose in [`status`].
fn act(app: &mut PdfCraftApp, action: Action) {
    match action {
        Action::Check => {
            app.start_check(false, false);
        }
        Action::Download(r) => app.start_download(r),
        Action::OpenPage(url) => app.open_url(&url),
        Action::Cancel => {
            if let Fetch::Running { progress, rx, .. } = &app.updates.fetch {
                progress.cancel();
                // The screenshot state has no download to stop.
                if rx.is_none() {
                    app.updates.fetch = Fetch::Idle;
                }
            }
        }
        Action::Install => app.install_downloaded_update(),
    }
}

/// The "Check for updates automatically" switch.
fn auto_toggle(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &theme::Tokens) {
    if cfg!(target_arch = "wasm32") {
        return;
    }
    if ui.checkbox(&mut app.updates.auto, tl!("Check for updates automatically")).changed() {
        app.save_requested = true;
    }
    ui.label(
        egui::RichText::new(crate::branded(tl!(
            "At most once a day, in the background, when PdfCraft starts. It only tells you about a new version: nothing is installed until you choose."
        )))
        .small()
        .color(t.text_muted),
    );
}

/// Preferences ▸ Updates.
pub(crate) fn preferences_section(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &theme::Tokens) {
    ui.label(egui::RichText::new(tl!("Updates")).font(theme::semibold(13.0)));
    let mut action = None;
    let mut status_action = None;
    let frame = egui::Frame::new().fill(t.hover).corner_radius(egui::CornerRadius::same(6)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        // The button follows the version: a right-aligned layout here would take the height the
        // Preferences scroll area offers, and grow it frame after frame.
        let busy = matches!(app.updates.check, Check::Running { .. }) || matches!(app.updates.fetch, Fetch::Running { .. });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tl!("Current version")).color(t.text_muted));
            ui.label(egui::RichText::new(APP_VERSION).strong());
            ui.add_space(12.0);
            if ui.add_enabled_ui(!busy, |ui| widgets::pill_button(ui, tl!("Check now"), false)).inner.clicked() {
                action = Some(Action::Check);
            }
        });
        auto_toggle(ui, app, t);
        ui.add_space(6.0);
        if !matches!((&app.updates.check, &app.updates.fetch), (Check::Idle, Fetch::Idle)) {
            status_action = status(ui, app, t);
        }
    });
    if std::mem::take(&mut app.updates.reveal) {
        ui.scroll_to_rect(frame.response.rect, Some(Align::Max));
    }
    if let Some(a) = status_action.or(action) {
        act(app, a);
    }
}

/// The Updates dialog (Help ▸ Check for updates).
pub(crate) fn dialog(app: &mut PdfCraftApp, ctx: &egui::Context) {
    if !app.updates.open {
        return;
    }
    let t = theme::Tokens::get(ctx);
    let mut close = false;
    let mut action = None;
    let modal = egui::Modal::new(egui::Id::new("updates")).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.horizontal(|ui| {
            ui.add(crate::icons::image("cloud", 22.0, t.accent));
            ui.label(egui::RichText::new(tl!("Check for updates")).font(theme::semibold(16.0)));
        });
        ui.add_space(8.0);
        action = status(ui, app, &t);
        ui.add_space(10.0);
        auto_toggle(ui, app, &t);
        ui.add_space(12.0);
        let offered = matches!(&app.updates.check, Check::Done(Ok(r)) if is_newer(&r.version, APP_VERSION));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let label = if offered && matches!(app.updates.fetch, Fetch::Idle | Fetch::Failed(_)) { tl!("Later") } else { tl!("Close") };
            if widgets::pill_button(ui, label, !offered).clicked() {
                close = true;
            }
        });
    });
    if modal.should_close() {
        close = true;
    }
    if let Some(a) = action {
        // Opening the release page is the dialog's last step; a download keeps showing here.
        close |= matches!(a, Action::OpenPage(_));
        act(app, a);
    }
    if close {
        app.updates.open = false;
    }
}

/// The automatic check's notice: a newer version exists, with a button to see it.
pub(crate) fn notice(app: &mut PdfCraftApp, ctx: &egui::Context) {
    let Some(release) = app.updates.notice.clone() else { return };
    if app.updates.open || app.dialog.is_some() {
        return;
    }
    let t = theme::Tokens::get(ctx);
    let screen = ctx.content_rect();
    let (mut details, mut later) = (false, false);
    egui::Area::new(egui::Id::new("update-notice"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .fixed_pos(screen.right_bottom() - egui::vec2(20.0, 20.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(t.card)
                .stroke(egui::Stroke::new(1.0, t.border))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(egui::Margin::same(14))
                .shadow(ui.visuals().popup_shadow)
                .show(ui, |ui| {
                    ui.set_max_width(320.0);
                    ui.horizontal(|ui| {
                        ui.add(crate::icons::image("cloud", 18.0, t.accent));
                        ui.label(
                            egui::RichText::new(crate::i18n::fmt(
                                &crate::branded(tl!("PdfCraft {v} is available.")),
                                &[("v", plain(&release.version))],
                            ))
                            .strong(),
                        );
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        details = widgets::pill_button(ui, tl!("See update"), true).clicked();
                        later = widgets::pill_button(ui, tl!("Later"), false).clicked();
                    });
                });
        });
    if details {
        app.updates.notice = None;
        app.updates.open = true;
    } else if later {
        app.updates.notice = None;
    }
}
