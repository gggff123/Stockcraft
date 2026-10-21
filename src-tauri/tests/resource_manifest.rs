//! Windows smoke test for the embedded application manifest.
//!
//! The test executable itself must start successfully and contain the Common-Controls v6
//! manifest. Without it the loader resolves comctl32 v5 and the process dies before any test can
//! run (`STATUS_ENTRYPOINT_NOT_FOUND`).

#[test]
fn test_binary_contains_common_controls_manifest() {
    if !cfg!(windows) {
        return;
    }
    let exe = std::env::current_exe().expect("test executable path");
    let bytes = std::fs::read(exe).expect("read test executable");
    let needle = b"Microsoft.Windows.Common-Controls";
    assert!(
        bytes.windows(needle.len()).any(|window| window == needle),
        "the test executable is missing the Common-Controls v6 manifest"
    );
}
