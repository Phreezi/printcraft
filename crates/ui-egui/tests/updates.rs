//! Updates (issue #28): Help ▸ Check for updates, Preferences ▸ Updates, the automatic check and
//! downloading and installing, with stand-in release sources and installers (no network, no
//! processes).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::updates::{Asset, CHECK_INTERVAL, Release, UpdateError, UpdateInstaller, UpdateSource, check_due, is_newer};
use pdfcraft_ui_egui::{CloseRequest, PdfCraftApp};

const PAGE: &str = "https://github.com/Phreezi/printcraft/releases";

fn release(v: &str, installer: bool) -> Release {
    Release {
        url: format!("{PAGE}/tag/{v}"),
        version: v.to_string(),
        installer: installer.then(|| Asset {
            name: "pedeefe-99.0.0-windows-x64.msi".into(),
            url: format!("{PAGE}/download/{v}/pedeefe-99.0.0-windows-x64.msi"),
            size: 1000,
        }),
        checksums: installer.then(|| Asset { name: "SHA256SUMS".into(), url: format!("{PAGE}/download/{v}/SHA256SUMS"), size: 100 }),
    }
}

fn source(answer: Result<&str, &str>) -> UpdateSource {
    let answer = answer.map(str::to_string).map_err(str::to_string);
    Arc::new(move || answer.clone().map(|v| release(&v, false)))
}

fn harness(answer: Result<&str, &str>) -> Harness<'static, PdfCraftApp> {
    Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.update_source = Some(source(answer));
        app
    })
}

/// Run frames until `done` holds (background work reports on its own thread), or give up.
fn wait(h: &mut Harness<'static, PdfCraftApp>, done: impl Fn(&Harness<'static, PdfCraftApp>) -> bool) {
    for _ in 0..400 {
        h.run_steps(2);
        if done(h) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(2);
}

/// Run frames until the background check has reported.
fn settle(h: &mut Harness<'static, PdfCraftApp>) {
    wait(h, |h| h.query_by_label_contains("Checking for a newer version").is_none());
}

#[test]
fn versions_compare_by_number() {
    assert!(is_newer("v0.2.0", "0.1.1"));
    assert!(is_newer("v0.1.10", "0.1.9"));
    assert!(is_newer("1", "0.9.9"));
    assert!(!is_newer("v0.1.1", "0.1.1"));
    assert!(!is_newer("v0.1.0", "0.1.1"));
    assert!(!is_newer("v0.1.1-beta.2", "0.1.1"), "a pre-release comes before its release");
    assert!(is_newer("v0.1.1", "0.1.1-beta.2"), "the release is newer than its pre-releases");
    assert!(is_newer("v0.2.1-test.10", "0.2.1-test.9"), "test builds compare by number");
    assert!(!is_newer("v0.2.1-test.9", "0.2.1-test.10"));
    assert!(!is_newer("v0.2.1-test.9", "0.2.1-test.9"));
    assert!(is_newer("v0.2.2-test.1", "0.2.1-test.40"), "the core version comes first");
    // The workspace moved to 0.3.0 with PdfCraft's version: its test builds replace the 0.2.1 ones.
    assert!(is_newer("v0.3.0-test.41", "0.2.1-test.40"));
    assert!(is_newer("v0.3.0-test.1", "0.2.1-test.99"), "whatever the run numbers");
    assert!(!is_newer("v0.2.1-test.99", "0.3.0-test.41"), "an older line is never offered");
    assert!(is_newer("v0.2.1-rc.1", "0.2.1-beta.5"), "identifiers compare in ASCII order");
    assert!(is_newer("v0.2.1-test.1.1", "0.2.1-test.1"), "more identifiers win a tie");
    assert!(!is_newer("v0.2.1+build.5", "0.2.1"), "build metadata is ignored");
    assert!(!is_newer("v0.2.1-te$t.1", "0.2.0"), "a malformed pre-release doesn't parse");
    assert!(!is_newer("nightly", "0.1.1"), "a tag that isn't a version is never newer");
    assert!(!is_newer("v1.2.3.4", "0.1.1"));
    assert!(!is_newer("v99999999999999999999.0.0", "0.1.1"), "out of range");
}

#[test]
fn the_automatic_check_asks_at_most_once_a_day() {
    let day = CHECK_INTERVAL;
    let t = 1_800_000_000;
    assert!(check_due(true, 0, t), "never checked");
    assert!(!check_due(true, t, t));
    assert!(!check_due(true, t, t + day - 1), "within a day");
    assert!(check_due(true, t, t + day), "a day later");
    assert!(check_due(true, t, t + 30 * day));
    assert!(check_due(true, t, t - 60), "the clock went back");
    assert!(!check_due(false, 0, t), "turned off");
    assert!(!check_due(false, t, t + 30 * day));
}

#[test]
fn a_newer_release_is_offered_for_download() {
    let mut h = harness(Ok("v99.0.0"));
    h.state_mut().execute("help.check_updates");
    settle(&mut h);
    h.get_by_label_contains("PeDeeFe 99.0.0 is available");
    // No installer here (other systems): the release page offers the download.
    h.get_by_label("Download");
    assert!(h.query_by_label("Download and install").is_none());
    h.get_by_label("Later").click();
    h.run_steps(3);
    assert!(h.query_by_label_contains("is available").is_none(), "Later closes the dialog");
}

#[test]
fn an_up_to_date_or_failed_check_says_so() {
    let mut h = harness(Ok(concat!("v", env!("CARGO_PKG_VERSION"))));
    h.state_mut().execute("help.check_updates");
    settle(&mut h);
    h.get_by_label_contains("You're using the latest version");
    assert!(h.query_by_label("Download").is_none());

    let mut h = harness(Err("couldn't reach GitHub"));
    h.state_mut().execute("help.check_updates");
    settle(&mut h);
    h.get_by_label_contains("Couldn't check for updates. Check that you're connected to the internet");
    h.get_by_label("couldn't reach GitHub");
}

/// A source that counts its calls.
fn counted(answer: &'static str) -> (UpdateSource, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    (
        Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(release(answer, false))
        }),
        calls,
    )
}

