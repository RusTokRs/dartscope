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

#[cfg(unix)]
#[test]
fn flutter_generated_symlink_directories_do_not_abort_the_analysis() {
    use std::os::unix::fs::symlink;

    let project = TempDirectory::new("flutter generated symlinks");
    write_package(project.path(), "root_package", "lib/root.dart");
    let pub_cache = TempDirectory::new("flutter generated symlink target");
    write_file(&pub_cache.path().join("plugin.dart"), "void plugin() {}\n");
    for generated in [
        "ios/.symlinks/plugins",
        "linux/flutter/ephemeral/.plugin_symlinks",
    ] {
        let directory = project.path().join(generated);
        fs::create_dir_all(&directory).expect("create generated directory");
        symlink(pub_cache.path(), directory.join("some_plugin")).expect("create plugin symlink");
    }

    let output = run_os([
        OsString::from("analyze-project"),
        project.path().as_os_str().to_owned(),
    ]);

    assert_json_success(&output, "dartscope.project-analysis");
    let json = stdout(&output);
    assert!(json.contains("lib/root.dart"), "stdout: {json}");
    assert!(!json.contains("plugin.dart"), "stdout: {json}");
}

#[test]
fn build_target_and_coverage_folders_inside_source_roots_are_sources() {
    let project = TempDirectory::new("source folders named like output");
    write_package(project.path(), "root_package", "lib/root.dart");
    for kept in [
        "lib/src/build/kept_build.dart",
        "lib/target/kept_target.dart",
        "test/coverage/kept_coverage.dart",
    ] {
        write_file(&project.path().join(kept), "void kept() {}\n");
    }
    for ignored in [
        "build/ignored_build.dart",
        "android/app/build/ignored_android.dart",
        "coverage/ignored_coverage.dart",
        "rust/target/ignored_target.dart",
    ] {
        write_file(&project.path().join(ignored), "void ignored() {}\n");
    }

    let output = run_os([
        OsString::from("analyze-project"),
        project.path().as_os_str().to_owned(),
    ]);

    assert_json_success(&output, "dartscope.project-analysis");
    let json = stdout(&output);
    for kept in ["kept_build.dart", "kept_target.dart", "kept_coverage.dart"] {
        assert!(json.contains(kept), "{kept} is a source: {json}");
    }
    assert!(!json.contains("ignored_"), "stdout: {json}");
}

#[test]
fn a_source_that_is_not_utf8_is_reported_instead_of_aborting_analyze_project() {
    let project = TempDirectory::new("source in another encoding");
    write_package(project.path(), "root_package", "lib/root.dart");
    let latin1 = project.path().join("lib/latin1.dart");
    fs::write(&latin1, b"// caf\xe9\nvoid cafe() {}\n").expect("write latin-1 source");

    let output = run_os([
        OsString::from("analyze-project"),
        project.path().as_os_str().to_owned(),
    ]);

    assert_json_success(&output, "dartscope.project-analysis");
    let json = stdout(&output);
    assert!(json.contains("lib/root.dart"), "stdout: {json}");
    assert!(
        json.contains("\"code\": \"input_file_not_utf8\""),
        "stdout: {json}"
    );
    assert!(
        json.contains("\"path\": \"lib/latin1.dart\""),
        "stdout: {json}"
    );
    assert!(json.contains("\"dart_files\": 1"), "stdout: {json}");
    assert!(json.contains("\"diagnostics\": 1"), "stdout: {json}");
    assert!(!json.contains("cafe"), "stdout: {json}");

    // Commands whose output cannot carry the report keep failing instead of dropping the file.
    assert_error(
        run_os([
            OsString::from("uri-graph"),
            project.path().as_os_str().to_owned(),
        ]),
        3,
        "failed to read",
    );
}

#[cfg(unix)]
#[test]
fn a_non_unicode_argument_is_a_usage_error_not_a_panic() {
    use std::os::unix::ffi::OsStringExt;

    let output = run_os([
        OsString::from("analyze-file"),
        OsString::from_vec(b"bad-\xff.dart".to_vec()),
    ]);

    assert_error(output, 2, "argument is not valid Unicode");
}

#[cfg(windows)]
#[test]
fn a_non_unicode_argument_is_a_usage_error_not_a_panic() {
    use std::os::windows::ffi::OsStringExt;

    let output = run_os([
        OsString::from("analyze-file"),
        OsString::from_wide(&[0x62, 0xd800, 0x2e, 0x64]),
    ]);

    assert_error(output, 2, "argument is not valid Unicode");
}

