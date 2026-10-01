mod input_limits;
mod lint_command;

use std::collections::VecDeque;
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use dartscope::{
    DartCompilationEnvironment, DartDiagnostic, DartFileInput, DartIndexOptions,
    DartProjectAnalysis, DartProjectInput, FlutterArbInput, FlutterCatalogInput, FlutterL10nInput,
    JsonContract, PackageConfigInput, PubspecInput, analyze_file_with_flutter,
    analyze_graphql_contracts_with_options, analyze_project, analyze_project_with_flutter,
    build_uri_graph_with_options, extract_flutter_inventory_with_catalogs, parse_pubspec,
    parse_pubspec_configuration, to_json, to_json_contract, to_json_contract_pretty,
    to_json_pretty,
};
use serde::Serialize;

const EXIT_INTERNAL: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_INPUT: u8 = 3;
const EXIT_FINDINGS: u8 = 4;
const EXIT_CONFIGURATION: u8 = 5;
const EXIT_PROJECT: u8 = 6;

#[derive(Debug, Clone, Eq, PartialEq)]
struct CliOutput {
    text: String,
    exit_code: u8,
}

impl CliOutput {
    fn success(text: impl Into<String>) -> Self {
        Self::new(text, 0)
    }

    fn new(text: impl Into<String>, exit_code: u8) -> Self {
        Self {
            text: text.into(),
            exit_code,
        }
    }
}

/// How JSON is written: indented for people (the default) or on one line for pipelines.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
enum JsonStyle {
    #[default]
    Pretty,
    Compact,
}

impl JsonStyle {
    /// Serializes `value` inside the versioned envelope of `contract`.
    fn contract_text<T: Serialize + ?Sized>(
        self,
        contract: JsonContract,
        value: &T,
    ) -> Result<String, String> {
        let json = match self {
            Self::Pretty => to_json_contract_pretty(contract, value),
            Self::Compact => to_json_contract(contract, value),
        };
        json.map_err(|error| error.to_string())
    }

    /// Serializes a value that has its own schema outside the DartScope envelopes, like SARIF.
    fn plain_text<T: Serialize + ?Sized>(self, value: &T) -> Result<String, String> {
        let json = match self {
            Self::Pretty => to_json_pretty(value),
            Self::Compact => to_json(value),
        };
        json.map_err(|error| error.to_string())
    }

    fn contract_output<T: Serialize + ?Sized>(
        self,
        contract: JsonContract,
        value: &T,
    ) -> Result<CliOutput, CliError> {
        self.contract_text(contract, value)
            .map(CliOutput::success)
            .map_err(|error| {
                CliError::internal(format!("failed to serialize JSON output: {error}"))
            })
    }
}

/// Takes the output options that every command shares out of its arguments.
fn split_json_style(args: &[String]) -> Result<(JsonStyle, Vec<String>), CliError> {
    let mut style = JsonStyle::Pretty;
    let mut rest = Vec::with_capacity(args.len());
    for argument in args {
        if argument != "--compact" {
            rest.push(argument.clone());
        } else if style == JsonStyle::Compact {
            return Err(CliError::usage("--compact may be given only once"));
        } else {
            style = JsonStyle::Compact;
        }
    }
    Ok((style, rest))
}

fn main() -> ExitCode {
    // `env::args()` panics on an argument that is not valid Unicode, but paths on Unix and Windows
    // are not required to be Unicode, so report such an argument as a usage error instead.
    let arguments = match env::args_os()
        .skip(1)
        .map(OsString::into_string)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(arguments) => arguments,
        Err(argument) => {
            let error = CliError::usage(format!(
                "argument is not valid Unicode: {}",
                argument.to_string_lossy()
            ));
            return report_error(&error);
        }
    };
    match run(arguments) {
        Ok(output) => write_output(&output),
        Err(error) => report_error(&error),
    }
}

fn report_error(error: &CliError) -> ExitCode {
    // A closed stderr leaves nowhere to report to, and `eprintln!` would panic on it.
    let _ = writeln!(io::stderr(), "error: {error}");
    ExitCode::from(error.exit_code())
}

