use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use dartscope::{
    DartCompilationEnvironment, DartFileInput, DartIndexOptions, DartProjectInput, JsonContract,
    PackageConfigInput, PubspecInput, analyze_file_with_flutter,
    analyze_graphql_contracts_with_options, analyze_project, analyze_project_with_flutter,
    build_uri_graph_with_options, extract_flutter_inventory, parse_pubspec,
    parse_pubspec_configuration, to_json_contract_pretty,
};

const EXIT_INTERNAL: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_INPUT: u8 = 3;

macro_rules! serialize_contract {
    ($contract:expr, $value:expr) => {
        to_json_contract_pretty($contract, $value).map_err(|error| {
            CliError::internal(format!("failed to serialize JSON output: {error}"))
        })
    };
}

fn main() -> ExitCode {
    match run(env::args().skip(1)) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<String, CliError> {
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
            return Ok(global_help());
        }
        "--version" | "-V" => {
            reject_global_extra_args(&args[1..], "--version")?;
            return Ok(version_text());
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
        return Ok(command.help());
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

fn help_command(args: &[String]) -> Result<String, CliError> {
    match args {
        [] => Ok(global_help()),
        [command] => CliCommand::parse(command)
            .map(CliCommand::help)
            .ok_or_else(|| {
                CliError::usage(format!("unknown command: {command}\n\n{}", global_help()))
            }),
        [_, extra, ..] => Err(CliError::usage(format!("unexpected argument: {extra}"))),
    }
}

fn execute(command: CliCommand, path: &str, extra_args: &[String]) -> Result<String, CliError> {
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
            let input = collect_project_input(path)?;
            let project = analyze_project(input);
            let inventory = extract_flutter_inventory(&project);
            serialize_contract!(JsonContract::FlutterInventory, &inventory)
        }
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

fn collect_project_input(root: &str) -> Result<DartProjectInput, CliError> {
    let root_path = resolve_project_root(root)?;
    let mut files = Vec::new();
    let mut pubspecs = Vec::new();
    let mut package_configs = Vec::new();

    collect_sources(
        &root_path,
        &root_path,
        &mut files,
        &mut pubspecs,
        &mut package_configs,
    )?;

    files.sort_by(|left, right| left.path.cmp(&right.path));
    pubspecs.sort_by(|left, right| left.path.cmp(&right.path));
    package_configs.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(
        DartProjectInput::new(root_path.to_string_lossy().into_owned(), files, pubspecs)
            .with_package_configs(package_configs),
    )
}

fn resolve_project_root(root: &str) -> Result<PathBuf, CliError> {
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

    Ok(path)
}

fn collect_sources(
    root: &Path,
    directory: &Path,
    files: &mut Vec<DartFileInput>,
    pubspecs: &mut Vec<PubspecInput>,
    package_configs: &mut Vec<PackageConfigInput>,
) -> Result<(), CliError> {
    let entries = fs::read_dir(directory).map_err(|error| {
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

        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if !is_skipped_directory(&path) {
                collect_sources(root, &path, files, pubspecs, package_configs)?;
            }
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let Some(source_relative_path) = relative_path(root, &path) else {
            continue;
        };

        match path.file_name().and_then(|name| name.to_str()) {
            Some("pubspec.yaml") => {
                let source = read_path(&path)?;
                pubspecs.push(PubspecInput::new(source_relative_path, source));
                if let Some(package_root) = path.parent() {
                    let package_config_path =
                        package_root.join(".dart_tool").join("package_config.json");
                    if is_regular_file(&package_config_path) {
                        let source = read_path(&package_config_path)?;
                        if let Some(relative_path) = relative_path(root, &package_config_path) {
                            package_configs.push(PackageConfigInput::new(relative_path, source));
                        }
                    }
                }
            }
            _ if path.extension().and_then(|extension| extension.to_str()) == Some("dart") => {
                let source = read_path(&path)?;
                files.push(DartFileInput::new(source_relative_path, source));
            }
            _ => {}
        }
    }

    Ok(())
}

fn read_path(path: &Path) -> Result<String, CliError> {
    fs::read_to_string(path)
        .map_err(|error| CliError::input(format!("failed to read {}: {error}", path.display())))
}

fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
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
}

impl CliCommand {
    const ALL: [Self; 7] = [
        Self::AnalyzeFile,
        Self::Pubspec,
        Self::PubspecConfig,
        Self::AnalyzeProject,
        Self::GraphqlContracts,
        Self::UriGraph,
        Self::FlutterInventory,
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
        }
    }

    fn help(self) -> String {
        let options = if matches!(self, Self::GraphqlContracts | Self::UriGraph) {
            "\nOPTIONS:\n  --env <key=value>  Add a Dart compilation-environment entry; repeatable\n  -h, --help         Print command help"
        } else {
            "\nOPTIONS:\n  -h, --help  Print command help"
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

    const fn exit_code(&self) -> u8 {
        match self.kind {
            CliErrorKind::Internal => EXIT_INTERNAL,
            CliErrorKind::Usage => EXIT_USAGE,
            CliErrorKind::Input => EXIT_INPUT,
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
