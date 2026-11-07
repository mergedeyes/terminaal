//! Windows only: puts the icon and the version information into
//! `terminaal.exe` -- what Explorer, the taskbar, the Start menu entry and
//! Settings → Apps show, and what the file's Properties → Details list.
//! The resource script is written here, with the version from Cargo.toml,
//! and compiled by `rc.exe` (MSVC) or `llvm-rc` (cargo xwin from Linux).

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/terminaal.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let script = Path::new(&out).join("terminaal.rc");
    std::fs::write(&script, resource_script(&manifest)).expect("can't write the resource script");
    embed_resource::compile(&script, embed_resource::NONE).manifest_required().expect("can't compile the resources");
}

/// The icon (ID 1: the one Windows shows for the exe) and a `VERSIONINFO`.
fn resource_script(manifest_dir: &str) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let number = |name: &str| std::env::var(name).ok().and_then(|n| n.parse::<u16>().ok()).unwrap_or(0);
    let (major, minor, patch) =
        (number("CARGO_PKG_VERSION_MAJOR"), number("CARGO_PKG_VERSION_MINOR"), number("CARGO_PKG_VERSION_PATCH"));
    // A string literal in a resource script: backslashes doubled.
    let icon = Path::new(manifest_dir).join("assets").join("terminaal.ico").display().to_string().replace('\\', "\\\\");
    format!(
        r#"#pragma code_page(65001)
1 ICON "{icon}"

1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Terminaal"
      VALUE "FileDescription", "Terminaal"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "terminaal"
      VALUE "LegalCopyright", "GNU General Public License v3"
      VALUE "OriginalFilename", "terminaal.exe"
      VALUE "ProductName", "Terminaal"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    )
}
