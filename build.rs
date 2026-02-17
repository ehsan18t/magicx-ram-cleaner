//! Build script — embeds a Windows manifest, application icon, and version metadata.

use embed_manifest::manifest::ExecutionLevel;
use embed_manifest::{embed_manifest, new_manifest};

fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        // Embed UAC admin-elevation manifest
        let manifest = new_manifest("MagicX.RAMCleaner")
            .requested_execution_level(ExecutionLevel::RequireAdministrator);
        embed_manifest(manifest).expect("unable to embed manifest file");

        // Embed application icon and version metadata
        // Version info fields are read from [package] and [package.metadata.winresource]
        // in Cargo.toml automatically.
        winresource::WindowsResource::new()
            .set_icon("assets/app.ico")
            .compile()
            .expect("unable to compile Windows resource file");
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/app.ico");
}
