use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[test]
fn help_version_and_command_help_are_stable() {
    for args in [vec!["--help"], vec!["-h"], vec!["help"]] {
        let output = run(args);
        assert_success_text(&output, "USAGE:");
        assert!(stdout(&output).contains("COMMANDS:"));
    }

    let version = run(["--version"]);
    assert_success_text(&version, concat!("dartscope ", env!("CARGO_PKG_VERSION")));
    assert_eq!(
        stdout(&version).trim(),
        concat!("dartscope ", env!("CARGO_PKG_VERSION"))
    );

    for command in command_names() {
        let help = run(["help", command]);
        assert_success_text(&help, &format!("dartscope {command} <path>"));

        let inline_help = run([command, "--help"]);
        assert_success_text(&inline_help, &format!("dartscope {command} <path>"));
    }
}

#[test]
fn usage_and_input_errors_use_stable_exit_codes() {
    assert_error(run(std::iter::empty::<&str>()), 2, "missing command");
    assert_error(run(["unknown-command"]), 2, "unknown command");
    assert_error(run(["analyze-file"]), 2, "missing path");
    assert_error(
        run(["uri-graph", ".", "--env"]),
        2,
        "missing value for --env",
    );
    assert_error(
        run(["uri-graph", ".", "--env", "missing-equals"]),
        2,
        "expected --env key=value",
    );
    assert_error(
        run(["uri-graph", ".", "--env", "=true"]),
        2,
        "key cannot be empty",
    );

    let temp = TempDirectory::new("errors");
    let source = temp.path().join("source.dart");
    write_file(&source, "void main() {}\n");
    assert_error(
        run_os([
            OsString::from("analyze-file"),
            source.as_os_str().to_owned(),
            OsString::from("extra"),
        ]),
        2,
        "unexpected argument",
    );
    assert_error(
        run_os([
            OsString::from("analyze-file"),
            temp.path().join("missing.dart").into_os_string(),
        ]),
        3,
        "failed to read",
    );
    assert_error(
        run_os([OsString::from("analyze-project"), source.into_os_string()]),
        3,
        "project root is not a directory",
    );
}

#[test]
fn all_json_commands_write_only_versioned_json_to_stdout() {
    let project = sample_project("all commands with spaces");
    let dart_file = project.path().join("lib/main.dart");
    let pubspec = project.path().join("pubspec.yaml");

    let commands = [
        (
            vec![OsString::from("analyze-file"), dart_file.into_os_string()],
            "dartscope.file-analysis",
        ),
        (
            vec![OsString::from("pubspec"), pubspec.as_os_str().to_owned()],
            "dartscope.pubspec-analysis",
        ),
        (
            vec![
                OsString::from("pubspec-config"),
                pubspec.as_os_str().to_owned(),
            ],
            "dartscope.pubspec-configuration",
        ),
        (
            vec![
                OsString::from("analyze-project"),
                project.path().as_os_str().to_owned(),
            ],
            "dartscope.project-analysis",
        ),
        (
            vec![
                OsString::from("graphql-contracts"),
                project.path().as_os_str().to_owned(),
                OsString::from("--env"),
                OsString::from("dart.library.io=true"),
            ],
            "dartscope.graphql-contracts",
        ),
        (
            vec![
                OsString::from("uri-graph"),
                project.path().as_os_str().to_owned(),
                OsString::from("--env"),
                OsString::from("dart.library.io=true"),
                OsString::from("--env"),
                OsString::from("dart.library.html=false"),
            ],
            "dartscope.uri-graph",
        ),
        (
            vec![
                OsString::from("flutter-inventory"),
                project.path().as_os_str().to_owned(),
            ],
            "dartscope.flutter-inventory",
        ),
    ];

    for (args, schema) in commands {
        let output = run_os(args);
        assert_json_success(&output, schema);
    }
}

