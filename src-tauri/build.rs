fn main() {
    tauri_build::build();
    // Unit tests now reach the native WebView host. TaskDialogIndirect requires
    // Common Controls v6, just as the manifest embedded by Tauri does for the app.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("windows-tests.manifest");
        println!("cargo:rerun-if-changed=windows-tests.manifest");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}
