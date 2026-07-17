use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[test]
fn lint_help_and_default_configuration_are_inert() {
    let help = run(["lint", "--help"]);
    assert_text_success(&help, "dartscope lint <project>");
    assert!(stdout(&help).contains("--config <path>"));
    assert!(stdout(&help).contains("--format <json|sarif>"));
    assert!(stdout(&help).contains("--deny-warnings"));

    let project = sample_project("default inert");
    let output = run_os([
        OsString::from("lint"),
        project.path().as_os_str().to_owned(),
    ]);
    assert_structured_output(&output, 0, "dartscope.lint-analysis");
    let json = stdout(&output);
    assert!(json.contains("\"enabled_rules\": 0"), "stdout: {json}");
    assert!(json.contains("\"diagnostics\": []"), "stdout: {json}");
}

#[test]
fn toml_configuration_and_deny_warnings_use_stable_exit_codes() {
    let project = sample_project("configured warnings");
    write_file(&project.path().join("lib/BadName.dart"), "class bad_name {}\n");
    let config = project.path().join("dartscope lint.toml");
    write_file(
        &config,
        concat!(
            "version = 1\n",
            "failure_threshold = \"error\"\n",
            "enabled_rules = [\"dartscope.naming_convention\"]\n",
        ),
    );

    let warning_only = run_os([
        OsString::from("lint"),
        project.path().as_os_str().to_owned(),
        OsString::from("--config"),
        config.as_os_str().to_owned(),
    ]);
    assert_structured_output(&warning_only, 0, "dartscope.lint-analysis");
    assert!(
        stdout(&warning_only).contains("dartscope.naming_convention"),
        "stdout: {}",
        stdout(&warning_only)
    );

    let denied = run_os([
        OsString::from("lint"),
        project.path().as_os_str().to_owned(),
        OsString::from("--config"),
        config.as_os_str().to_owned(),
        OsString::from("--deny-warnings"),
    ]);
    assert_structured_output(&denied, 4, "dartscope.lint-analysis");
}

#[test]
fn sarif_output_is_deterministic_and_keeps_spans_and_rule_metadata() {
    let project = sample_project("sarif output");
    write_file(&project.path().join("lib/BadName.dart"), "class bad_name {}\n");
    let config = project.path().join("dartscope.toml");
    write_file(
        &config,
        concat!(
            "version = 1\n",
            "enabled_rules = [\"dartscope.naming_convention\"]\n",
        ),
    );

    let args = || {
        vec![
            OsString::from("lint"),
            project.path().as_os_str().to_owned(),
            OsString::from("--config"),
            config.as_os_str().to_owned(),
            OsString::from("--format"),
            OsString::from("sarif"),
            OsString::from("--deny-warnings"),
        ]
    };
    let first = run_os(args());
    let second = run_os(args());
    assert_output_code(&first, 4);
    assert_output_code(&second, 4);
    assert_eq!(stdout(&first), stdout(&second));

    let sarif = stdout(&first);
    for marker in [
        "\"$schema\": \"https://json.schemastore.org/sarif-2.1.0.json\"",
        "\"version\": \"2.1.0\"",
        "\"ruleId\": \"dartscope.naming_convention\"",
        "\"name\": \"naming_convention\"",
        "\"startLine\"",
        "\"startColumn\"",
        "lib/BadName.dart",
    ] {
        assert!(sarif.contains(marker), "missing {marker:?} in {sarif}");
    }
}

#[test]
fn invalid_configuration_project_input_and_arguments_are_distinct() {
    let project = sample_project("error categories");
    let invalid = project.path().join("invalid.toml");
    write_file(&invalid, "version = 1\nenabled_rules = [\n");

    assert_error(
        run_os([
            OsString::from("lint"),
            project.path().as_os_str().to_owned(),
            OsString::from("--config"),
            invalid.into_os_string(),
        ]),
        5,
        "invalid lint configuration",
    );
    assert_error(
        run_os([
            OsString::from("lint"),
            project.path().as_os_str().to_owned(),
            OsString::from("--config"),
            project.path().join("missing.toml").into_os_string(),
        ]),
        3,
        "failed to read lint configuration",
    );
    assert_error(
        run_os([
            OsString::from("lint"),
            project.path().as_os_str().to_owned(),
            OsString::from("--format"),
            OsString::from("unknown"),
        ]),
        2,
        "expected json or sarif",
    );

    let malformed = TempDirectory::new("malformed project");
    write_file(
        &malformed.path().join("pubspec.yaml"),
        "flutter: [unterminated\n",
    );
    assert_error(
        run_os([
            OsString::from("lint"),
            malformed.path().as_os_str().to_owned(),
        ]),
        6,
        "malformed project input",
    );
}

