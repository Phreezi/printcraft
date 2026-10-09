//! Updates (issue #28): asks GitHub for the latest PeDeeFe build, published by
//! `.github/workflows/test-build.yml` as this fork's releases, and on Windows downloads, verifies
//! and installs it (`pdfcraft_ui_egui::updates` has the interface and explains the flow).
//!
//! Everything GitHub answers is untrusted:
//! - only files under [`RELEASES_PAGE`]`/download/` are ever downloaded ([`is_allowed_download`]);
//!   GitHub's redirect to its download servers is followed, over HTTPS only;
//! - the answer, the checksum list and the installer have size caps;
//! - the installer is hashed while it streams to the temporary folder and must match its line in
//!   the release's `SHA256SUMS` ([`checksum_for`]); a missing or different checksum refuses it;
//! - the installer's path reaches the installing script through environment variables, never
//!   through the script's text or a command line it builds ([`install_plan`], [`INSTALL_SCRIPT`]).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pdfcraft_ui_egui::updates::{APP_VERSION, Asset, Progress, RELEASES_PAGE, Release, UpdateError, UpdateInstaller};
use sha2::{Digest, Sha256};

const LATEST: &str = "https://api.github.com/repos/Phreezi/printcraft/releases/latest";

/// The largest installer accepted (bytes); PeDeeFe's is about 40 MB.
pub const MAX_INSTALLER: u64 = 300 << 20;
/// The largest checksum list accepted (bytes).
const MAX_SUMS: u64 = 64 << 10;
/// The checksum list's names: `SHA256SUMS`, and `SHA256SUMS.txt` in releases before it.
const SUMS_NAMES: [&str; 2] = ["SHA256SUMS", "SHA256SUMS.txt"];

/// The latest release. The answer is untrusted: its size is capped, only a page under
/// [`RELEASES_PAGE`] is ever offered (anything else falls back to that list), and only release
/// files pass [`is_allowed_download`].
pub fn latest_release() -> Result<Release, String> {
    let agent = agent(Some(Duration::from_secs(10)))?;
    let mut response =
        agent.get(LATEST).header("Accept", "application/vnd.github+json").call().map_err(|e| format!("couldn't reach GitHub ({e})"))?;
    let body = response.body_mut().with_config().limit(1 << 20).read_to_string().map_err(|e| format!("unreadable answer ({e})"))?;
    parse(&body, installer_arch())
}

/// The installers' name for this computer's architecture (`x64`, `arm64`), on Windows only:
/// elsewhere there is no installer to offer, and the release page opens instead.
pub fn installer_arch() -> Option<&'static str> {
    if !cfg!(windows) {
        return None;
    }
    arch_name(std::env::consts::ARCH)
}

/// The installers' name for a Rust architecture.
fn arch_name(arch: &str) -> Option<&'static str> {
    match arch {
        "x86_64" => Some("x64"),
        "aarch64" => Some("arm64"),
        _ => None,
    }
}

/// How the desktop app downloads and installs updates on Windows.
pub fn installer() -> UpdateInstaller {
    UpdateInstaller { download: Arc::new(download), launch: Arc::new(launch) }
}

/// An HTTPS client trusting the OS's certificate authorities. `timeout` bounds the whole call;
/// without it, a download may take its time but each step still has a limit.
fn agent(timeout: Option<Duration>) -> Result<ureq::Agent, String> {
    let mut config = ureq::Agent::config_builder()
        .https_only(true)
        .user_agent(format!("PeDeeFe/{APP_VERSION}"))
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(os_roots()?).build());
    config = match timeout {
        Some(t) => config.timeout_global(Some(t)),
        None => config
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            .timeout_recv_body(Some(Duration::from_secs(30 * 60))),
    };
    Ok(config.build().new_agent())
}

/// The certificate authorities the operating system trusts.
fn os_roots() -> Result<ureq::tls::RootCerts, String> {
    let found = rustls_native_certs::load_native_certs();
    let certs: Vec<ureq::tls::Certificate<'static>> = found.certs.iter().map(|c| ureq::tls::Certificate::from_der(c.as_ref()).to_owned()).collect();
    if certs.is_empty() {
        return Err("no trusted certificates found on this system".into());
    }
    Ok(ureq::tls::RootCerts::new_with_certs(&certs))
}