#[test]
fn the_automatic_check_only_notifies_and_is_remembered() {
    let (src, calls) = counted("v99.0.0");
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.update_source = Some(src);
        app
    });
    h.run_steps(2);
    // Nothing is asked until the desktop app starts the automatic check.
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(h.state().auto_update_check(), "on by default");
    let t = 1_800_000_000;
    assert!(h.state_mut().start_automatic_update_check(t));
    wait(&mut h, |h| h.state().update_notice().is_some());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    // A notice with a button, not the dialog; nothing downloads.
    h.get_by_label_contains("PeDeeFe 99.0.0 is available");
    assert!(h.query_by_label("Later").is_some());
    assert!(h.query_by_label_contains("Downloading").is_none());
    h.get_by_label("See update").click();
    h.run_steps(3);
    assert!(h.state().update_notice().is_none());
    h.get_by_label("Download");
    h.get_by_label("Later").click();
    h.run_steps(3);
    // Within a day: not again. The time is remembered with the settings.
    assert!(!h.state_mut().start_automatic_update_check(t + 3600));
    let saved = h.state().persist();
    let mut next = PdfCraftApp::new();
    next.restore(&saved);
    let (src, again) = counted("v99.0.0");
    next.update_source = Some(src);
    assert!(!next.start_automatic_update_check(t + 3600), "restored: still within the day");
    assert!(next.start_automatic_update_check(t + CHECK_INTERVAL), "a day later");
    // Turned off: never.
    let mut off = PdfCraftApp::new();
    off.restore(r#"{"updates": {"auto": false, "last_check": 0}}"#);
    off.update_source = Some(counted("v99.0.0").0);
    assert!(!off.auto_update_check());
    assert!(!off.start_automatic_update_check(t));
    // Malformed settings keep the defaults.
    let mut odd = PdfCraftApp::new();
    odd.restore(r#"{"updates": {"auto": "yes", "last_check": -5}}"#);
    assert!(odd.auto_update_check());
    // Without a source (the web build) nothing is asked.
    assert!(!PdfCraftApp::new().start_automatic_update_check(t));
    wait(&mut h, |_| again.load(Ordering::SeqCst) == 1);
    assert_eq!(again.load(Ordering::SeqCst), 1, "the restored app asked once");
}

#[test]
fn an_automatic_check_without_news_stays_quiet() {
    for answer in [Ok(concat!("v", env!("CARGO_PKG_VERSION"))), Err("offline")] {
        let mut h = harness(answer);
        h.run_steps(2);
        assert!(h.state_mut().start_automatic_update_check(1_800_000_000));
        settle(&mut h);
        std::thread::sleep(std::time::Duration::from_millis(50));
        h.run_steps(4);
        assert!(h.state().update_notice().is_none(), "{answer:?}");
        assert!(h.query_by_label_contains("is available").is_none());
        assert!(h.query_by_label_contains("Couldn't check").is_none(), "errors aren't shown unasked");
    }
}

/// What a stand-in installer was asked to do.
#[derive(Default)]
struct Calls {
    downloads: usize,
    launched: Vec<PathBuf>,
}

type DownloadStep = Box<dyn Fn(&pdfcraft_ui_egui::updates::Progress) -> Result<PathBuf, UpdateError> + Send + Sync>;

/// An app offering `v99.0.0` with an installer; `download` decides how its download goes and
/// `launch_ok` whether the installer starts.
fn installing(download: DownloadStep, launch_ok: bool, dirty: bool) -> (Harness<'static, PdfCraftApp>, Arc<Mutex<Calls>>) {
    let calls = Arc::new(Mutex::new(Calls::default()));
    let (dc, lc) = (calls.clone(), calls.clone());
    let download = Arc::new(download);
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        if dirty {
            app.open_bytes("doc.pdf", None, fixture()).expect("fixture opens");
            app.views[0].select_pages(&[0]);
            app.apply_edit(pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 });
        }
        app.update_source = Some(Arc::new(|| Ok(release("v99.0.0", true))));
        let download = download.clone();
        app.update_installer = Some(UpdateInstaller {
            download: Arc::new(move |r: &Release, p| {
                assert!(r.installer.is_some());
                dc.lock().unwrap().downloads += 1;
                download(p)
            }),
            launch: Arc::new(move |path: &Path| {
                lc.lock().unwrap().launched.push(path.to_path_buf());
                if launch_ok { Ok(()) } else { Err(UpdateError::Launch("PowerShell is missing".into())) }
            }),
        });
        app.set_option("dialog", "preferences").unwrap();
        app
    });
    h.run_steps(3);
    h.get_by_label("Check now").click();
    wait(&mut h, |h| h.query_by_label("Download and install").is_some());
    (h, calls)
}

