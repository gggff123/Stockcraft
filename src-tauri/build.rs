fn main() {
    tauri_build::build();

    // `tauri-build` embeds the Windows resource archive (icon, version info and the
    // Common-Controls v6 manifest) with `cargo:rustc-link-arg-bins`, so test binaries
    // inherit none of it. Without the v6 manifest the loader resolves comctl32 v5,
    // which does not export `TaskDialogIndirect` (imported by muda/tauri-runtime-wry),
    // and the test executable dies with STATUS_ENTRYPOINT_NOT_FOUND before main().
    // Link the very same archive into every target. There is no Cargo directive for "library
    // test only": `rustc-link-arg-tests` does not reach the lib-test harness, while plain
    // `rustc-link-arg` does but also links the archive into the bin a second time (MinGW then
    // prints cosmetic duplicate-resource warnings). The bin still contains exactly one usable
    // manifest, and every test executable can start.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo");
        let archive_name = if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            "resource.lib"
        } else {
            "libresource.a"
        };
        let resource = std::path::Path::new(&out_dir).join(archive_name);
        if resource.exists() {
            println!("cargo:rustc-link-arg={}", resource.display());
        } else {
            println!(
                "cargo:warning=Windows resource archive not found at {}; test binaries may fail to start",
                resource.display()
            );
        }
    }
}