fn write_output(output: &CliOutput) -> ExitCode {
    let mut stdout = io::stdout().lock();
    match writeln!(stdout, "{}", output.text).and_then(|()| stdout.flush()) {
        Ok(()) => ExitCode::from(output.exit_code),
        // The reader went away (`dartscope ... | head`). There is nobody left to inform, so keep
        // the exit code of the command itself instead of panicking inside `println!`.
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::from(output.exit_code),
        Err(error) => report_error(&CliError::internal(format!(
            "failed to write output: {error}"
        ))),
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<CliOutput, CliError> {
    let args: Vec<String> = args.into_iter().collect();
    let Some(first) = args.first() else {
        return Err(CliError::usage(format!(
            "missing command\n\n{}",
            global_help()
        )));
    };

    match first.as_str() {
        "--help" | "-h" => {
            reject_global_extra_args(&args[1..], "--help")?;
            return Ok(CliOutput::success(global_help()));
        }
        "--version" | "-V" => {
            reject_global_extra_args(&args[1..], "--version")?;
            return Ok(CliOutput::success(version_text()));
        }
        "help" => return help_command(&args[1..]),
        _ => {}
    }

    let command = CliCommand::parse(first)
        .ok_or_else(|| CliError::usage(format!("unknown command: {first}\n\n{}", global_help())))?;
    if args
        .get(1)
        .is_some_and(|argument| matches!(argument.as_str(), "--help" | "-h"))
    {
        reject_global_extra_args(&args[2..], "--help")?;
        return Ok(CliOutput::success(command.help()));
    }

    let path = args.get(1).ok_or_else(|| {
        CliError::usage(format!(
            "missing path for {}\n\n{}",
            command.name(),
            command.help()
        ))
    })?;
    let (style, extra_args) = split_json_style(&args[2..])?;
    execute(command, path, &extra_args, style)
}

fn help_command(args: &[String]) -> Result<CliOutput, CliError> {
    match args {
        [] => Ok(CliOutput::success(global_help())),
        [command] => CliCommand::parse(command)
            .map(|command| CliOutput::success(command.help()))
            .ok_or_else(|| {
                CliError::usage(format!("unknown command: {command}\n\n{}", global_help()))
            }),
        [_, extra, ..] => Err(CliError::usage(format!("unexpected argument: {extra}"))),
    }
}

fn execute(
    command: CliCommand,
    path: &str,
    extra_args: &[String],
    style: JsonStyle,
) -> Result<CliOutput, CliError> {
    match command {
        CliCommand::AnalyzeFile => {
            reject_extra_args(extra_args, command)?;
            let source = read_source(path)?;
            let analysis = analyze_file_with_flutter(DartFileInput::new(path, source));
            style.contract_output(JsonContract::FileAnalysis, &analysis)
        }
        CliCommand::Pubspec => {
            reject_extra_args(extra_args, command)?;
            let source = read_source(path)?;
            let analysis = parse_pubspec(PubspecInput::new(path, source));
            style.contract_output(JsonContract::PubspecAnalysis, &analysis)
        }
        CliCommand::PubspecConfig => {
            reject_extra_args(extra_args, command)?;
            let source = read_source(path)?;
            let analysis = parse_pubspec_configuration(PubspecInput::new(path, source));
            style.contract_output(JsonContract::PubspecConfiguration, &analysis)
        }
        CliCommand::AnalyzeProject => {
            let options = ProjectOptions::parse(extra_args, command)?;
            let mut sources = collect_project_sources_reporting_skips(path, options)?;
            if options.relative_root {
                // Keeps the report free of the machine it ran on, so it can be diffed and cached.
                sources.dart.root = ".".to_string();
            }
            let analysis = with_input_diagnostics(
                analyze_project_with_flutter(sources.dart),
                sources.input_diagnostics,
            );
            style.contract_output(JsonContract::ProjectAnalysis, &analysis)
        }
        CliCommand::GraphqlContracts => {
            let options = parse_index_options(extra_args, command)?;
            let input = collect_project_input(path)?;
            let project = analyze_project(input);
            let analysis = analyze_graphql_contracts_with_options(&project, &options);
            style.contract_output(JsonContract::GraphqlContracts, &analysis)
        }
        CliCommand::UriGraph => {
            let options = parse_index_options(extra_args, command)?;
            let input = collect_project_input(path)?;
            let project = analyze_project(input);
            let graph = build_uri_graph_with_options(&project, &options);
            style.contract_output(JsonContract::UriGraph, &graph)
        }
        CliCommand::FlutterInventory => {
            reject_extra_args(extra_args, command)?;
            let input = collect_flutter_project_sources(path)?;
            let project = analyze_project(input.dart);
            let inventory = extract_flutter_inventory_with_catalogs(&project, &input.flutter);
            style.contract_output(JsonContract::FlutterInventory, &inventory)
        }
        CliCommand::Lint => lint_command::execute(path, extra_args, style),
    }
}

fn reject_global_extra_args(args: &[String], option: &str) -> Result<(), CliError> {
    if let Some(extra) = args.first() {
        Err(CliError::usage(format!(
            "unexpected argument after {option}: {extra}"
        )))
    } else {
        Ok(())
    }
}

fn reject_extra_args(args: &[String], command: CliCommand) -> Result<(), CliError> {
    if let Some(argument) = args.first() {
        Err(CliError::usage(format!(
            "unexpected argument for {}: {argument}\n\n{}",
            command.name(),
            command.help()
        )))
    } else {
        Ok(())
    }
}

fn parse_index_options(args: &[String], command: CliCommand) -> Result<DartIndexOptions, CliError> {
    let mut entries = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--env" => {
                let pair = args.get(index + 1).ok_or_else(|| {
                    CliError::usage(format!(
                        "missing value for --env; expected --env key=value\n\n{}",
                        command.help()
                    ))
                })?;
                entries.push(parse_environment_entry(pair)?);
                index += 2;
            }
            argument => {
                return Err(CliError::usage(format!(
                    "unexpected argument for {}: {argument}\n\n{}",
                    command.name(),
                    command.help()
                )));
            }
        }
    }

    let options = if entries.is_empty() {
        DartIndexOptions::default()
    } else {
        DartIndexOptions::default()
            .with_compilation_environment(DartCompilationEnvironment::from_pairs(entries))
    };
    Ok(options)
}

/// Options of `analyze-project`.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
struct ProjectOptions {
    /// Report the project root as `.` instead of the absolute path of this machine.
    relative_root: bool,
    /// Report a symlink the walker refuses to follow as a diagnostic instead of failing the run.
    skip_symlinks: bool,
}