/// A one-page PDF.
fn fixture() -> Vec<u8> {
    let objs =
        ["<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>", "<< /Type /Page /Parent 2 0 R >>"];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn close_root(h: &mut Harness<'static, PdfCraftApp>) {
    h.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
}

fn root_close_sent(h: &Harness<'static, PdfCraftApp>) -> bool {
    h.output().viewport_output.get(&egui::ViewportId::ROOT).is_some_and(|v| v.commands.iter().any(|c| matches!(c, egui::ViewportCommand::Close)))
}

/// Run frames one at a time until the app asks to close; `false` if it never does.
fn wait_for_close(h: &mut Harness<'static, PdfCraftApp>) -> bool {
    for _ in 0..400 {
        h.step();
        if root_close_sent(h) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    false
}

#[test]
fn preferences_show_the_version_the_switch_and_the_check() {
    let (src, calls) = counted(concat!("v", env!("CARGO_PKG_VERSION")));
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.update_source = Some(src);
        app.set_option("dialog", "preferences").unwrap();
        app
    });
    h.run_steps(3);
    h.get_by_label("Updates");
    h.get_by_label("Current version");
    h.get_by_label(pdfcraft_ui_egui::updates::APP_VERSION);
    // Up to date.
    h.get_by_label("Check now").click();
    wait(&mut h, |h| h.query_by_label_contains("You're using the latest version").is_some());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(h.state().dialog, Some(pdfcraft_ui_egui::Dialog::Preferences), "checking from Preferences stays there");
    // The switch is remembered.
    h.get_by_label("Check for updates automatically").click();
    h.run_steps(2);
    assert!(!h.state().auto_update_check());
    let saved: serde_json::Value = serde_json::from_str(&h.state().persist()).unwrap();
    assert_eq!(saved["updates"]["auto"], false);
}

