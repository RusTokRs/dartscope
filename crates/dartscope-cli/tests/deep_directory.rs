use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "dartscope-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temporary project directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn analyze_project_handles_deep_directory_trees() {
    let project = TempDirectory::new("deep-directory-contract");
    let mut directory = project.path().to_path_buf();
    let mut relative = PathBuf::new();
    for _ in 0..64 {
        directory.push("d");
        relative.push("d");
    }
    fs::create_dir_all(&directory).expect("deep directory tree");
    fs::write(directory.join("deep.dart"), "void deep() {}\n").expect("deep Dart file");

    let output = Command::new(env!("CARGO_BIN_EXE_dartscope"))
        .arg(OsStr::new("analyze-project"))
        .arg(project.path())
        .output()
        .expect("run dartscope");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 CLI output");
    let expected_path = format!(
        "{}/deep.dart",
        relative.to_string_lossy().replace('\\', "/")
    );
    assert!(stdout.contains(&expected_path), "stdout: {stdout}");
}