#[test]
fn full_toml_surface_maps_to_the_existing_lint_engine() {
    let project = sample_project("full config surface");
    write_file(
        &project.path().join("lib/ui/screen.dart"),
        "import 'package:legacy/legacy.dart';\nclass Screen {}\n",
    );
    let config = project.path().join("dartscope.toml");
    write_file(
        &config,
        concat!(
            "version = 1\n",
            "failure_threshold = \"never\"\n",
            "enabled_rules = [\n",
            "  \"dartscope.forbidden_import\",\n",
            "  \"dartscope.layer_boundary\",\n",
            "  \"dartscope.naming_convention\",\n",
            "  \"dartscope.unresolved_part\",\n",
            "  \"dartscope.orphan_file\",\n",
            "]\n",
            "\n[[severity_overrides]]\n",
            "rule_id = \"dartscope.forbidden_import\"\n",
            "severity = \"error\"\n",
            "\n[[forbidden_imports]]\n",
            "uri = \"package:legacy/\"\n",
            "match_kind = \"prefix\"\n",
            "source_prefix = \"lib\\\\ui\\\\\"\n",
            "\n[[layer_boundaries]]\n",
            "source_prefix = \"lib/ui/\"\n",
            "denied_target_prefixes = [\"lib/data/\"]\n",
            "\n[naming]\n",
            "check_file_names = true\n",
            "check_top_level_declarations = true\n",
            "ignored_path_prefixes = [\"generated/\"]\n",
            "\n[orphan_files]\n",
            "entry_points = [\"lib/main.dart\"]\n",
            "ignored_path_prefixes = [\"test/\"]\n",
        ),
    );

    let output = run_os([
        OsString::from("lint"),
        project.path().as_os_str().to_owned(),
        OsString::from("--config"),
        config.into_os_string(),
    ]);
    assert_structured_output(&output, 0, "dartscope.lint-analysis");
    let json = stdout(&output);
    assert!(json.contains("dartscope.forbidden_import"), "stdout: {json}");
    assert!(json.contains("\"severity\": \"error\""), "stdout: {json}");
    assert!(json.contains("\"enabled_rules\": 5"), "stdout: {json}");
}

fn sample_project(label: &str) -> TempDirectory {
    let project = TempDirectory::new(label);
    write_file(&project.path().join("lib/main.dart"), "void main() {}\n");
    write_file(
        &project.path().join("pubspec.yaml"),
        "name: lint_sample\nenvironment:\n  sdk: ^3.4.0\n",
    );
    project
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

fn assert_text_success(output: &Output, expected: &str) {
    assert_output_code(output, 0);
    assert!(stdout(output).contains(expected), "stdout: {}", stdout(output));
}

fn assert_structured_output(output: &Output, exit_code: i32, schema: &str) {
    assert_output_code(output, exit_code);
    let stdout = stdout(output);
    assert!(
        stdout.contains(&format!("\"schema\": \"{schema}\"")),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("\"version\": 1"), "stdout: {stdout}");
    assert!(stdout.contains("\"data\":"), "stdout: {stdout}");
}

fn assert_output_code(output: &Output, exit_code: i32) {
    assert_eq!(
        output.status.code(),
        Some(exit_code),
        "stderr: {}",
        stderr(output)
    );
    assert!(stderr(output).is_empty(), "stderr: {}", stderr(output));
    assert!(!stdout(output).is_empty(), "expected structured stdout");
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
            "dartscope-lint-{sanitized}-{}-{sequence}",
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