#[test]
fn malformed_inputs_never_panic() {
    let project = TempDirectory::new("malformed inputs");
    let dart_file = project.path().join("lib/broken.dart");
    let pubspec = project.path().join("pubspec.yaml");
    write_file(&dart_file, "class { unterminated(\n");
    write_file(&pubspec, "flutter: [unterminated\n");

    let commands = [
        (
            vec![OsString::from("analyze-file"), dart_file.into_os_string()],
            "dartscope.file-analysis",
        ),
        (
            vec![OsString::from("pubspec"), pubspec.as_os_str().to_owned()],
            "dartscope.pubspec-analysis",
        ),
        (
            vec![
                OsString::from("pubspec-config"),
                pubspec.as_os_str().to_owned(),
            ],
            "dartscope.pubspec-configuration",
        ),
        (
            vec![
                OsString::from("analyze-project"),
                project.path().as_os_str().to_owned(),
            ],
            "dartscope.project-analysis",
        ),
        (
            vec![
                OsString::from("graphql-contracts"),
                project.path().as_os_str().to_owned(),
            ],
            "dartscope.graphql-contracts",
        ),
        (
            vec![
                OsString::from("uri-graph"),
                project.path().as_os_str().to_owned(),
            ],
            "dartscope.uri-graph",
        ),
        (
            vec![
                OsString::from("flutter-inventory"),
                project.path().as_os_str().to_owned(),
            ],
            "dartscope.flutter-inventory",
        ),
    ];

    for (args, schema) in commands {
        assert_json_success(&run_os(args), schema);
    }
}

#[test]
fn flutter_inventory_reads_l10n_and_arb_catalogs() {
    let project = TempDirectory::new("flutter catalogs");
    write_file(
        &project.path().join("pubspec.yaml"),
        concat!(
            "name: catalog_demo\n",
            "flutter:\n",
            "  generate: true\n",
            "  assets:\n",
            "    - assets/logo.png\n",
            "    - assets/unused.png\n",
        ),
    );
    write_file(
        &project.path().join("l10n.yaml"),
        concat!(
            "arb-dir: lib/l10n\n",
            "template-arb-file: app_en.arb\n",
            "output-localization-file: app_localizations.dart\n",
        ),
    );
    write_file(
        &project.path().join("lib/l10n/app_en.arb"),
        r#"{"title":"Title"}"#,
    );
    write_file(
        &project.path().join("lib/main.dart"),
        concat!(
            "void build(context) {\n",
            "  Image.asset('assets/logo.png');\n",
            "  Image.asset('assets/missing.png');\n",
            "  AppLocalizations.of(context).title;\n",
            "  AppLocalizations.of(context).missing;\n",
            "}\n",
        ),
    );

    let output = run_os([
        OsString::from("flutter-inventory"),
        project.path().as_os_str().to_owned(),
    ]);
    assert_json_success(&output, "dartscope.flutter-inventory");
    let json = stdout(&output);

    assert!(json.contains("\"asset_declarations\""), "stdout: {json}");
    assert!(json.contains("\"arb_catalogs\""), "stdout: {json}");
    assert!(
        json.contains("flutter_asset_used_but_undeclared"),
        "stdout: {json}"
    );
    assert!(
        json.contains("flutter_asset_declared_but_unused"),
        "stdout: {json}"
    );
    assert!(
        json.contains("flutter_localization_key_missing"),
        "stdout: {json}"
    );
    assert!(json.contains("lib/l10n/app_en.arb"), "stdout: {json}");
}

#[test]
fn non_catalog_commands_ignore_invalid_arb_bytes() {
    let project = sample_project("invalid arb is catalog only");
    let arb = project.path().join("lib/l10n/app_en.arb");
    fs::create_dir_all(arb.parent().expect("ARB parent")).expect("create ARB directory");
    fs::write(&arb, [0xff, 0xfe]).expect("write invalid UTF-8 ARB");

    assert_json_success(
        &run_os([
            OsString::from("analyze-project"),
            project.path().as_os_str().to_owned(),
        ]),
        "dartscope.project-analysis",
    );
    assert_error(
        run_os([
            OsString::from("flutter-inventory"),
            project.path().as_os_str().to_owned(),
        ]),
        3,
        "failed to read",
    );
}

#[test]
fn project_discovery_handles_nested_packages_and_generated_directories() {
    let project = TempDirectory::new("nested project with spaces");
    write_package(project.path(), "root_package", "lib/root.dart");
    write_package(
        &project.path().join("packages/nested package"),
        "nested_package",
        "lib/nested.dart",
    );

    for directory in [
        ".git",
        ".idea",
        ".pub-cache",
        ".vscode",
        "build",
        "coverage",
        "node_modules",
        "Pods",
        "target",
    ] {
        write_file(
            &project.path().join(directory).join("ignored.dart"),
            "void ignored() {}\n",
        );
    }
    let output = run_os([
        OsString::from("analyze-project"),
        project.path().as_os_str().to_owned(),
    ]);
    assert_json_success(&output, "dartscope.project-analysis");
    let json = stdout(&output);

    assert!(json.contains("lib/root.dart"));
    assert!(json.contains("packages/nested package/lib/nested.dart"));
    assert!(json.contains(".dart_tool/package_config.json"));
    assert!(json.contains("packages/nested package/.dart_tool/package_config.json"));
    assert!(!json.contains("ignored.dart"));
}

