fn main() {
    let operating_system =
        std::env::var("CARGO_CFG_TARGET_OS").expect("target operating system is unavailable");
    if operating_system != "windows" {
        return;
    }

    let architecture =
        std::env::var("CARGO_CFG_TARGET_ARCH").expect("target architecture is unavailable");
    assert_eq!(
        architecture, "x86_64",
        "ClipShare's patched WebView2 loader currently supports x64 Windows only"
    );

    let loader_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("x64");
    let loader = loader_dir.join("WebView2LoaderStatic.lib");
    assert!(
        loader.is_file(),
        "Win7-compatible WebView2LoaderStatic.lib is missing"
    );

    println!("cargo:rerun-if-changed={}", loader.display());
    println!("cargo:rustc-link-search=native={}", loader_dir.display());
    println!("cargo:rustc-link-lib=advapi32");
}