#[test]
fn a_closed_stdout_ends_the_command_quietly() {
    use std::process::Stdio;

    let project = TempDirectory::new("closed stdout");
    let source = project.path().join("big.dart");
    let classes: String = (0..3000)
        .map(|index| format!("class C{index} {{\n  int f{index} = {index};\n}}\n"))
        .collect();
    write_file(&source, &classes);

    let mut child = Command::new(env!("CARGO_BIN_EXE_dartscope"))
        .arg("analyze-file")
        .arg(&source)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run dartscope");
    // The output is far larger than a pipe buffer, so the write must hit the closed pipe.
    drop(child.stdout.take());
    let output = child.wait_with_output().expect("wait for dartscope");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        !stderr(&output).contains("panicked"),
        "stderr: {}",
        stderr(&output)
    );
}

/// Removes the whitespace between JSON tokens, so pretty and compact output can be compared.
fn without_json_whitespace(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    let (mut in_string, mut escaped) = (false, false);
    for ch in json.chars() {
        if in_string {
            out.push(ch);
            match (escaped, ch) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
        } else if ch == '"' {
            in_string = true;
            out.push(ch);
        } else if !ch.is_whitespace() {
            out.push(ch);
        }
    }
    out
}

#[test]
fn compact_prints_the_same_document_on_one_line() {
    let project = sample_project("compact output");
    let source = project.path().join("lib/main.dart");
    let cases: Vec<Vec<OsString>> = vec![
        vec!["analyze-file".into(), source.as_os_str().to_owned()],
        vec![
            "pubspec".into(),
            project.path().join("pubspec.yaml").into_os_string(),
        ],
        vec![
            "analyze-project".into(),
            project.path().as_os_str().to_owned(),
        ],
        vec!["uri-graph".into(), project.path().as_os_str().to_owned()],
        vec![
            "graphql-contracts".into(),
            project.path().as_os_str().to_owned(),
        ],
        vec![
            "flutter-inventory".into(),
            project.path().as_os_str().to_owned(),
        ],
        vec!["lint".into(), project.path().as_os_str().to_owned()],
    ];
    for case in cases {
        let pretty = run_os(case.clone());
        let mut compact_args = case.clone();
        compact_args.push("--compact".into());
        let compact = run_os(compact_args);

        assert_eq!(
            compact.status.code(),
            Some(0),
            "{case:?}: {}",
            stderr(&compact)
        );
        assert!(stderr(&compact).is_empty(), "stderr: {}", stderr(&compact));
        let compact_text = stdout(&compact);
        let compact_json = compact_text
            .strip_suffix('\n')
            .expect("one trailing newline");
        assert!(!compact_json.contains('\n'), "{case:?}: {compact_json}");
        assert!(
            compact_json.len() < stdout(&pretty).len(),
            "{case:?}: compact output is smaller"
        );
        assert_eq!(
            without_json_whitespace(&stdout(&pretty)),
            compact_json,
            "{case:?}: the same document"
        );
    }
}

#[test]
fn compact_is_accepted_anywhere_after_the_path_and_only_once() {
    let project = sample_project("compact position");
    let path = project.path().as_os_str().to_owned();

    let after_env = run_os([
        OsString::from("uri-graph"),
        path.clone(),
        OsString::from("--env"),
        OsString::from("flag=true"),
        OsString::from("--compact"),
    ]);
    assert_eq!(after_env.status.code(), Some(0), "{}", stderr(&after_env));
    assert!(!stdout(&after_env).trim_end().contains('\n'));

    let before_env = run_os([
        OsString::from("uri-graph"),
        path.clone(),
        OsString::from("--compact"),
        OsString::from("--env"),
        OsString::from("flag=true"),
    ]);
    assert_eq!(stdout(&before_env), stdout(&after_env));

    assert_error(
        run_os([
            OsString::from("analyze-project"),
            path.clone(),
            OsString::from("--compact"),
            OsString::from("--compact"),
        ]),
        2,
        "--compact may be given only once",
    );
    for command in command_names() {
        let help = run([command, "--help"]);
        assert!(stdout(&help).contains("--compact"), "{command}");
    }
}

#[test]
fn analyze_project_reports_the_root_without_dot_components_or_as_a_dot() {
    let project = sample_project("root label");
    let in_project = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_dartscope"))
            .current_dir(project.path())
            .args(args)
            .output()
            .expect("run dartscope")
    };

    let default = in_project(&["analyze-project", "."]);
    assert_json_success(&default, "dartscope.project-analysis");
    let root = json_string_field(&stdout(&default), "root");
    assert_ne!(root, ".", "the default root stays absolute");
    assert!(
        Path::new(&root).is_absolute()
            && Path::new(&root).file_name() == project.path().file_name(),
        "root: {root}"
    );
    assert!(
        !root.ends_with("/.") && !root.ends_with("\\."),
        "no trailing dot component in {root}"
    );

    let relative = in_project(&["analyze-project", ".", "--relative-root"]);
    assert_json_success(&relative, "dartscope.project-analysis");
    assert_eq!(json_string_field(&stdout(&relative), "root"), ".");
    assert!(stdout(&relative).contains("\"path\": \"lib/main.dart\""));

    // Only analyze-project reports a root.
    assert_error(
        in_project(&["uri-graph", ".", "--relative-root"]),
        2,
        "unexpected argument for uri-graph: --relative-root",
    );
    assert_error(
        in_project(&["analyze-project", ".", "--relative-root", "--relative-root"]),
        2,
        "--relative-root may be given only once",
    );
}

