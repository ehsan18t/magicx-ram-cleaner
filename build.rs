//! Build script — embeds a Windows manifest, application icon, and version metadata.

use embed_manifest::manifest::ExecutionLevel;
use embed_manifest::{embed_manifest, new_manifest};

fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        // Embed UAC admin-elevation manifest
        let manifest = new_manifest("MagicX.RAMCleaner")
            .requested_execution_level(ExecutionLevel::RequireAdministrator);
        embed_manifest(manifest).expect("unable to embed manifest file");

        // Embed application icon, additional menu icons, and version metadata.
        // Version info fields are read from [package] and [package.metadata.winresource]
        // in Cargo.toml automatically.
        //
        // Icon resource IDs (used by context_menu.rs for registry Icon values):
        //   1 = app.ico        (main application icon + root menu / purge / status)
        //   2 = lite.ico        (gentle / moderate entries)
        //   3 = aggressive.ico  (aggressive entry)
        //
        // To add a new icon: call .set_icon_with_id() with the next sequential ID,
        // add a cargo:rerun-if-changed line, and reference the ID in context_menu.rs.
        // Explicitly set FileDescription to override the automatic
        // CARGO_PKG_DESCRIPTION mapping, which would otherwise show the
        // long crate description in Task Manager's app-group header.
        winresource::WindowsResource::new()
            .set("FileDescription", "MagicX RAM Cleaner")
            .set_icon_with_id("assets/app.ico", "1")
            .set_icon_with_id("assets/lite.ico", "2")
            .set_icon_with_id("assets/aggressive.ico", "3")
            .compile()
            .expect("unable to compile Windows resource file");
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=assets/app.ico");
    println!("cargo:rerun-if-changed=assets/lite.ico");
    println!("cargo:rerun-if-changed=assets/aggressive.ico");
}