impl ProjectOptions {
    fn parse(args: &[String], command: CliCommand) -> Result<Self, CliError> {
        let mut options = Self::default();
        for argument in args {
            let flag = match argument.as_str() {
                "--relative-root" => &mut options.relative_root,
                "--skip-symlinks" => &mut options.skip_symlinks,
                other => {
                    return Err(CliError::usage(format!(
                        "unexpected argument for {}: {other}\n\n{}",
                        command.name(),
                        command.help()
                    )));
                }
            };
            if std::mem::replace(flag, true) {
                return Err(CliError::usage(format!(
                    "{argument} may be given only once"
                )));
            }
        }
        Ok(options)
    }
}

fn parse_environment_entry(pair: &str) -> Result<(String, String), CliError> {
    let Some((key, value)) = pair.split_once('=') else {
        return Err(CliError::usage(format!(
            "invalid --env value {pair:?}; expected --env key=value"
        )));
    };
    if key.is_empty() {
        return Err(CliError::usage("invalid --env value: key cannot be empty"));
    }
    Ok((key.to_string(), value.to_string()))
}

fn read_source(path: &str) -> Result<String, CliError> {
    input_limits::read_path(
        Path::new(path),
        Path::new(path),
        input_limits::DEFAULT_INPUT_LIMITS.max_file_bytes,
    )
}

struct CollectedProjectSources {
    dart: DartProjectInput,
    flutter: FlutterCatalogInput,
    /// Inputs that were deliberately left out of `dart`, such as a source that is not valid UTF-8.
    input_diagnostics: Vec<DartDiagnostic>,
}

#[derive(Default)]
struct ProjectSourceAccumulator {
    files: Vec<DartFileInput>,
    pubspecs: Vec<PubspecInput>,
    package_configs: Vec<PackageConfigInput>,
    l10n_files: Vec<FlutterL10nInput>,
    arb_files: Vec<FlutterArbInput>,
    input_diagnostics: Vec<DartDiagnostic>,
    collect_flutter_catalogs: bool,
    /// Leave a source that is not valid UTF-8 out of the project and report it, instead of
    /// failing. Only commands that can show the report ask for this.
    skip_invalid_utf8: bool,
    /// Leave a rejected symlink out of the project and report it, instead of failing.
    skip_symlinks: bool,
    /// Report the directories the walk does not enter, such as `build` or `Pods`.
    report_skipped_directories: bool,
}

impl ProjectSourceAccumulator {
    fn new(collect_flutter_catalogs: bool) -> Self {
        Self {
            collect_flutter_catalogs,
            ..Self::default()
        }
    }

    fn finish(mut self, root_path: &Path) -> CollectedProjectSources {
        self.files.sort_by(|left, right| left.path.cmp(&right.path));
        self.pubspecs
            .sort_by(|left, right| left.path.cmp(&right.path));
        self.package_configs
            .sort_by(|left, right| left.path.cmp(&right.path));
        self.l10n_files
            .sort_by(|left, right| left.path.cmp(&right.path));
        self.arb_files
            .sort_by(|left, right| left.path.cmp(&right.path));
        self.input_diagnostics
            .sort_by(|left, right| (&left.path, &left.code).cmp(&(&right.path, &right.code)));

        CollectedProjectSources {
            dart: DartProjectInput::new(
                root_path.to_string_lossy().into_owned(),
                self.files,
                self.pubspecs,
            )
            .with_package_configs(self.package_configs),
            flutter: FlutterCatalogInput::new(self.l10n_files, self.arb_files),
            input_diagnostics: self.input_diagnostics,
        }
    }
}

/// Adds the diagnostics for inputs that were left out of the analysis to the project diagnostics.
fn with_input_diagnostics(
    mut analysis: DartProjectAnalysis,
    input_diagnostics: Vec<DartDiagnostic>,
) -> DartProjectAnalysis {
    analysis.diagnostics.extend(input_diagnostics);
    analysis.summary.diagnostics = analysis.diagnostics.len();
    analysis
}

/// Why the walk does not enter a directory worth telling the user about, or `None` for tool state
/// (`.git`, `.dart_tool`, ...) nobody expects to be analyzed.
fn skipped_directory_reason(name: &str) -> Option<&'static str> {
    match name {
        "build" | "coverage" | "target" => Some(
            "build output directories are only analyzed inside lib, bin, test, test_driver, tool, integration_test and benchmark",
        ),
        "Pods" | "node_modules" => Some("dependency directories are not analyzed"),
        ".symlinks" | ".plugin_symlinks" => {
            Some("Flutter plugin link directories are not analyzed")
        }
        _ => None,
    }
}

fn skipped_directory_diagnostic(path: String, reason: &str) -> DartDiagnostic {
    let mut diagnostic = DartDiagnostic::info(
        "input_directory_skipped",
        format!("the directory was not analyzed: {reason}"),
        None,
    );
    diagnostic.path = Some(path);
    diagnostic
}

fn skipped_symlink_diagnostic(path: String, reason: &str) -> DartDiagnostic {
    let mut diagnostic = DartDiagnostic::warning(
        "input_symlink_skipped",
        format!("the symlink was left out of the analysis: {reason}"),
        None,
    );
    diagnostic.path = Some(path);
    diagnostic
}

