//! Shared filesystem helpers for MataWin examples.
//!
//! This module is private to the example binaries. It resolves the Cargo
//! workspace manifest directory at runtime without depending on the process's
//! current working directory.

use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

/// Finds the nearest ancestor Cargo manifest that declares a `[workspace]`
/// table and returns that manifest's directory.
///
/// If the package is built outside a Cargo workspace, this falls back to the
/// package manifest directory (`CARGO_MANIFEST_DIR`).
///
/// Visibility: **private to examples that import this module**.
pub(crate) fn workspace_manifest_dir() -> PathBuf {
    let package_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    // Permit a standalone checkout to keep the requested asset directories
    // beside the package manifest.
    if package_dir.join("images").is_dir() || package_dir.join("audio").is_dir() {
        return package_dir;
    }

    for directory in package_dir.ancestors() {
        let manifest = directory.join("Cargo.toml");
        let Ok(contents) = fs::read_to_string(&manifest) else {
            continue;
        };

        if contents.lines().any(|line| line.trim() == "[workspace]") {
            return directory.to_path_buf();
        }
    }

    // A normal multi-crate checkout keeps packages directly under the
    // workspace root. This fallback still matches that layout if the root
    // manifest is generated or otherwise unavailable at runtime.
    package_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(package_dir)
}

/// Recursively collects files below `directory` for which `accept` returns
/// `true`. The resulting list is sorted by its case-insensitive path so example
/// playback/navigation order is stable across runs.
///
/// Visibility: **private to examples that import this module**.
pub(crate) fn collect_files(
    directory: &Path,
    accept: impl Fn(&Path) -> bool + Copy,
) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files_into(directory, accept, &mut files)?;
    files.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());
    Ok(files)
}

/// Recursive implementation used by [`collect_files`].
///
/// Visibility: **private to this module**.
fn collect_files_into(
    directory: &Path,
    accept: impl Fn(&Path) -> bool + Copy,
    files: &mut Vec<PathBuf>,
) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            collect_files_into(&path, accept, files)?;
        } else if file_type.is_file() && accept(&path) {
            files.push(path);
        }
    }

    Ok(())
}