/// The value of the first `"<name>": "<value>"` pair of pretty JSON output.
fn json_string_field(json: &str, name: &str) -> String {
    let marker = format!("\"{name}\": \"");
    let start = json.find(&marker).expect("field is present") + marker.len();
    let rest = &json[start..];
    let end = rest.find('"').expect("field ends");
    rest[..end].replace("\\\\", "\\")
}

#[test]
fn directories_that_are_not_walked_are_reported_as_info_diagnostics() {
    let project = TempDirectory::new("skipped directories");
    write_package(project.path(), "root_package", "lib/root.dart");
    for skipped in [
        "build/out.dart",
        "ios/Pods/Pod.dart",
        "ios/.symlinks/plugin.dart",
        "node_modules/dep.dart",
        ".git/hook.dart",
        "coverage/lcov.dart",
    ] {
        write_file(&project.path().join(skipped), "void skipped() {}\n");
    }
    write_file(
        &project.path().join("lib/build/kept.dart"),
        "void kept() {}\n",
    );

    let output = run_os([
        OsString::from("analyze-project"),
        project.path().as_os_str().to_owned(),
    ]);

    assert_json_success(&output, "dartscope.project-analysis");
    let json = stdout(&output);
    assert_eq!(
        json.matches("\"code\": \"input_directory_skipped\"")
            .count(),
        5,
        "stdout: {json}"
    );
    for path in [
        "build",
        "coverage",
        "ios/.symlinks",
        "ios/Pods",
        "node_modules",
    ] {
        assert!(
            json.contains(&format!("\"path\": \"{path}\"")),
            "{path}: {json}"
        );
    }
    // Tool state and source folders named like output are not worth a message.
    assert!(!json.contains("\"path\": \".git\""), "stdout: {json}");
    assert!(!json.contains("\"path\": \"lib/build\""), "stdout: {json}");
    assert!(json.contains("\"severity\": \"info\""), "stdout: {json}");
    assert!(json.contains("\"dart_files\": 2"), "stdout: {json}");
    assert!(!json.contains("out.dart"), "stdout: {json}");
}

#[cfg(unix)]
#[test]
fn skip_symlinks_turns_rejected_links_into_warnings() {
    use std::os::unix::fs::symlink;

    let project = TempDirectory::new("skip symlinks");
    write_package(project.path(), "root_package", "lib/root.dart");
    write_file(
        &project.path().join("lib/real/inner.dart"),
        "void inner() {}\n",
    );
    let outside = TempDirectory::new("skip symlinks outside");
    write_file(&outside.path().join("outside.dart"), "void outside() {}\n");
    symlink(
        outside.path().join("outside.dart"),
        project.path().join("lib/escape.dart"),
    )
    .expect("escaping link");
    symlink("real", project.path().join("lib/linked")).expect("directory link");
    symlink("missing.dart", project.path().join("lib/dangling.dart")).expect("dangling link");

    let rejected = run_os([
        OsString::from("analyze-project"),
        project.path().as_os_str().to_owned(),
    ]);
    assert_error(rejected, 3, "input_symlink_rejected");

    let output = run_os([
        OsString::from("analyze-project"),
        project.path().as_os_str().to_owned(),
        OsString::from("--skip-symlinks"),
    ]);
    assert_json_success(&output, "dartscope.project-analysis");
    let json = stdout(&output);
    assert_eq!(
        json.matches("\"code\": \"input_symlink_skipped\"").count(),
        3,
        "stdout: {json}"
    );
    for path in ["lib/escape.dart", "lib/linked", "lib/dangling.dart"] {
        assert!(
            json.contains(&format!("\"path\": \"{path}\"")),
            "{path}: {json}"
        );
    }
    assert!(json.contains("\"severity\": \"warning\""), "stdout: {json}");
    assert!(json.contains("\"dart_files\": 2"), "stdout: {json}");
    assert!(json.contains("lib/real/inner.dart"), "stdout: {json}");
    assert!(!json.contains("void outside"), "stdout: {json}");

    // The other commands cannot carry a report, so they keep failing.
    assert_error(
        run_os([
            OsString::from("uri-graph"),
            project.path().as_os_str().to_owned(),
            OsString::from("--skip-symlinks"),
        ]),
        2,
        "unexpected argument for uri-graph: --skip-symlinks",
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