fn not_utf8_diagnostic(path: String) -> DartDiagnostic {
    let mut diagnostic = DartDiagnostic::warning(
        "input_file_not_utf8",
        "the file is not valid UTF-8 text and was left out of the analysis",
        None,
    );
    diagnostic.path = Some(path);
    diagnostic
}

fn collect_project_input(root: &str) -> Result<DartProjectInput, CliError> {
    Ok(collect_project_sources(root, false)?.dart)
}

fn collect_flutter_project_sources(root: &str) -> Result<CollectedProjectSources, CliError> {
    collect_project_sources(root, true)
}

fn collect_project_sources(
    root: &str,
    collect_flutter_catalogs: bool,
) -> Result<CollectedProjectSources, CliError> {
    collect_project_sources_with_limits(
        root,
        collect_flutter_catalogs,
        input_limits::DEFAULT_INPUT_LIMITS,
    )
}

fn collect_project_sources_with_limits(
    root: &str,
    collect_flutter_catalogs: bool,
    limits: input_limits::InputLimits,
) -> Result<CollectedProjectSources, CliError> {
    collect_into_accumulator(
        root,
        ProjectSourceAccumulator::new(collect_flutter_catalogs),
        limits,
    )
}

/// Collects a project like [`collect_project_sources`], but leaves sources that are not valid
/// UTF-8 out and returns a diagnostic for each of them, and for every directory the walk skips.
fn collect_project_sources_reporting_skips(
    root: &str,
    options: ProjectOptions,
) -> Result<CollectedProjectSources, CliError> {
    let mut sources = ProjectSourceAccumulator::new(false);
    sources.skip_invalid_utf8 = true;
    sources.skip_symlinks = options.skip_symlinks;
    sources.report_skipped_directories = true;
    collect_into_accumulator(root, sources, input_limits::DEFAULT_INPUT_LIMITS)
}

fn collect_into_accumulator(
    root: &str,
    mut sources: ProjectSourceAccumulator,
    limits: input_limits::InputLimits,
) -> Result<CollectedProjectSources, CliError> {
    let root = resolve_project_root(root)?;
    let mut budget = input_limits::ProjectInputBudget::default();
    let mut traversal = input_limits::ProjectTraversalBudget::default();
    collect_sources(
        &root,
        &root.logical,
        &mut sources,
        limits,
        &mut budget,
        &mut traversal,
    )?;
    Ok(sources.finish(&root.logical))
}

#[derive(Debug)]
struct ProjectRoot {
    logical: PathBuf,
    canonical: PathBuf,
}

fn resolve_project_root(root: &str) -> Result<ProjectRoot, CliError> {
    let path = PathBuf::from(root);
    let path = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map_err(|error| CliError::input(format!("failed to read current directory: {error}")))?
            .join(path)
    };
    // `dartscope analyze-project .` would otherwise report `/work/project/.`. Dropping the `.`
    // components is purely lexical: `..` stays, so nothing is resolved through a symlink here.
    let path: PathBuf = path.components().collect();

    let metadata = fs::metadata(&path).map_err(|error| {
        CliError::input(format!(
            "failed to inspect project root {}: {error}",
            path.display()
        ))
    })?;
    if !metadata.is_dir() {
        return Err(CliError::input(format!(
            "project root is not a directory: {}",
            path.display()
        )));
    }
    let canonical = fs::canonicalize(&path).map_err(|error| {
        CliError::input(format!(
            "failed to resolve project root {}: {error}",
            path.display()
        ))
    })?;

    Ok(ProjectRoot {
        logical: path,
        canonical,
    })
}

