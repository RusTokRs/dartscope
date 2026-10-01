//! Reading the files of a project from the filesystem for the stdio server.
//!
//! [`FsWorkspace`] is the one place of this crate that touches the filesystem, and it only reads. It
//! applies the same rules as the CLI walker (`dartscope analyze-project`): tool state (`.git`,
//! `.dart_tool`, ...) and dependency directories are not entered, `build`, `coverage` and `target`
//! only inside the source roots (`lib`, `bin`, `test`, ...), and symbolic links are never followed.
//! The limits keep a huge checkout from stalling the editor: 20,000 Dart files, 128 MiB of source,
//! 250,000 directory entries, and no Dart file over [`MAX_NAVIGATION_BYTES`].

use std::fs;
use std::path::{Path, PathBuf};

use crate::server::{MAX_NAVIGATION_BYTES, WorkspaceFile, WorkspaceScan, WorkspaceSource};

/// The most Dart files a scan loads.
pub const MAX_WORKSPACE_FILES: usize = 20_000;

/// The most source text, in bytes, a scan loads.
pub const MAX_WORKSPACE_BYTES: u64 = 128 * 1024 * 1024;

/// The most directory entries a scan looks at.
pub const MAX_DIRECTORY_ENTRIES: usize = 250_000;

/// The largest `pubspec.yaml` or `package_config.json` that is read.
const MAX_CONFIG_BYTES: u64 = 8 * 1024 * 1024;

/// A [`WorkspaceSource`] that reads from the filesystem.
#[derive(Debug, Clone, Copy, Default)]
pub struct FsWorkspace;

impl WorkspaceSource for FsWorkspace {
    fn scan(&self, root: &str) -> WorkspaceScan {
        scan_directory(&file_system_path(root))
    }

    fn read(&self, path: &str) -> Option<String> {
        let path = file_system_path(path);
        let limit = size_limit(&path)?;
        read_limited(&path, limit)
    }
}

/// The path as the operating system writes it: the path of a Windows file URI has a `/` before the
/// drive letter (`/C:/proj`), which is not a path there.
fn file_system_path(path: &str) -> PathBuf {
    let bytes = path.as_bytes();
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' {
        return PathBuf::from(&path[1..]);
    }
    PathBuf::from(path)
}

/// How large a file of this kind may be, or `None` for a file the server does not use.
fn size_limit(path: &Path) -> Option<u64> {
    let name = path.file_name()?.to_str()?;
    if name.ends_with(".dart") {
        Some(MAX_NAVIGATION_BYTES as u64)
    } else if name == "pubspec.yaml" || name == "package_config.json" {
        Some(MAX_CONFIG_BYTES)
    } else {
        None
    }
}