#[cfg(unix)]
#[test]
fn project_discovery_rejects_external_symlink_directories() {
    use std::os::unix::fs::symlink;

    let project = TempDirectory::new("external symlink project");
    write_package(project.path(), "root_package", "lib/root.dart");
    let external = TempDirectory::new("external symlink target");
    write_file(&external.path().join("linked.dart"), "void linked() {}\n");
    symlink(external.path(), project.path().join("linked-source")).expect("create symlink");

    assert_error(
        run_os([
            OsString::from("analyze-project"),
            project.path().as_os_str().to_owned(),
        ]),
        3,
        "input_symlink_rejected",
    );
}

fn command_names() -> [&'static str; 7] {
    [
        "analyze-file",
        "pubspec",
        "pubspec-config",
        "analyze-project",
        "graphql-contracts",
        "uri-graph",
        "flutter-inventory",
    ]
}

fn sample_project(label: &str) -> TempDirectory {
    let project = TempDirectory::new(label);
    write_package(project.path(), "sample", "lib/main.dart");
    write_file(
        &project.path().join("lib/main.dart"),
        concat!(
            "import 'stub.dart' if (dart.library.io) 'io.dart';\n",
            "const query = r'''query Viewer { viewer { id } }''';\n",
            "void main() {}\n",
        ),
    );
    write_file(&project.path().join("lib/stub.dart"), "class Platform {}\n");
    write_file(&project.path().join("lib/io.dart"), "class Platform {}\n");
    project
}

fn write_package(root: &Path, package_name: &str, dart_path: &str) {
    write_file(
        &root.join("pubspec.yaml"),
        &format!(
            "name: {package_name}\nenvironment:\n  sdk: ^3.4.0\ndependencies:\n  flutter:\n    sdk: flutter\n"
        ),
    );
    write_file(&root.join(dart_path), "void main() {}\n");
    write_file(
        &root.join(".dart_tool/package_config.json"),
        &format!(
            concat!(
                "{{\n",
                "  \"configVersion\": 2,\n",
                "  \"packages\": [\n",
                "    {{\"name\": \"{}\", \"rootUri\": \"../\", ",
                "\"packageUri\": \"lib/\", \"languageVersion\": \"3.4\"}}\n",
                "  ]\n",
                "}}\n"
            ),
            package_name
        ),
    );
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture directory");
    }
    fs::write(path, contents).expect("write fixture file");
}

fn run<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(env!("CARGO_BIN_EXE_dartscope"))
        .args(args)
        .output()
        .expect("run dartscope")
}

fn run_os(args: impl IntoIterator<Item = OsString>) -> Output {
    run(args)
}

fn assert_success_text(output: &Output, expected: &str) {
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(output));
    assert!(stderr(output).is_empty(), "stderr: {}", stderr(output));
    assert!(
        stdout(output).contains(expected),
        "stdout: {}",
        stdout(output)
    );
}

fn assert_json_success(output: &Output, schema: &str) {
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(output));
    assert!(stderr(output).is_empty(), "stderr: {}", stderr(output));
    let stdout = stdout(output);
    let json = stdout.trim();
    assert!(
        json.starts_with('{') && json.ends_with('}'),
        "stdout: {stdout}"
    );
    assert_eq!(
        json.matches(&format!("\"schema\": \"{schema}\"")).count(),
        1,
        "stdout: {stdout}"
    );
    assert_eq!(
        json.matches("\"version\": 1").count(),
        1,
        "stdout: {stdout}"
    );
    assert_eq!(json.matches("\"data\":").count(), 1, "stdout: {stdout}");
}

fn assert_error(output: Output, exit_code: i32, expected: &str) {
    assert_eq!(
        output.status.code(),
        Some(exit_code),
        "stderr: {}",
        stderr(&output)
    );
    assert!(stdout(&output).is_empty(), "stdout: {}", stdout(&output));
    let stderr = stderr(&output);
    assert!(stderr.starts_with("error: "), "stderr: {stderr}");
    assert!(stderr.contains(expected), "stderr: {stderr}");
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout must be UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr must be UTF-8")
}

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let sanitized = label.replace(|character: char| !character.is_ascii_alphanumeric(), "-");
        let path = std::env::temp_dir().join(format!(
            "dartscope-cli-{sanitized}-{}-{sequence}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temporary directory");
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
