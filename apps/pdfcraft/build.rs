//! Windows only: embed the app icon and version info (VERSIONINFO) into `pdfcraft.exe` (shipped
//! as `pedeefe.exe`), so it shows in Explorer, the taskbar, the Start menu and Alt-Tab. A
//! separate PDF document icon (PeDeeFe's, `assets/brand/pedeefe/document`) uses resource ID 2, so
//! `pedeefe.exe,-2` names it; the installer also installs it as `pedeefe-document.ico`.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `PDFCRAFT_REQUIRE_WINRES=1` turns it
//! into an error (for release builds).

#[path = "src/windows_manifest.rs"]
mod windows_manifest;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/pdfcraft.ico");
    println!("cargo:rerun-if-changed=../../assets/brand/pedeefe/document/pedeefe-document.ico");
    println!("cargo:rerun-if-changed=src/windows_manifest.rs");
    println!("cargo:rerun-if-env-changed=PDFCRAFT_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/pdfcraft.ico")
        .set_icon_with_id("../../assets/brand/pedeefe/document/pedeefe-document.ico", "2")
        .set_manifest(windows_manifest::WINDOWS_MANIFEST)
        .set("ProductName", "PeDeeFe")
        .set("FileDescription", "PeDeeFe PDF workbench")
        .set("LegalCopyright", "Copyright (c) 2026 ArtCraft Team and the PdfCraft contributors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "pedeefe.exe")
        .set("InternalName", "pedeefe");
    if let Err(e) = res.compile() {
        if std::env::var_os("PDFCRAFT_REQUIRE_WINRES").is_some() {
            println!("cargo::error=embedding Windows resources failed: {e}");
            return;
        }
        println!("cargo:warning=pdfcraft.exe built without icon/version resources: {e}");
    }
}
