fn main() {
    println!("cargo:rustc-check-cfg=cfg(clipshare_win7_loader)");
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
    let loader_dir = if std::env::var("TARGET").as_deref() == Ok("x86_64-win7-windows-msvc") {
        println!("cargo:rustc-cfg=clipshare_win7_loader");
        let output =
            std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR missing"));
        redirect_loader_import(&loader, &output.join("WebView2LoaderStatic.lib"));
        output
    } else {
        loader_dir
    };
    println!("cargo:rustc-link-search=native={}", loader_dir.display());
    println!("cargo:rustc-link-lib=advapi32");
}

fn redirect_loader_import(source: &std::path::Path, output: &std::path::Path) {
    // Rename the two COFF references of the pinned SDK archive's import pointer.
    // Equal-length names preserve the archive offsets and scope the shim to this loader.
    const IMPORT: &[u8; 21] = b"__imp_LoadLibraryExW\0";
    const SHIM: &[u8; 21] = b"ClipShare_LoadSysDll\0";
    let mut archive = std::fs::read(source).expect("read the pinned WebView2 loader");
    let positions: Vec<_> = archive
        .windows(IMPORT.len())
        .enumerate()
        .filter_map(|(offset, bytes)| (bytes == IMPORT).then_some(offset))
        .collect();
    assert_eq!(
        positions.len(),
        2,
        "unexpected WebView2 loader import table"
    );
    for offset in positions {
        archive[offset..offset + IMPORT.len()].copy_from_slice(SHIM);
    }
    std::fs::write(output, archive).expect("write the Win7 WebView2 loader");
}