/// Read GitHub's answer; `arch` picks the installer ([`installer_arch`]).
fn parse(body: &str, arch: Option<&str>) -> Result<Release, String> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| format!("unreadable answer ({e})"))?;
    let version = v["tag_name"].as_str().filter(|t| !t.is_empty() && t.len() <= 64).ok_or("no release found")?.to_string();
    let url = v["html_url"]
        .as_str()
        .filter(|u| u.strip_prefix(RELEASES_PAGE).is_some_and(|rest| rest.starts_with('/') && !rest.contains(['?', '#', '\\'])))
        .unwrap_or(RELEASES_PAGE)
        .to_string();
    // A release lists a handful of files; a huge list is not ours.
    let assets: Vec<Asset> = v["assets"].as_array().map(|a| a.iter().take(200).filter_map(asset).collect()).unwrap_or_default();
    let installer = arch.and_then(|a| select_installer(&assets, a));
    let checksums = SUMS_NAMES.iter().find_map(|n| assets.iter().find(|a| a.name == *n)).cloned();
    Ok(Release { version, url, installer, checksums })
}

/// One file of the answer, if it is a release file with a plain name.
fn asset(v: &serde_json::Value) -> Option<Asset> {
    let name = v["name"].as_str().filter(|n| safe_file_name(n))?;
    let url = v["browser_download_url"].as_str().filter(|u| is_allowed_download(u) && u.ends_with(&format!("/{name}")))?;
    Some(Asset { name: name.to_string(), url: url.to_string(), size: v["size"].as_u64().unwrap_or(0) })
}

/// The Windows installer for architecture `arch` (`x64`, `arm64`):
/// `pedeefe-<version>-windows-<arch>.msi`.
pub fn select_installer(assets: &[Asset], arch: &str) -> Option<Asset> {
    let suffix = format!("-windows-{arch}.msi");
    assets.iter().find(|a| a.name.starts_with("pedeefe-") && a.name.ends_with(&suffix) && is_allowed_download(&a.url)).cloned()
}

/// Whether `url` is one of this repository's release files:
/// `https://github.com/Phreezi/printcraft/releases/download/<tag>/<file>`, with a plain tag and
/// file name (no queries, fragments, `..` or other paths).
pub fn is_allowed_download(url: &str) -> bool {
    let Some(rest) = url.strip_prefix(RELEASES_PAGE).and_then(|r| r.strip_prefix("/download/")) else { return false };
    let parts: Vec<&str> = rest.split('/').collect();
    matches!(parts.as_slice(), [tag, file] if safe_file_name(tag) && safe_file_name(file))
}

/// A plain file name: letters, digits, `.`, `_`, `+` and `-`, not starting with a dot.
fn safe_file_name(name: &str) -> bool {
    (1..=128).contains(&name.len())
        && !name.starts_with('.')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
}

/// The SHA-256 that `sums` (`sha256sum` output: `<hex>  <name>` or `<hex> *<name>` per line)
/// lists for file `name`. No line, a malformed one or two different ones refuse the file.
pub fn checksum_for(sums: &str, name: &str) -> Result<[u8; 32], UpdateError> {
    let mut found: Option<[u8; 32]> = None;
    for line in sums.lines().take(10_000) {
        let Some((hex, file)) = line.trim_end_matches('\r').split_once([' ', '\t']) else { continue };
        let file = file.trim_start_matches([' ', '\t']);
        if file.strip_prefix('*').unwrap_or(file) != name {
            continue;
        }
        let hash = parse_hex(hex).ok_or(UpdateError::NoChecksum)?;
        if found.is_some_and(|f| f != hash) {
            return Err(UpdateError::NoChecksum);
        }
        found = Some(hash);
    }
    found.ok_or(UpdateError::NoChecksum)
}

