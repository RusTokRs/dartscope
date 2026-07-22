mod lint_command;

use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use dartscope::{
    DartCompilationEnvironment, DartFileInput, DartIndexOptions, DartProjectInput, FlutterArbInput,
    FlutterCatalogInput, FlutterL10nInput, JsonContract, PackageConfigInput, PubspecInput,
    analyze_file_with_flutter, analyze_graphql_contracts_with_options, analyze_project,
    analyze_project_with_flutter, build_uri_graph_with_options,
    extract_flutter_inventory_with_catalogs, parse_pubspec, parse_pubspec_configuration,
    to_json_contract_pretty,
};

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

macro_rules! serialize_contract {
    ($contract:expr, $value:expr) => {
        to_json_contract_pretty($contract, $value)
            .map(CliOutput::success)
            .map_err(|error| {
                CliError::internal(format!("failed to serialize JSON output: {error}"))
            })
    };
}

fn main() -> ExitCode {
    match run(env::args().skip(1)) {
        Ok(output) => {
            println!("{}", output.text);
            ExitCode::from(output.exit_code)
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(error.exit_code())
        }
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
    execute(command, path, &args[2..])
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

fn execute(command: CliCommand, path: &str, extra_args: &[String]) -> Result<CliOutput, CliError> {
    match command {
        CliCommand::AnalyzeFile => {
            reject_extra_args(extra_args, command)?;
            let source = read_source(path)?;
            let analysis = analyze_file_with_flutter(DartFileInput::new(path, source));
            serialize_contract!(JsonContract::FileAnalysis, &analysis)
        }
        CliCommand::Pubspec => {
            reject_extra_args(extra_args, command)?;
            let source = read_source(path)?;
            let analysis = parse_pubspec(PubspecInput::new(path, source));
            serialize_contract!(JsonContract::PubspecAnalysis, &analysis)
        }
        CliCommand::PubspecConfig => {
            reject_extra_args(extra_args, command)?;
            let source = read_source(path)?;
            let analysis = parse_pubspec_configuration(PubspecInput::new(path, source));
            serialize_contract!(JsonContract::PubspecConfiguration, &analysis)
        }
        CliCommand::AnalyzeProject => {
            reject_extra_args(extra_args, command)?;
            let input = collect_project_input(path)?;
            let analysis = analyze_project_with_flutter(input);
            serialize_contract!(JsonContract::ProjectAnalysis, &analysis)
        }
        CliCommand::GraphqlContracts => {
            let options = parse_index_options(extra_args, command)?;
            let input = collect_project_input(path)?;
            let project = analyze_project(input);
            let analysis = analyze_graphql_contracts_with_options(&project, &options);
            serialize_contract!(JsonContract::GraphqlContracts, &analysis)
        }
        CliCommand::UriGraph => {
            let options = parse_index_options(extra_args, command)?;
            let input = collect_project_input(path)?;
            let project = analyze_project(input);
            let graph = build_uri_graph_with_options(&project, &options);
            serialize_contract!(JsonContract::UriGraph, &graph)
        }
        CliCommand::FlutterInventory => {
            reject_extra_args(extra_args, command)?;
            let input = collect_flutter_project_sources(path)?;
            let project = analyze_project(input.dart);
            let inventory = extract_flutter_inventory_with_catalogs(&project, &input.flutter);
            serialize_contract!(JsonContract::FlutterInventory, &inventory)
        }
        CliCommand::Lint => lint_command::execute(path, extra_args),
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
    fs::read_to_string(path)
        .map_err(|error| CliError::input(format!("failed to read {path}: {error}")))
}

struct CollectedProjectSources {
    dart: DartProjectInput,
    flutter: FlutterCatalogInput,
}

#[derive(Default)]
struct ProjectSourceAccumulator {
    files: Vec<DartFileInput>,
    pubspecs: Vec<PubspecInput>,
    package_configs: Vec<PackageConfigInput>,
    l10n_files: Vec<FlutterL10nInput>,
    arb_files: Vec<FlutterArbInput>,
    collect_flutter_catalogs: bool,
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

        CollectedProjectSources {
            dart: DartProjectInput::new(
                root_path.to_string_lossy().into_owned(),
                self.files,
                self.pubspecs,
            )
            .with_package_configs(self.package_configs),
            flutter: FlutterCatalogInput::new(self.l10n_files, self.arb_files),
        }
    }
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
    let root = resolve_project_root(root)?;
    let mut sources = ProjectSourceAccumulator::new(collect_flutter_catalogs);
    collect_sources(&root, &root.logical, &mut sources)?;
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
) -> Result<(), CliError> {
    let mut pending_directories = vec![directory.to_path_buf()];

    while let Some(directory) = pending_directories.pop() {
        let entries = fs::read_dir(&directory).map_err(|error| {
            CliError::input(format!(
                "failed to read directory {}: {error}",
                directory.display()
            ))
        })?;

        for entry in entries {
            let entry = entry.map_err(|error| {
                CliError::input(format!(
                    "failed to read directory entry in {}: {error}",
                    directory.display()
                ))
            })?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|error| {
                CliError::input(format!("failed to inspect {}: {error}", path.display()))
            })?;

            if file_type.is_dir() {
                if !is_skipped_directory(&path) {
                    pending_directories.push(path);
                }
                continue;
            }
            let Some(source_read_path) = source_file_read_path(root, &path, &file_type)? else {
                continue;
            };

            let Some(source_relative_path) = relative_path(&root.logical, &path) else {
                continue;
            };

            match path.file_name().and_then(|name| name.to_str()) {
                Some("l10n.yaml") if sources.collect_flutter_catalogs => {
                    let source = read_path(&source_read_path, &path)?;
                    sources
                        .l10n_files
                        .push(FlutterL10nInput::new(source_relative_path, source));
                }
                Some("pubspec.yaml") => {
                    let source = read_path(&source_read_path, &path)?;
                    sources
                        .pubspecs
                        .push(PubspecInput::new(source_relative_path, source));
                    if let Some(package_root) = path.parent() {
                        let package_config_path =
                            package_root.join(".dart_tool").join("package_config.json");
                        if let Some(package_config_read_path) =
                            optional_source_file_read_path(root, &package_config_path)?
                        {
                            let source =
                                read_path(&package_config_read_path, &package_config_path)?;
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
                    let source = read_path(&source_read_path, &path)?;
                    sources
                        .files
                        .push(DartFileInput::new(source_relative_path, source));
                }
                _ if sources.collect_flutter_catalogs
                    && path.extension().and_then(|extension| extension.to_str()) == Some("arb") =>
                {
                    let source = read_path(&source_read_path, &path)?;
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

fn optional_source_file_read_path(
    root: &ProjectRoot,
    path: &Path,
) -> Result<Option<PathBuf>, CliError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => source_file_read_path(root, path, &metadata.file_type()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CliError::input(format!(
            "failed to inspect {}: {error}",
            path.display()
        ))),
    }
}

fn source_file_read_path(
    root: &ProjectRoot,
    path: &Path,
    file_type: &fs::FileType,
) -> Result<Option<PathBuf>, CliError> {
    if file_type.is_file() {
        return Ok(Some(path.to_path_buf()));
    }
    if !file_type.is_symlink() {
        return Ok(None);
    }

    let target = fs::canonicalize(path).map_err(|error| {
        CliError::input(format!(
            "input_symlink_rejected: failed to resolve symlink {}: {error}",
            path.display()
        ))
    })?;
    if !target.starts_with(&root.canonical) {
        return Err(CliError::input(format!(
            "input_symlink_rejected: symlink {} resolves outside project root {}: {}",
            path.display(),
            root.logical.display(),
            target.display()
        )));
    }

    let metadata = fs::metadata(&target).map_err(|error| {
        CliError::input(format!(
            "input_symlink_rejected: failed to inspect symlink target {}: {error}",
            target.display()
        ))
    })?;
    if metadata.is_dir() {
        return Err(CliError::input(format!(
            "input_symlink_rejected: symlinked directories are not supported: {} -> {}",
            path.display(),
            target.display()
        )));
    }
    if !metadata.is_file() {
        return Err(CliError::input(format!(
            "input_symlink_rejected: symlink target is not a regular file: {} -> {}",
            path.display(),
            target.display()
        )));
    }

    Ok(Some(target))
}

fn read_path(read_path: &Path, display_path: &Path) -> Result<String, CliError> {
    fs::read_to_string(read_path).map_err(|error| {
        CliError::input(format!(
            "failed to read {}: {error}",
            display_path.display()
        ))
    })
}

fn relative_path(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
}

fn is_skipped_directory(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(
            ".dart_tool"
                | ".git"
                | ".idea"
                | ".pub-cache"
                | ".vscode"
                | "build"
                | "coverage"
                | "node_modules"
                | "Pods"
                | "target"
        )
    )
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
            Self::AnalyzeFile => "dartscope analyze-file <path>",
            Self::Pubspec => "dartscope pubspec <path>",
            Self::PubspecConfig => "dartscope pubspec-config <path>",
            Self::AnalyzeProject => "dartscope analyze-project <path>",
            Self::GraphqlContracts => "dartscope graphql-contracts <path> [--env <key=value>]...",
            Self::UriGraph => "dartscope uri-graph <path> [--env <key=value>]...",
            Self::FlutterInventory => "dartscope flutter-inventory <path>",
            Self::Lint => "dartscope lint <project>",
        }
    }

    fn help(self) -> String {
        let options = match self {
            Self::GraphqlContracts | Self::UriGraph => {
                "\nOPTIONS:\n  --env <key=value>  Add a Dart compilation-environment entry; repeatable\n  -h, --help         Print command help"
            }
            Self::Lint => {
                "\nOPTIONS:\n  --config <path>        Read versioned TOML lint configuration\n  --format <json|sarif>  Select structured output; default: json\n  --deny-warnings        Fail when warning findings are present\n  -h, --help             Print command help"
            }
            _ => "\nOPTIONS:\n  -h, --help  Print command help",
        };
        format!(
            "{}\n\nUSAGE:\n  {}\n{options}",
            self.summary(),
            self.usage()
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
            read_path(&validated_read_path, &link).unwrap(),
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
