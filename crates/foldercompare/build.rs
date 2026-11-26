//! Windows only: builds the native Explorer extension (`src/FolderCompare.ShellExt`, MSVC) to embed it, and
//! compiles the icon, manifest and version info into the exe.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(shellext)");
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=FOLDERCOMPARE_NO_SHELLEXT");
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let target = env::var("TARGET").unwrap_or_default();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target == "x86_64-pc-windows-msvc" && cfg!(windows) {
        build_shell_extension(&manifest_dir);
    }
    if target_os == "windows" {
        compile_resources(&manifest_dir);
    }
}

/// Runs `build.ps1` (it serializes parallel builds with a mutex and skips an up-to-date DLL). Without the MSVC
/// Build Tools the app is built without the Explorer integration.
fn build_shell_extension(manifest_dir: &Path) {
    if env::var_os("FOLDERCOMPARE_NO_SHELLEXT").is_some() {
        println!(
            "cargo::warning=FOLDERCOMPARE_NO_SHELLEXT is set: building without the Explorer integration"
        );
        return;
    }
    let dir = manifest_dir.join("../../src/FolderCompare.ShellExt");
    for f in ["ShellExt.cpp", "ShellExt.def", "build.ps1"] {
        println!("cargo::rerun-if-changed={}", dir.join(f).display());
    }
    let status = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(dir.join("build.ps1"))
        .status();
    let dll = dir.join("bin/x64/FolderCompare.ShellExt.dll");
    match status {
        Ok(s) if s.success() && dll.is_file() => {
            println!(
                "cargo::rustc-env=FC_SHELLEXT_DLL={}",
                fs::canonicalize(&dll).unwrap().display()
            );
            println!("cargo::rustc-cfg=shellext");
        }
        _ => println!(
            "cargo::warning=Could not build the Explorer extension (MSVC Build Tools missing?): building without it"
        ),
    }
}

fn compile_resources(manifest_dir: &Path) {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let icon = manifest_dir.join("../../src/FolderCompare.App/Assets/app.ico");
    let manifest = manifest_dir.join("windows/app.manifest");
    println!("cargo::rerun-if-changed={}", manifest.display());
    println!("cargo::rerun-if-changed={}", icon.display());
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let mut parts: Vec<u16> = version.split('.').filter_map(|p| p.parse().ok()).collect();
    parts.resize(4, 0);
    let numeric = format!("{},{},{},{}", parts[0], parts[1], parts[2], parts[3]);
    let path = |p: &Path| {
        fs::canonicalize(p)
            .unwrap()
            .display()
            .to_string()
            .replace('\\', "\\\\")
    };
    let rc = format!(
        r#"#include <winver.h>
1 ICON "{icon}"
1 24 "{manifest}"
VS_VERSION_INFO VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS VOS_NT_WINDOWS32
FILETYPE VFT_APP
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "FolderCompare"
      VALUE "FileDescription", "FolderCompare"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "FolderCompare"
      VALUE "OriginalFilename", "FolderCompare.exe"
      VALUE "ProductName", "FolderCompare"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        icon = path(&icon),
        manifest = path(&manifest),
    );
    let rc_path = out.join("app.rc");
    fs::write(&rc_path, rc).unwrap();
    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_required()
        .unwrap();
}