/// 64 hexadecimal digits as 32 bytes.
fn parse_hex(hex: &str) -> Option<[u8; 32]> {
    let digits = hex.as_bytes();
    if digits.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    let (pairs, _) = digits.as_chunks::<2>();
    for (byte, [hi, lo]) in out.iter_mut().zip(pairs) {
        let digit = |c: u8| (c as char).to_digit(16);
        let (Some(hi), Some(lo)) = (digit(*hi), digit(*lo)) else { return None };
        // Both digits are below 16.
        *byte = (hi * 16 + lo) as u8;
    }
    Some(out)
}

/// Copy `from` to `to`, hashing as it goes, and check the result against `expected`. Stops at
/// `cap` bytes (or at once when the announced `total` is larger), when `progress` is cancelled,
/// and when the stream ends before the announced `total`. Returns the size.
pub fn copy_verified(
    mut from: impl Read,
    to: &mut impl Write,
    expected: &[u8; 32],
    total: Option<u64>,
    cap: u64,
    progress: &Progress,
) -> Result<u64, UpdateError> {
    if total.is_some_and(|t| t > cap) {
        return Err(UpdateError::TooLarge);
    }
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 << 10];
    let mut done: u64 = 0;
    progress.set(0, total);
    loop {
        if progress.cancelled() {
            return Err(UpdateError::Cancelled);
        }
        let n = match from.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(UpdateError::Network(e.to_string())),
        };
        done = done.saturating_add(n as u64);
        if done > cap {
            return Err(UpdateError::TooLarge);
        }
        let chunk = buf.get(..n).ok_or_else(|| UpdateError::Network("the download misbehaved".into()))?;
        hasher.update(chunk);
        to.write_all(chunk).map_err(|e| UpdateError::Disk(e.to_string()))?;
        progress.set(done, total);
    }
    if total.is_some_and(|t| done < t) {
        return Err(UpdateError::Network(format!("the download ended early ({done} bytes)")));
    }
    if hasher.finalize().as_slice() != expected.as_slice() {
        return Err(UpdateError::Mismatch);
    }
    Ok(done)
}

/// Where downloaded updates go: a folder of the user's temporary folder.
fn download_dir() -> PathBuf {
    std::env::temp_dir().join("pedeefe-update")
}