fn collect_sources(
    root: &ProjectRoot,
    directory: &Path,
    sources: &mut ProjectSourceAccumulator,
    limits: input_limits::InputLimits,
    budget: &mut input_limits::ProjectInputBudget,
    traversal: &mut input_limits::ProjectTraversalBudget,
) -> Result<(), CliError> {
    // Breadth-first queue (VecDeque + pop_front/push_back) keeps traversal order deterministic
    // and sorted: each directory's entries are sorted, then enqueued in that order, so siblings
    // are visited lexicographically and diagnostics are reproducible across hosts.
    let mut pending_directories = VecDeque::from([directory.to_path_buf()]);
    traversal.ensure_pending_directories(directory, pending_directories.len(), limits)?;

    while let Some(directory) = pending_directories.pop_front() {
        let entries = fs::read_dir(&directory).map_err(|error| {
            CliError::input(format!(
                "failed to read directory {}: {error}",
                directory.display()
            ))
        })?;
        let mut entries = entries
            .map(|entry| {
                entry.map_err(|error| {
                    CliError::input(format!(
                        "failed to read directory entry in {}: {error}",
                        directory.display()
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Directory iteration order is filesystem-dependent. Sorting each directory keeps traversal,
        // and therefore every traversal-limit diagnostic, reproducible across hosts.
        entries.sort_by_key(|entry| entry.path());

        for entry in entries {
            let path = entry.path();
            traversal.record_directory_entry(&path, limits)?;
            let file_type = entry.file_type().map_err(|error| {
                CliError::input(format!("failed to inspect {}: {error}", path.display()))
            })?;

            if file_type.is_dir() {
                if is_skipped_directory(&root.logical, &path) {
                    sources.note_skipped_directory(root, &path);
                } else {
                    traversal.ensure_can_queue_directory(
                        &path,
                        pending_directories.len(),
                        limits,
                    )?;
                    pending_directories.push_back(path);
                }
                continue;
            }
            let Some(source_read_path) = sources.readable_path(root, &path, &file_type)? else {
                continue;
            };

            let Some(source_relative_path) = relative_path(&root.logical, &path) else {
                continue;
            };

            match path.file_name().and_then(|name| name.to_str()) {
                Some("l10n.yaml") if sources.collect_flutter_catalogs => {
                    let source =
                        input_limits::read_project_path(&source_read_path, &path, limits, budget)?;
                    sources
                        .l10n_files
                        .push(FlutterL10nInput::new(source_relative_path, source));
                }
                Some("pubspec.yaml") => {
                    let source =
                        input_limits::read_project_path(&source_read_path, &path, limits, budget)?;
                    sources
                        .pubspecs
                        .push(PubspecInput::new(source_relative_path, source));
                    if let Some(package_root) = path.parent() {
                        let package_config_path =
                            package_root.join(".dart_tool").join("package_config.json");
                        if let Some(package_config_read_path) =
                            sources.optional_readable_path(root, &package_config_path)?
                        {
                            let source = input_limits::read_project_path(
                                &package_config_read_path,
                                &package_config_path,
                                limits,
                                budget,
                            )?;
                            if let Some(relative_path) =
                                relative_path(&root.logical, &package_config_path)
                            {
                                sources
                                    .package_configs
                                    .push(PackageConfigInput::new(relative_path, source));
                            }
                        }
                    }
                }
                _ if path.extension().and_then(|extension| extension.to_str()) == Some("dart") => {
                    if sources.skip_invalid_utf8 {
                        // One source in another encoding must not abort the analysis of the others.
                        match input_limits::read_project_text_path(
                            &source_read_path,
                            &path,
                            limits,
                            budget,
                        )? {
                            Some(source) => sources
                                .files
                                .push(DartFileInput::new(source_relative_path, source)),
                            None => sources
                                .input_diagnostics
                                .push(not_utf8_diagnostic(source_relative_path)),
                        }
                    } else {
                        let source = input_limits::read_project_path(
                            &source_read_path,
                            &path,
                            limits,
                            budget,
                        )?;
                        sources
                            .files
                            .push(DartFileInput::new(source_relative_path, source));
                    }
                }
                _ if sources.collect_flutter_catalogs
                    && path.extension().and_then(|extension| extension.to_str()) == Some("arb") =>
                {
                    let source =
                        input_limits::read_project_path(&source_read_path, &path, limits, budget)?;
                    sources
                        .arb_files
                        .push(FlutterArbInput::new(source_relative_path, source));
                }
                _ => {}
            }
        }
    }

    Ok(())
}

/// A symlink the walker refuses to follow, and why.
#[derive(Debug)]
struct RejectedSymlink(String);

impl From<RejectedSymlink> for CliError {
    fn from(rejected: RejectedSymlink) -> Self {
        CliError::input(format!("input_symlink_rejected: {}", rejected.0))
    }
}

impl ProjectSourceAccumulator {
    /// The path to read a directory entry from: the entry itself for a file, the target of a
    /// symlink that stays inside the project, `None` for anything that is not a source file.
    /// A rejected symlink is an input error, or a diagnostic with `skip_symlinks`.
    fn readable_path(
        &mut self,
        root: &ProjectRoot,
        path: &Path,
        file_type: &fs::FileType,
    ) -> Result<Option<PathBuf>, CliError> {
        match source_file_read_path(root, path, file_type) {
            Ok(read_path) => Ok(read_path),
            Err(rejected) if self.skip_symlinks => {
                let shown = relative_path(&root.logical, path)
                    .unwrap_or_else(|| path.display().to_string());
                self.input_diagnostics
                    .push(skipped_symlink_diagnostic(shown, &rejected.0));
                Ok(None)
            }
            Err(rejected) => Err(rejected.into()),
        }
    }

    /// Like [`Self::readable_path`] for a file that may not exist.
    fn optional_readable_path(
        &mut self,
        root: &ProjectRoot,
        path: &Path,
    ) -> Result<Option<PathBuf>, CliError> {
        match fs::symlink_metadata(path) {
            Ok(metadata) => self.readable_path(root, path, &metadata.file_type()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(CliError::input(format!(
                "failed to inspect {}: {error}",
                path.display()
            ))),
        }
    }

    /// Records a directory the walk does not enter, when the command reports them.
    fn note_skipped_directory(&mut self, root: &ProjectRoot, path: &Path) {
        if !self.report_skipped_directories {
            return;
        }
        let reason = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(skipped_directory_reason);
        if let (Some(reason), Some(shown)) = (reason, relative_path(&root.logical, path)) {
            self.input_diagnostics
                .push(skipped_directory_diagnostic(shown, reason));
        }
    }
}

fn source_file_read_path(
    root: &ProjectRoot,
    path: &Path,
    file_type: &fs::FileType,
) -> Result<Option<PathBuf>, RejectedSymlink> {
    if file_type.is_file() {
        return Ok(Some(path.to_path_buf()));
    }
    if !file_type.is_symlink() {
        return Ok(None);
    }

    let target = fs::canonicalize(path).map_err(|error| {
        RejectedSymlink(format!(
            "failed to resolve symlink {}: {error}",
            path.display()
        ))
    })?;
    if !target.starts_with(&root.canonical) {
        return Err(RejectedSymlink(format!(
            "symlink {} resolves outside project root {}: {}",
            path.display(),
            root.logical.display(),
            target.display()
        )));
    }

    let metadata = fs::metadata(&target).map_err(|error| {
        RejectedSymlink(format!(
            "failed to inspect symlink target {}: {error}",
            target.display()
        ))
    })?;
    if metadata.is_dir() {
        return Err(RejectedSymlink(format!(
            "symlinked directories are not supported: {} -> {}",
            path.display(),
            target.display()
        )));
    }
    if !metadata.is_file() {
        return Err(RejectedSymlink(format!(
            "symlink target is not a regular file: {} -> {}",
            path.display(),
            target.display()
        )));
    }

    Ok(Some(target))
}

fn relative_path(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
}

/// Directories that hold tool state, dependencies or generated output rather than project sources.
///
/// `.symlinks` and `.plugin_symlinks` are created by Flutter next to the platform projects and hold
/// links into the pub cache, which the walker would otherwise reject.
fn is_skipped_directory(root: &Path, path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    match name {
        ".dart_tool" | ".git" | ".idea" | ".pub-cache" | ".vscode" | ".symlinks"
        | ".plugin_symlinks" | "Pods" | "node_modules" => true,
        // Build, coverage and cargo output directories sit next to a package, but the same names
        // are ordinary folders inside the source roots, where skipping them would hide sources.
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum CliCommand {
    AnalyzeFile,
    Pubspec,
    PubspecConfig,
    AnalyzeProject,
    GraphqlContracts,
    UriGraph,
    FlutterInventory,
    Lint,
}

impl CliCommand {
    const ALL: [Self; 8] = [
        Self::AnalyzeFile,
        Self::Pubspec,
        Self::PubspecConfig,
        Self::AnalyzeProject,
        Self::GraphqlContracts,
        Self::UriGraph,
        Self::FlutterInventory,
        Self::Lint,
    ];

    fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|command| command.name() == value)
    }

    const fn name(self) -> &'static str {
        match self {
            Self::AnalyzeFile => "analyze-file",
            Self::Pubspec => "pubspec",
            Self::PubspecConfig => "pubspec-config",
            Self::AnalyzeProject => "analyze-project",
            Self::GraphqlContracts => "graphql-contracts",
            Self::UriGraph => "uri-graph",
            Self::FlutterInventory => "flutter-inventory",
            Self::Lint => "lint",
        }
    }

    const fn summary(self) -> &'static str {
        match self {
            Self::AnalyzeFile => "Analyze one Dart source file",
            Self::Pubspec => "Analyze pubspec package metadata and dependencies",
            Self::PubspecConfig => "Analyze typed pubspec environment and Flutter configuration",
            Self::AnalyzeProject => "Analyze a Dart or Flutter project directory",
            Self::GraphqlContracts => "Build project-level GraphQL operation contracts",
            Self::UriGraph => "Build the project import, export, and part URI graph",
            Self::FlutterInventory => {
                "Aggregate Flutter widgets, routes, assets, and localizations"
            }
            Self::Lint => "Run configured deterministic project lints",
        }
    }

    const fn usage(self) -> &'static str {
        match self {
            Self::AnalyzeFile => "dartscope analyze-file <path> [--compact]",
            Self::Pubspec => "dartscope pubspec <path> [--compact]",
            Self::PubspecConfig => "dartscope pubspec-config <path> [--compact]",
            Self::AnalyzeProject => {
                "dartscope analyze-project <path> [--relative-root] [--skip-symlinks] [--compact]"
            }
            Self::GraphqlContracts => {
                "dartscope graphql-contracts <path> [--env <key=value>]... [--compact]"
            }
            Self::UriGraph => "dartscope uri-graph <path> [--env <key=value>]... [--compact]",
            Self::FlutterInventory => "dartscope flutter-inventory <path> [--compact]",
            Self::Lint => {
                "dartscope lint <project> [--config <path>] [--format <json|sarif>] [--deny-warnings] [--compact]"
            }
        }
    }

    /// The options only this command takes, one per line.
    const fn specific_options(self) -> &'static str {
        match self {
            Self::AnalyzeProject => {
                "  --relative-root        Report the project root as `.` instead of an absolute path\n  --skip-symlinks        Report a rejected symlink as a diagnostic instead of failing\n"
            }
            Self::GraphqlContracts | Self::UriGraph => {
                "  --env <key=value>      Add a Dart compilation-environment entry; repeatable\n"
            }
            Self::Lint => {
                "  --config <path>        Read versioned TOML lint configuration\n  --format <json|sarif>  Select structured output; default: json\n  --deny-warnings        Fail when warning findings are present\n"
            }
            _ => "",
        }
    }

    fn help(self) -> String {
        format!(
            "{}\n\nUSAGE:\n  {}\n\nOPTIONS:\n{}  --compact              Print the JSON on one line instead of indented\n  -h, --help             Print command help",
            self.summary(),
            self.usage(),
            self.specific_options()
        )
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum CliErrorKind {
    Internal,
    Usage,
    Input,
    Configuration,
    Project,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct CliError {
    kind: CliErrorKind,
    message: String,
}

impl CliError {
    fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: CliErrorKind::Internal,
            message: message.into(),
        }
    }

    fn usage(message: impl Into<String>) -> Self {
        Self {
            kind: CliErrorKind::Usage,
            message: message.into(),
        }
    }

    fn input(message: impl Into<String>) -> Self {
        Self {
            kind: CliErrorKind::Input,
            message: message.into(),
        }
    }

    fn configuration(message: impl Into<String>) -> Self {
        Self {
            kind: CliErrorKind::Configuration,
            message: message.into(),
        }
    }

    fn project(message: impl Into<String>) -> Self {
        Self {
            kind: CliErrorKind::Project,
            message: message.into(),
        }
    }

    const fn exit_code(&self) -> u8 {
        match self.kind {
            CliErrorKind::Internal => EXIT_INTERNAL,
            CliErrorKind::Usage => EXIT_USAGE,
            CliErrorKind::Input => EXIT_INPUT,
            CliErrorKind::Configuration => EXIT_CONFIGURATION,
            CliErrorKind::Project => EXIT_PROJECT,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

fn version_text() -> String {
    format!("dartscope {}", env!("CARGO_PKG_VERSION"))
}

fn global_help() -> String {
    let commands = CliCommand::ALL
        .into_iter()
        .map(|command| format!("  {:<20} {}", command.name(), command.summary()))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "DartScope {}\n\nUSAGE:\n  dartscope <COMMAND> [OPTIONS]\n\nCOMMANDS:\n{commands}\n\nOPTIONS:\n  -h, --help     Print help\n  -V, --version  Print version\n\nRun `dartscope help <COMMAND>` for command-specific help.",
        env!("CARGO_PKG_VERSION")
    )
}

#[cfg(all(test, unix))]
mod project_symlink_tests {
    use super::*;
    use std::os::unix::fs::symlink;
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
            let path =
                env::temp_dir().join(format!("dartscope-{label}-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&path).expect("temporary project directory");
            Self { path }
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn cli_allows_in_root_symlink_files() {
        let temp = TempDirectory::new("in-root-symlink");
        fs::create_dir_all(temp.path.join("lib")).unwrap();
        fs::write(temp.path.join("real_source.txt"), "void realFn() {}\n").unwrap();
        symlink("../real_source.txt", temp.path.join("lib/linked.dart")).unwrap();

        let input = collect_project_input(temp.path.to_str().unwrap()).unwrap();

        assert_eq!(input.files.len(), 1);
        assert_eq!(input.files[0].path, "lib/linked.dart");
        assert_eq!(input.files[0].source, "void realFn() {}\n");
    }

    #[test]
    fn cli_rejects_symlink_files_that_escape_the_project_root() {
        let temp = TempDirectory::new("escaping-symlink");
        let root = temp.path.join("project");
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::write(temp.path.join("outside.dart"), "void outside() {}\n").unwrap();
        symlink("../../outside.dart", root.join("lib/escape.dart")).unwrap();

        let error = collect_project_input(root.to_str().unwrap()).unwrap_err();

        assert_eq!(error.kind, CliErrorKind::Input);
        assert!(error.message.contains("input_symlink_rejected"));
        assert!(error.message.contains("outside project root"));
    }

    #[test]
    fn cli_rejects_symlink_directories() {
        let temp = TempDirectory::new("symlink-directory");
        fs::create_dir_all(temp.path.join("target")).unwrap();
        fs::write(temp.path.join("target/inside.dart"), "void inside() {}\n").unwrap();
        symlink("target", temp.path.join("linked-directory")).unwrap();

        let error = collect_project_input(temp.path.to_str().unwrap()).unwrap_err();

        assert_eq!(error.kind, CliErrorKind::Input);
        assert!(error.message.contains("input_symlink_rejected"));
        assert!(
            error
                .message
                .contains("symlinked directories are not supported")
        );
    }

    #[test]
    fn cli_allows_in_root_package_config_symlink_files() {
        let temp = TempDirectory::new("package-config-symlink");
        fs::create_dir_all(temp.path.join(".dart_tool")).unwrap();
        fs::write(temp.path.join("pubspec.yaml"), "name: demo\n").unwrap();
        fs::write(
            temp.path.join("package_config_source.json"),
            r#"{"configVersion":2,"packages":[]}"#,
        )
        .unwrap();
        symlink(
            "../package_config_source.json",
            temp.path.join(".dart_tool/package_config.json"),
        )
        .unwrap();

        let input = collect_project_input(temp.path.to_str().unwrap()).unwrap();

        assert_eq!(input.package_configs.len(), 1);
        assert_eq!(
            input.package_configs[0].path,
            ".dart_tool/package_config.json"
        );
    }

    #[test]
    fn cli_reads_the_validated_symlink_target_after_the_link_is_retargeted() {
        let temp = TempDirectory::new("retargeted-symlink");
        let root_path = temp.path.join("project");
        fs::create_dir_all(root_path.join("lib")).unwrap();
        fs::write(root_path.join("inside.txt"), "void inside() {}\n").unwrap();
        fs::write(temp.path.join("outside.dart"), "void outside() {}\n").unwrap();
        let link = root_path.join("lib/linked.dart");
        symlink("../inside.txt", &link).unwrap();

        let root = resolve_project_root(root_path.to_str().unwrap()).unwrap();
        let file_type = fs::symlink_metadata(&link).unwrap().file_type();
        let validated_read_path = source_file_read_path(&root, &link, &file_type)
            .unwrap()
            .expect("allowed source file");

        fs::remove_file(&link).unwrap();
        symlink("../../outside.dart", &link).unwrap();

        assert_eq!(
            input_limits::read_path(
                &validated_read_path,
                &link,
                input_limits::DEFAULT_INPUT_LIMITS.max_file_bytes,
            )
            .unwrap(),
            "void inside() {}\n"
        );
    }

    #[test]
    fn cli_collects_sources_from_deep_directory_trees_without_recursion() {
        let temp = TempDirectory::new("deep-directory-tree");
        let mut directory = temp.path.clone();
        let mut relative = PathBuf::new();
        for _ in 0..256 {
            directory.push("d");
            relative.push("d");
        }
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("deep.dart"), "void deep() {}\n").unwrap();

        let input = collect_project_input(temp.path.to_str().unwrap()).unwrap();

        assert_eq!(input.files.len(), 1);
        assert_eq!(
            input.files[0].path,
            format!(
                "{}/deep.dart",
                relative.to_string_lossy().replace('\\', "/")
            )
        );
    }
}
#[cfg(test)]
mod project_input_limit_tests {
    use super::*;
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
                "dartscope-input-limit-{label}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(path.join("lib")).expect("temporary project directory");
            Self { path }
        }
    }

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn cli_rejects_a_project_source_over_the_per_file_limit() {
        let temp = TempDirectory::new("file-bytes");
        fs::write(temp.path.join("lib/large.dart"), "12345").unwrap();

        let error = collect_project_sources_with_limits(
            temp.path.to_str().unwrap(),
            false,
            input_limits::InputLimits::new(4, 10, 100),
        )
        .err()
        .expect("input limit error");

        assert_eq!(error.kind, CliErrorKind::Input);
        assert!(error.message.contains("input_file_too_large"));
        assert!(error.message.contains("limit is 4 bytes"));
    }

    #[test]
    fn cli_rejects_projects_over_the_source_file_count_limit() {
        let temp = TempDirectory::new("file-count");
        fs::write(temp.path.join("lib/a.dart"), "a").unwrap();
        fs::write(temp.path.join("lib/b.dart"), "b").unwrap();

        let error = collect_project_sources_with_limits(
            temp.path.to_str().unwrap(),
            false,
            input_limits::InputLimits::new(10, 1, 100),
        )
        .err()
        .expect("input limit error");

        assert_eq!(error.kind, CliErrorKind::Input);
        assert!(error.message.contains("project_input_limit_exceeded"));
        assert!(error.message.contains("source file count"));
        assert!(error.message.contains("limit of 1"));
    }

    #[test]
    fn cli_rejects_projects_over_the_aggregate_source_byte_limit() {
        let temp = TempDirectory::new("project-bytes");
        fs::write(temp.path.join("lib/a.dart"), "1234").unwrap();
        fs::write(temp.path.join("lib/b.dart"), "5678").unwrap();

        let error = collect_project_sources_with_limits(
            temp.path.to_str().unwrap(),
            false,
            input_limits::InputLimits::new(4, 10, 7),
        )
        .err()
        .expect("input limit error");

        assert_eq!(error.kind, CliErrorKind::Input);
        assert!(error.message.contains("project_input_limit_exceeded"));
        assert!(error.message.contains("source byte limit of 7"));
    }

    #[test]
    fn cli_rejects_projects_over_the_directory_entry_limit() {
        let temp = TempDirectory::new("directory-entries");
        fs::write(temp.path.join("noise.txt"), "ignored").unwrap();

        let error = collect_project_sources_with_limits(
            temp.path.to_str().unwrap(),
            false,
            input_limits::InputLimits::new(10, 10, 100).with_traversal_limits(1, 10),
        )
        .err()
        .expect("traversal limit error");

        assert_eq!(error.kind, CliErrorKind::Input);
        assert!(error.message.contains("project_traversal_limit_exceeded"));
        assert!(error.message.contains("directory entry limit of 1"));
    }

    #[test]
    fn cli_reports_the_sorted_first_entry_when_the_entry_limit_is_hit() {
        let temp = TempDirectory::new("directory-entry-order");
        fs::write(temp.path.join("b.txt"), "ignored").unwrap();
        fs::write(temp.path.join("a.dart"), "void a() {}\n").unwrap();

        let error = collect_project_sources_with_limits(
            temp.path.to_str().unwrap(),
            false,
            input_limits::InputLimits::new(10, 10, 100).with_traversal_limits(1, 10),
        )
        .err()
        .expect("traversal limit error");

        assert_eq!(error.kind, CliErrorKind::Input);
        assert!(
            error.message.contains("a.dart"),
            "directory entries are inspected in sorted order, so the first limit diagnostic must name \
             a.dart: {}",
            error.message
        );
    }

    #[test]
    fn cli_rejects_projects_over_the_pending_directory_limit() {
        let temp = TempDirectory::new("pending-directories");
        fs::create_dir(temp.path.join("second_directory")).unwrap();

        let error = collect_project_sources_with_limits(
            temp.path.to_str().unwrap(),
            false,
            input_limits::InputLimits::new(10, 10, 100).with_traversal_limits(10, 1),
        )
        .err()
        .expect("traversal limit error");

        assert_eq!(error.kind, CliErrorKind::Input);
        assert!(error.message.contains("project_traversal_limit_exceeded"));
        assert!(error.message.contains("pending directory limit of 1"));
    }
}