/// The text of a file, unless it is larger than `limit` bytes or is not UTF-8.
fn read_limited(path: &Path, limit: u64) -> Option<String> {
    if fs::metadata(path).ok()?.len() > limit {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn scan_directory(root: &Path) -> WorkspaceScan {
    let mut scan = WorkspaceScan::default();
    let mut pending = vec![root.to_path_buf()];
    let mut entries_seen = 0usize;
    let mut dart_files = 0usize;
    let mut bytes = 0u64;
    let mut too_large = 0usize;
    'walk: while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(fs::DirEntry::file_name);
        let mut subdirectories = Vec::new();
        let mut has_pubspec = false;
        for entry in entries {
            entries_seen += 1;
            if entries_seen > MAX_DIRECTORY_ENTRIES {
                scan.notes.push(format!(
                    "the workspace scan stopped after {MAX_DIRECTORY_ENTRIES} directory entries; files beyond them are unknown to dartscope"
                ));
                break 'walk;
            }
            // `file_type` does not follow symbolic links: a link is neither a file nor a directory.
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                if !is_skipped_directory(root, &path) {
                    subdirectories.push(path);
                }
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            let Some(limit) = size_limit(&path) else {
                continue;
            };
            let is_dart = path
                .extension()
                .is_some_and(|extension| extension == "dart");
            if is_dart {
                if dart_files >= MAX_WORKSPACE_FILES || bytes >= MAX_WORKSPACE_BYTES {
                    scan.notes.push(format!(
                        "the workspace scan stopped at {dart_files} Dart files and {} MiB of source; files beyond them are unknown to dartscope",
                        bytes / (1024 * 1024)
                    ));
                    break 'walk;
                }
            } else if path.file_name().is_some_and(|name| name == "pubspec.yaml") {
                has_pubspec = true;
            }
            if fs::metadata(&path).is_ok_and(|metadata| metadata.len() > limit) {
                if is_dart {
                    too_large += 1;
                }
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            if is_dart {
                dart_files += 1;
                bytes += text.len() as u64;
            }
            scan.files.push(WorkspaceFile {
                path: slashes(&path),
                text,
            });
        }
        if has_pubspec {
            let config = directory.join(".dart_tool").join("package_config.json");
            if let Some(text) = read_limited(&config, MAX_CONFIG_BYTES) {
                scan.files.push(WorkspaceFile {
                    path: slashes(&config),
                    text,
                });
            }
        }
        // Visit the subdirectories in name order: the stack gives the last one pushed first.
        pending.extend(subdirectories.into_iter().rev());
    }
    if too_large > 0 {
        scan.notes.push(format!(
            "{too_large} Dart files larger than {} KiB are not part of workspace navigation",
            MAX_NAVIGATION_BYTES / 1024
        ));
    }
    scan
}

fn slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Directories that hold tool state, dependencies or generated output rather than project sources.
///
/// The same rule as the walker of the `dartscope` CLI. Build, coverage and cargo output directories
/// sit next to a package, but the same names are ordinary folders inside the source roots, where
/// skipping them would hide sources.
fn is_skipped_directory(root: &Path, path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    match name {
        ".dart_tool" | ".git" | ".idea" | ".pub-cache" | ".vscode" | ".symlinks"
        | ".plugin_symlinks" | "Pods" | "node_modules" => true,
        "build" | "coverage" | "target" => !is_inside_source_root(root, path),
        _ => false,
    }
}

fn is_inside_source_root(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root)
        .ok()
        .and_then(Path::parent)
        .is_some_and(|parent| {
            parent.components().any(|component| {
                matches!(
                    component.as_os_str().to_str(),
                    Some(
                        "lib"
                            | "bin"
                            | "test"
                            | "test_driver"
                            | "tool"
                            | "integration_test"
                            | "benchmark"
                    )
                )
            })
        })
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    /// A scratch directory that removes itself.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos());
            let path = std::env::temp_dir().join(format!(
                "dartscope-lsp-{name}-{}-{nanos}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, relative: &str, text: &[u8]) {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn relative_paths(scan: &WorkspaceScan, root: &Path) -> Vec<String> {
        let prefix = format!("{}/", slashes(root));
        scan.files
            .iter()
            .map(|file| {
                file.path
                    .strip_prefix(&prefix)
                    .unwrap_or(&file.path)
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn the_scan_follows_the_rules_of_the_cli_walker() {
        let root = Scratch::new("rules");
        root.write("pubspec.yaml", b"name: app\n");
        root.write(".dart_tool/package_config.json", b"{}");
        root.write(".dart_tool/hidden.dart", b"class Hidden {}");
        root.write("lib/a.dart", b"class A {}");
        root.write("lib/build/inside.dart", b"class Inside {}");
        root.write("lib/src/b.dart", b"class B {}");
        root.write("test/a_test.dart", b"void main() {}");
        root.write("build/outside.dart", b"class Outside {}");
        root.write("node_modules/x/dep.dart", b"class Dep {}");
        root.write(".git/hook.dart", b"class Hook {}");
        root.write("README.md", b"# app");
        root.write("lib/not_utf8.dart", &[0xff, 0xfe, 0x00]);

        let scan = FsWorkspace.scan(&slashes(&root.0));

        // The files of a directory come first, in name order, then its subdirectories in name order.
        assert_eq!(
            relative_paths(&scan, &root.0),
            [
                "pubspec.yaml",
                ".dart_tool/package_config.json",
                "lib/a.dart",
                "lib/build/inside.dart",
                "lib/src/b.dart",
                "test/a_test.dart",
            ]
        );
        assert!(scan.notes.is_empty(), "{:?}", scan.notes);
    }

    #[test]
    fn a_missing_directory_has_no_files_and_nothing_to_say() {
        let scan = FsWorkspace.scan("/dartscope-lsp-this-directory-does-not-exist");
        assert!(scan.files.is_empty());
        assert!(scan.notes.is_empty());
    }

    #[test]
    fn a_dart_file_over_the_limit_is_left_out_with_a_note() {
        let root = Scratch::new("large");
        root.write("lib/small.dart", b"class Small {}");
        let mut big = String::new();
        while big.len() <= MAX_NAVIGATION_BYTES {
            big.push_str("class Big {}\n");
        }
        root.write("lib/big.dart", big.as_bytes());

        let scan = FsWorkspace.scan(&slashes(&root.0));

        assert_eq!(relative_paths(&scan, &root.0), ["lib/small.dart"]);
        assert_eq!(scan.notes.len(), 1, "{:?}", scan.notes);
        assert!(scan.notes[0].contains("1 Dart files larger than"));
        assert_eq!(
            FsWorkspace.read(&slashes(&root.0.join("lib/big.dart"))),
            None
        );
        assert_eq!(
            FsWorkspace
                .read(&slashes(&root.0.join("lib/small.dart")))
                .as_deref(),
            Some("class Small {}")
        );
    }

    #[test]
    fn a_windows_file_uri_path_is_a_drive_path() {
        assert_eq!(
            file_system_path("/C:/proj/a.dart"),
            PathBuf::from("C:/proj/a.dart")
        );
        assert_eq!(
            file_system_path("/work/a.dart"),
            PathBuf::from("/work/a.dart")
        );
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_are_not_followed() {
        let root = Scratch::new("links");
        root.write("lib/real.dart", b"class Real {}");
        root.write("outside/secret.dart", b"class Secret {}");
        std::os::unix::fs::symlink(root.0.join("outside"), root.0.join("lib/linked")).unwrap();
        std::os::unix::fs::symlink(
            root.0.join("outside/secret.dart"),
            root.0.join("lib/secret_link.dart"),
        )
        .unwrap();

        let scan = FsWorkspace.scan(&slashes(&root.0.join("lib")));

        assert_eq!(relative_paths(&scan, &root.0.join("lib")), ["real.dart"]);
    }
}