/// Download `release`'s installer into the temporary folder and verify it against the release's
/// `SHA256SUMS`. Returns the verified file.
fn download(release: &Release, progress: &Progress) -> Result<PathBuf, UpdateError> {
    let installer = release.installer.as_ref().ok_or(UpdateError::NoInstaller)?;
    if !is_allowed_download(&installer.url) || !safe_file_name(&installer.name) || !installer.url.ends_with(&format!("/{}", installer.name)) {
        return Err(UpdateError::NotAllowed);
    }
    let sums = release.checksums.as_ref().ok_or(UpdateError::NoChecksum)?;
    if !is_allowed_download(&sums.url) {
        return Err(UpdateError::NotAllowed);
    }
    let agent = agent(None).map_err(UpdateError::Network)?;
    let text = agent
        .get(&sums.url)
        .call()
        .and_then(|mut r| r.body_mut().with_config().limit(MAX_SUMS).read_to_string())
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let expected = checksum_for(&text, &installer.name)?;
    let dir = download_dir();
    std::fs::create_dir_all(&dir).map_err(|e| UpdateError::Disk(format!("{}: {e}", dir.display())))?;
    let target = dir.join(&installer.name);
    let part = dir.join(format!("{}.part", installer.name));
    let mut response = agent.get(&installer.url).call().map_err(|e| UpdateError::Network(e.to_string()))?;
    let total = response.body().content_length().or(Some(installer.size).filter(|s| *s > 0));
    let reader = response.body_mut().with_config().limit(MAX_INSTALLER.saturating_add(1)).reader();
    let result = (|| {
        let file = std::fs::File::create(&part).map_err(|e| UpdateError::Disk(format!("{}: {e}", part.display())))?;
        let mut out = std::io::BufWriter::new(file);
        copy_verified(reader, &mut out, &expected, total, MAX_INSTALLER, progress)?;
        let file = out.into_inner().map_err(|e| UpdateError::Disk(e.to_string()))?;
        file.sync_all().map_err(|e| UpdateError::Disk(e.to_string()))?;
        drop(file);
        std::fs::rename(&part, &target).map_err(|e| UpdateError::Disk(format!("{}: {e}", target.display())))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result.map(|()| target)
}

/// Waits for PeDeeFe to exit, installs the update with Windows Installer (which asks for
/// administrator rights), and opens the installed PeDeeFe again; it opens even when the
/// installation failed or was declined, so the user is never left without the app. Every value
/// comes from the environment: nothing of the paths is ever part of this text.
#[cfg_attr(not(windows), allow(dead_code))] // Used by `launch` on Windows; tested everywhere.
pub const INSTALL_SCRIPT: &str = r#"# PeDeeFe update: wait for the app to exit, install the update, open the app again.
$ErrorActionPreference = 'Continue'
$appPid = 0
[void][int]::TryParse($env:PEDEEFE_UPDATE_PID, [ref]$appPid)
if ($appPid -gt 0) {
  $app = Get-Process -Id $appPid -ErrorAction SilentlyContinue
  if ($app) { [void]$app.WaitForExit(300000) }
}
$msi = $env:PEDEEFE_UPDATE_MSI
$log = $env:PEDEEFE_UPDATE_LOG
$msiexec = Join-Path $env:SystemRoot 'System32\msiexec.exe'
$msiArgs = @('/i', ('"' + $msi + '"'), '/passive', '/norestart', '/l*v', ('"' + $log + '"'))
$code = -1
try {
  $run = Start-Process -FilePath $msiexec -ArgumentList $msiArgs -Wait -PassThru
  $code = $run.ExitCode
} catch { }
$exe = $null
try {
  $exe = (Get-ItemProperty -LiteralPath 'Registry::HKEY_LOCAL_MACHINE\Software\Microsoft\Windows\CurrentVersion\App Paths\pedeefe.exe' -ErrorAction Stop).'(default)'
} catch { }
if (-not $exe -or -not (Test-Path -LiteralPath $exe)) { $exe = $env:PEDEEFE_UPDATE_EXE }
if ($exe -and (Test-Path -LiteralPath $exe)) { Start-Process -FilePath $exe }
# The installer is kept only when it failed, next to its log.
if ($code -eq 0) { Remove-Item -LiteralPath $msi -ErrorAction SilentlyContinue }
"#;

/// The process that installs the update: Windows PowerShell running [`INSTALL_SCRIPT`] (its
/// file), with the values in its environment.
#[derive(Debug, PartialEq)]
#[cfg_attr(not(windows), allow(dead_code))] // Used by `launch` on Windows; tested everywhere.
pub struct InstallPlan {
    pub program: PathBuf,
    pub args: Vec<std::ffi::OsString>,
    pub env: Vec<(&'static str, std::ffi::OsString)>,
}

/// Build the [`InstallPlan`]. Paths a Windows file name can't hold (`"`, control characters)
/// are refused rather than passed on.
#[cfg_attr(not(windows), allow(dead_code))] // Used by `launch` on Windows; tested everywhere.
pub fn install_plan(powershell: &Path, script: &Path, msi: &Path, log: &Path, exe: &Path, pid: u32) -> Result<InstallPlan, UpdateError> {
    for p in [script, msi, log, exe] {
        let s = p.to_string_lossy();
        if s.is_empty() || s.chars().any(|c| c == '"' || c.is_control()) {
            return Err(UpdateError::Launch(format!("unusable path {s:?}")));
        }
    }
    let args = ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"]
        .into_iter()
        .map(std::ffi::OsString::from)
        .chain([script.as_os_str().to_owned()])
        .collect();
    let env = vec![
        ("PEDEEFE_UPDATE_MSI", msi.as_os_str().to_owned()),
        ("PEDEEFE_UPDATE_LOG", log.as_os_str().to_owned()),
        ("PEDEEFE_UPDATE_EXE", exe.as_os_str().to_owned()),
        ("PEDEEFE_UPDATE_PID", pid.to_string().into()),
    ];
    Ok(InstallPlan { program: powershell.to_owned(), args, env })
}

/// Start the installing script for `msi`, to run once this process has exited.
#[cfg(windows)]
fn launch(msi: &Path) -> Result<(), UpdateError> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    /// CREATE_NO_WINDOW (no console window) | CREATE_NEW_PROCESS_GROUP (not stopped with us).
    const FLAGS: u32 = 0x0800_0000 | 0x0000_0200;
    let launch_err = |e: String| UpdateError::Launch(e);
    let dir = msi.parent().ok_or_else(|| launch_err("the installer has no folder".into()))?;
    let script = dir.join("install-update.ps1");
    std::fs::write(&script, INSTALL_SCRIPT).map_err(|e| launch_err(format!("{}: {e}", script.display())))?;
    // The system's own PowerShell, not whatever a PATH entry might shadow it with.
    let powershell = std::env::var_os("SystemRoot")
        .map(|r| PathBuf::from(r).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
        .filter(|p| p.exists())
        .unwrap_or_else(|| "powershell.exe".into());
    let exe = std::env::current_exe().map_err(|e| launch_err(e.to_string()))?;
    let plan = install_plan(&powershell, &script, msi, &dir.join("install-update.log"), &exe, std::process::id())?;
    Command::new(&plan.program)
        .args(&plan.args)
        .envs(plan.env.iter().map(|(k, v)| (*k, v.as_os_str())))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(FLAGS)
        .spawn()
        .map(drop)
        .map_err(|e| launch_err(format!("Windows PowerShell could not be started: {e}")))
}

/// Installing updates from the app is for Windows; elsewhere the release page opens instead.
#[cfg(not(windows))]
fn launch(_msi: &Path) -> Result<(), UpdateError> {
    Err(UpdateError::Launch("installing updates from the app is only available on Windows".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TAG: &str = "https://github.com/Phreezi/printcraft/releases/download/v0.3.0-test.41";

    fn asset(name: &str) -> Asset {
        Asset { name: name.into(), url: format!("{TAG}/{name}"), size: 1 }
    }

    fn sha(bytes: &[u8]) -> [u8; 32] {
        Sha256::digest(bytes).into()
    }

    fn hex(h: &[u8; 32]) -> String {
        h.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn answers_are_read_and_only_our_release_pages_are_offered() {
        let r = parse(r#"{"tag_name":"v0.2.0","html_url":"https://github.com/Phreezi/printcraft/releases/tag/v0.2.0"}"#, None).unwrap();
        assert_eq!(
            r,
            Release { version: "v0.2.0".into(), url: "https://github.com/Phreezi/printcraft/releases/tag/v0.2.0".into(), ..Default::default() }
        );
        for elsewhere in ["https://example.com/pdfcraft.exe", "https://github.com/Phreezi/printcraft/releases.evil/x", "javascript:alert(1)"] {
            let r = parse(&format!(r#"{{"tag_name":"v9.9.9","html_url":"{elsewhere}"}}"#), None).unwrap();
            assert_eq!(r.url, RELEASES_PAGE, "{elsewhere}");
        }
        assert!(parse(r#"{"message":"Not Found"}"#, None).is_err());
        assert!(parse("<html>", None).is_err());
    }

    #[test]
    fn the_installer_for_this_architecture_and_the_checksums_are_picked() {
        let body = serde_json::json!({
            "tag_name": "v0.3.0-test.41",
            "html_url": "https://github.com/Phreezi/printcraft/releases/tag/v0.3.0-test.41",
            "assets": [
                {"name": "pedeefe-0.3.0-test.41-windows-x64-portable.zip", "browser_download_url": format!("{TAG}/pedeefe-0.3.0-test.41-windows-x64-portable.zip"), "size": 30},
                {"name": "pedeefe-0.3.0-test.41-windows-arm64.msi", "browser_download_url": format!("{TAG}/pedeefe-0.3.0-test.41-windows-arm64.msi"), "size": 41},
                {"name": "pedeefe-0.3.0-test.41-windows-x64.msi", "browser_download_url": format!("{TAG}/pedeefe-0.3.0-test.41-windows-x64.msi"), "size": 40},
                {"name": "SHA256SUMS", "browser_download_url": format!("{TAG}/SHA256SUMS"), "size": 2},
                // Not ours: wrong host, or the URL names another file.
                {"name": "pedeefe-9-windows-x64.msi", "browser_download_url": "https://example.com/pedeefe-9-windows-x64.msi"},
                {"name": "x", "browser_download_url": 7}
            ]
        })
        .to_string();
        let x64 = parse(&body, Some("x64")).unwrap();
        assert_eq!(x64.installer.as_ref().map(|a| (a.name.as_str(), a.size)), Some(("pedeefe-0.3.0-test.41-windows-x64.msi", 40)));
        assert_eq!(x64.checksums.as_ref().map(|a| a.name.as_str()), Some("SHA256SUMS"));
        let arm = parse(&body, Some("arm64")).unwrap();
        assert_eq!(arm.installer.map(|a| a.name), Some("pedeefe-0.3.0-test.41-windows-arm64.msi".into()));
        // Other systems get no installer: the release page opens instead.
        assert_eq!(parse(&body, None).unwrap().installer, None);
        // An architecture without an installer gets none.
        let only_arm = [asset("pedeefe-1-windows-arm64.msi")];
        assert_eq!(select_installer(&only_arm, "x64"), None);
        assert_eq!(select_installer(&[asset("pdfcraft-1-windows-x64.msi"), asset("pedeefe-1-windows-x64.zip")], "x64"), None);
        // The older checksum file name is understood too.
        let old = body.replace("\"SHA256SUMS\"", "\"SHA256SUMS.txt\"").replace("/SHA256SUMS\"", "/SHA256SUMS.txt\"");
        assert_eq!(parse(&old, Some("x64")).unwrap().checksums.map(|a| a.name), Some("SHA256SUMS.txt".into()));
        assert_eq!(arch_name("x86_64"), Some("x64"));
        assert_eq!(arch_name("aarch64"), Some("arm64"));
        assert_eq!(arch_name("x86"), None);
        if !cfg!(windows) {
            assert_eq!(installer_arch(), None);
        }
    }

    #[test]
    fn only_this_repositorys_release_files_are_downloaded() {
        assert!(is_allowed_download(&format!("{TAG}/pedeefe-0.3.0-test.41-windows-x64.msi")));
        assert!(is_allowed_download(&format!("{TAG}/SHA256SUMS")));
        for bad in [
            "http://github.com/Phreezi/printcraft/releases/download/v1/pedeefe.msi",
            "https://github.com/Phreezi/printcraft/releases/download/v1/../../../evil/pedeefe.msi",
            "https://github.com/Phreezi/printcraft/releases/download/v1/sub/pedeefe.msi",
            "https://github.com/Phreezi/printcraft/releases/download/v1/pedeefe.msi?x=1",
            "https://github.com/Phreezi/printcraft/releases/download/v1/pedeefe.msi#x",
            "https://github.com/Phreezi/printcraft/releases/download/v1/",
            "https://github.com/Phreezi/printcraft/releases/download//pedeefe.msi",
            "https://github.com/Phreezi/printcraft/releases/download/v1/.hidden",
            "https://github.com/Phreezi/printcraft/releases/download/v1/a%2Fb.msi",
            "https://github.com/Phreezi/printcraft/releases/tag/v1",
            "https://github.com/Phreezi/printcraft.evil/releases/download/v1/pedeefe.msi",
            "https://github.com/Phreezi/printcraftx/releases/download/v1/pedeefe.msi",
            "https://github.com.evil.example/Phreezi/printcraft/releases/download/v1/pedeefe.msi",
            "https://objects.githubusercontent.com/pedeefe.msi",
            "https://github.com/Phreezi/printcraft/releases/download/v1/pede efe.msi",
            "https://github.com/Phreezi/printcraft/releases/download/v1/pedeefe\\x.msi",
            "",
        ] {
            assert!(!is_allowed_download(bad), "{bad}");
        }
    }

    #[test]
    fn checksums_are_read_from_sha256sums() {
        let good = sha(b"installer");
        let other = sha(b"portable");
        let sums = format!("{}  pedeefe-1-windows-x64-portable.zip\r\n{}  pedeefe-1-windows-x64.msi\n\nnot a line\n", hex(&other), hex(&good));
        assert_eq!(checksum_for(&sums, "pedeefe-1-windows-x64.msi"), Ok(good));
        assert_eq!(checksum_for(&sums, "pedeefe-1-windows-x64-portable.zip"), Ok(other));
        // Binary-mode lines and upper-case digits.
        assert_eq!(checksum_for(&format!("{} *a.msi", hex(&good).to_uppercase()), "a.msi"), Ok(good));
        // Missing: no line, an empty list, only a similar name.
        assert_eq!(checksum_for(&sums, "pedeefe-2-windows-x64.msi"), Err(UpdateError::NoChecksum));
        assert_eq!(checksum_for("", "a.msi"), Err(UpdateError::NoChecksum));
        assert_eq!(checksum_for(&format!("{}  a.msi.part", hex(&good)), "a.msi"), Err(UpdateError::NoChecksum));
        // Malformed or contradictory lines for the file refuse it.
        assert_eq!(checksum_for("abc123  a.msi", "a.msi"), Err(UpdateError::NoChecksum));
        assert_eq!(checksum_for(&format!("{}  a.msi", "g".repeat(64)), "a.msi"), Err(UpdateError::NoChecksum));
        assert_eq!(checksum_for(&format!("{}  a.msi\n{}  a.msi", hex(&good), hex(&other)), "a.msi"), Err(UpdateError::NoChecksum));
        assert_eq!(checksum_for(&format!("{}  a.msi\n{}  a.msi", hex(&good), hex(&good)), "a.msi"), Ok(good));
        // Multi-byte text never splits badly.
        assert_eq!(checksum_for("ééééé  a.msi\n€", "a.msi"), Err(UpdateError::NoChecksum));
    }

    #[test]
    fn downloads_are_verified_capped_and_cancellable() {
        let data = vec![7u8; 200_000];
        let expected = sha(&data);
        // Good: every byte arrives and matches.
        let progress = Progress::default();
        let mut out = Vec::new();
        assert_eq!(copy_verified(&data[..], &mut out, &expected, Some(200_000), MAX_INSTALLER, &progress), Ok(200_000));
        assert_eq!(out, data);
        assert_eq!((progress.done(), progress.total()), (200_000, Some(200_000)));
        // Bad: one byte differs.
        let mut tampered = data.clone();
        tampered[100] = 8;
        assert_eq!(copy_verified(&tampered[..], &mut Vec::new(), &expected, None, MAX_INSTALLER, &Progress::default()), Err(UpdateError::Mismatch));
        // Cut short of the announced size.
        assert!(matches!(
            copy_verified(&data[..1000], &mut Vec::new(), &expected, Some(200_000), MAX_INSTALLER, &Progress::default()),
            Err(UpdateError::Network(_))
        ));
        // Too large: announced, or found while streaming.
        assert_eq!(
            copy_verified(&data[..], &mut Vec::new(), &expected, Some(MAX_INSTALLER + 1), MAX_INSTALLER, &Progress::default()),
            Err(UpdateError::TooLarge)
        );
        assert_eq!(copy_verified(&data[..], &mut Vec::new(), &expected, None, 1000, &Progress::default()), Err(UpdateError::TooLarge));
        // Cancelled.
        let progress = Progress::default();
        progress.cancel();
        assert_eq!(copy_verified(&data[..], &mut Vec::new(), &expected, None, MAX_INSTALLER, &progress), Err(UpdateError::Cancelled));
        // A full disk.
        struct Full;
        impl Write for Full {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("no space left"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        assert!(matches!(copy_verified(&data[..], &mut Full, &expected, None, MAX_INSTALLER, &Progress::default()), Err(UpdateError::Disk(_))));
    }

    #[test]
    fn a_release_without_a_checksum_or_from_elsewhere_is_never_downloaded() {
        // Refused before any network access.
        let installer = asset("pedeefe-1-windows-x64.msi");
        let release = Release { version: "v1".into(), url: RELEASES_PAGE.into(), installer: Some(installer.clone()), checksums: None };
        assert_eq!(download(&release, &Progress::default()), Err(UpdateError::NoChecksum));
        let elsewhere = Asset { url: "https://example.com/pedeefe-1-windows-x64.msi".into(), ..installer.clone() };
        let release = Release { installer: Some(elsewhere), checksums: Some(asset("SHA256SUMS")), ..release };
        assert_eq!(download(&release, &Progress::default()), Err(UpdateError::NotAllowed));
        let renamed = Asset { name: "other.msi".into(), ..installer };
        let release = Release { installer: Some(renamed), ..release };
        assert_eq!(download(&release, &Progress::default()), Err(UpdateError::NotAllowed));
        assert_eq!(download(&Release { installer: None, ..release }, &Progress::default()), Err(UpdateError::NoInstaller));
    }

    #[test]
    fn the_installer_gets_its_values_through_the_environment() {
        let msi = Path::new(r#"C:\Users\João Tomás\AppData\Local\Temp\pedeefe-update\pedeefe-1 $(evil) `x'; & del.msi"#);
        let plan = install_plan(
            Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"),
            Path::new(r"C:\Temp\install-update.ps1"),
            msi,
            Path::new(r"C:\Temp\install-update.log"),
            Path::new(r"C:\Program Files\PeDeeFe\pedeefe.exe"),
            4242,
        )
        .unwrap();
        let args: Vec<String> = plan.args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(
            args,
            ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File", r"C:\Temp\install-update.ps1"]
        );
        assert!(args.iter().all(|a| !a.contains("evil")), "the installer path is never on the command line");
        let env: std::collections::HashMap<&str, String> = plan.env.iter().map(|(k, v)| (*k, v.to_string_lossy().into_owned())).collect();
        assert_eq!(env["PEDEEFE_UPDATE_MSI"], msi.to_string_lossy());
        assert_eq!(env["PEDEEFE_UPDATE_PID"], "4242");
        assert_eq!(env["PEDEEFE_UPDATE_EXE"], r"C:\Program Files\PeDeeFe\pedeefe.exe");
        // The script reads every value from the environment and quotes the paths it hands on.
        for var in ["$env:PEDEEFE_UPDATE_MSI", "$env:PEDEEFE_UPDATE_PID", "$env:PEDEEFE_UPDATE_EXE", "$env:PEDEEFE_UPDATE_LOG"] {
            assert!(INSTALL_SCRIPT.contains(var), "{var}");
        }
        assert!(INSTALL_SCRIPT.contains(r#"('"' + $msi + '"'), '/passive', '/norestart'"#));
        assert!(INSTALL_SCRIPT.is_ascii(), "Windows PowerShell reads a script without a BOM as ANSI");
        // Paths a Windows file name can't hold are refused.
        for bad in [r#"C:\a"b.msi"#, "C:\\a\nb.msi", ""] {
            let r = install_plan(Path::new("powershell.exe"), Path::new("s.ps1"), Path::new(bad), Path::new("l.log"), Path::new("p.exe"), 1);
            assert!(matches!(r, Err(UpdateError::Launch(_))), "{bad:?}");
        }
    }

    /// Live: asks GitHub over TLS with the OS's roots (`cargo test -p pdfcraft -- --ignored`).
    #[test]
    #[ignore = "needs network access"]
    fn github_answers_with_the_latest_release() {
        let r = latest_release().unwrap();
        assert!(pdfcraft_ui_egui::updates::is_newer(&r.version, "0.0.0"), "{r:?}");
        assert!(r.url.starts_with(RELEASES_PAGE), "{r:?}");
    }
}
