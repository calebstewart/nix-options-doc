use std::path::PathBuf;
use tempfile::TempDir;

/// Creates a fresh temp directory containing a single `.nix` file with the
/// given contents, and returns the directory (kept alive for the caller).
pub(crate) fn temp_dir_with_nix_file(contents: &str) -> TempDir {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    std::fs::write(temp_dir.path().join("module.nix"), contents)
        .expect("failed to write temp nix file");
    temp_dir
}

/// Path to the built `nix-options-doc` binary under test.
pub(crate) fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_nix-options-doc"))
}

/// Sets `path`'s permission bits. Unix-only: the permission-based tests
/// below cannot be expressed on Windows.
#[cfg(unix)]
pub(crate) fn set_mode(path: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .expect("failed to set permissions");
}

/// True when a `0o000` directory really is unreadable for this process.
/// Running as root (or with `CAP_DAC_OVERRIDE`) it is not, and the
/// permission-based tests have nothing to exercise, so they skip instead
/// of failing.
#[cfg(unix)]
pub(crate) fn permissions_are_enforced(dir: &std::path::Path) -> bool {
    std::fs::read_dir(dir).is_err()
}