#[test]
fn download_progress_and_errors_show_in_preferences() {
    // The download waits until the test lets it finish, with a known amount done.
    let gate = Arc::new(Mutex::new(None::<Result<PathBuf, UpdateError>>));
    let g = gate.clone();
    let step: DownloadStep = Box::new(move |p| {
        p.set(250, Some(1000));
        loop {
            if p.cancelled() {
                return Err(UpdateError::Cancelled);
            }
            if let Some(r) = g.lock().unwrap().take() {
                return r;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    });
    let (mut h, calls) = installing(step, true, false);
    h.get_by_label_contains("PeDeeFe 99.0.0 is available");
    h.get_by_label("Download and install").click();
    wait(&mut h, |h| h.query_by_label_contains("250 bytes of 1000 bytes").is_some());
    h.get_by_label_contains("Downloading PeDeeFe 99.0.0");
    h.get_by_label("Cancel");
    assert!(h.query_by(|n| n.label().as_deref() == Some("Check now") && n.is_disabled()).is_some(), "no new check while downloading");
    // The checksum doesn't match: said plainly, nothing installed, and it can be tried again.
    *gate.lock().unwrap() = Some(Err(UpdateError::Mismatch));
    wait(&mut h, |h| h.query_by_label_contains("checksum doesn't match").is_some());
    h.get_by_label_contains("The downloaded file is damaged or was changed");
    h.get_by_label("Try again");
    assert!(calls.lock().unwrap().launched.is_empty());
    assert!(!root_close_sent(&h));
    // Cancelling a second attempt goes back to the offer, without an error.
    h.get_by_label("Try again").click();
    wait(&mut h, |h| h.query_by_label("Cancel").is_some());
    h.get_by_label("Cancel").click();
    wait(&mut h, |h| h.query_by_label("Download and install").is_some());
    assert!(h.query_by_label_contains("cancelled").is_none());
    assert_eq!(calls.lock().unwrap().downloads, 2);
    // A missing checksum and a network failure, in plain words with the detail below.
    for (error, words) in [
        (UpdateError::NoChecksum, "the release has no checksum for it"),
        (UpdateError::TooLarge, "much larger than expected"),
        (UpdateError::Network("timed out".into()), "Check that you're connected to the internet"),
    ] {
        *gate.lock().unwrap() = Some(Err(error));
        h.query_by_label("Download and install").or_else(|| h.query_by_label("Try again")).expect("the download is offered").click();
        wait(&mut h, |h| h.query_by_label_contains(words).is_some());
        h.get_by_label_contains(words);
    }
    h.get_by_label("timed out");
}

#[test]
fn a_verified_download_installs_after_the_app_closes() {
    let msi = PathBuf::from("pedeefe-99.0.0-windows-x64.msi");
    let path = msi.clone();
    let (mut h, calls) = installing(Box::new(move |_| Ok(path.clone())), true, false);
    h.get_by_label("Download and install").click();
    assert!(wait_for_close(&mut h), "the app closes to install");
    assert_eq!(h.state().dialog, None, "Preferences closed");
    assert!(calls.lock().unwrap().launched.is_empty(), "the installer starts only as the app closes");
    close_root(&mut h);
    h.step();
    assert_eq!(calls.lock().unwrap().launched, vec![msi]);
}

#[test]
fn unsaved_changes_are_asked_about_and_cancelling_keeps_the_update_ready() {
    let msi = PathBuf::from("pedeefe-99.0.0-windows-x64.msi");
    let path = msi.clone();
    let (mut h, calls) = installing(Box::new(move |_| Ok(path.clone())), true, true);
    h.get_by_label("Download and install").click();
    assert!(wait_for_close(&mut h));
    close_root(&mut h);
    h.run_steps(3);
    h.get_by_label_contains("Save changes to “doc.pdf”");
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(calls.lock().unwrap().launched.is_empty(), "cancelled: nothing installs");
    assert_eq!(h.state().views.len(), 1, "the document stays open");
    h.get_by_label_contains("The update wasn't installed");
    // Preferences offers to install the downloaded update.
    h.state_mut().set_option("dialog", "preferences").unwrap();
    h.run_steps(3);
    h.get_by_label_contains("PeDeeFe 99.0.0 is downloaded and verified");
    h.get_by_label("Install and restart").click();
    assert!(wait_for_close(&mut h));
    close_root(&mut h);
    h.run_steps(3);
    h.get_by_label("Don't save").click();
    h.run_steps(3);
    assert_eq!(h.state().close_request, None::<CloseRequest>);
    // The quit goes ahead once the document is closed: the next close starts the installer.
    close_root(&mut h);
    h.step();
    assert_eq!(calls.lock().unwrap().launched, vec![msi]);
}

#[test]
fn an_installer_that_cannot_start_cancels_the_close_and_says_why() {
    let (mut h, calls) = installing(Box::new(|_| Ok(PathBuf::from("x.msi"))), false, false);
    h.get_by_label("Download and install").click();
    assert!(wait_for_close(&mut h));
    close_root(&mut h);
    h.run_steps(3);
    assert_eq!(calls.lock().unwrap().launched.len(), 1);
    h.get_by_label("The installer couldn't be started.");
    h.get_by_label("PowerShell is missing");
}

#[test]
fn the_screenshot_states_render() {
    for state in ["idle", "up-to-date", "available", "downloading", "ready", "failed", "error"] {
        let mut h = Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_eframe(move |_cc| {
            let mut app = PdfCraftApp::new();
            app.set_option("dialog", "preferences").unwrap();
            app.set_option("update-state", state).unwrap();
            app
        });
        h.run_steps(3);
        h.get_by_label("Check now");
        let marker = match state {
            "idle" => "Check for updates automatically",
            "up-to-date" => "You're using the latest version",
            "available" => "Download and install",
            "downloading" => "Downloading PeDeeFe",
            "ready" => "Install and restart",
            "failed" => "checksum doesn't match",
            _ => "Couldn't check for updates",
        };
        assert!(h.query_by_label_contains(marker).is_some(), "{state}: {marker}");
    }
    assert!(PdfCraftApp::new().set_option("update-state", "bogus").is_err());
}
