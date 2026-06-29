use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use dartscope::{
    analyze_file, analyze_graphql_contracts_with_options, analyze_project,
    build_uri_graph_with_options, parse_pubspec, to_json_pretty, DartCompilationEnvironment,
    DartFileInput, DartIndexOptions, DartProjectInput, PackageConfigInput, PubspecInput,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(usage)?;
    let path = args.next().ok_or_else(usage)?;
    let extra_args: Vec<_> = args.collect();

    match command.as_str() {
        "analyze-file" => {
            reject_extra_args(&extra_args)?;
            let source = read_source(&path)?;
            let analysis = analyze_file(DartFileInput::new(path, source));
            println!(
                "{}",
                to_json_pretty(&analysis).map_err(|error| error.to_string())?
            );
        }
        "pubspec" => {
            reject_extra_args(&extra_args)?;
            let source = read_source(&path)?;
            let analysis = parse_pubspec(PubspecInput::new(path, source));
            println!(
                "{}",
                to_json_pretty(&analysis).map_err(|error| error.to_string())?
            );
        }
        "analyze-project" => {
            reject_extra_args(&extra_args)?;
            let input = collect_project_input(&path)?;
            let analysis = analyze_project(input);
            println!(
                "{}",
                to_json_pretty(&analysis).map_err(|error| error.to_string())?
            );
        }
        "graphql-contracts" => {
            let options = parse_index_options(&extra_args)?;
            let input = collect_project_input(&path)?;
            let project = analyze_project(input);
            let analysis = analyze_graphql_contracts_with_options(&project, &options);
            println!(
                "{}",
                to_json_pretty(&analysis).map_err(|error| error.to_string())?
            );
        }
        "uri-graph" => {
            let options = parse_index_options(&extra_args)?;
            let input = collect_project_input(&path)?;
            let project = analyze_project(input);
            let graph = build_uri_graph_with_options(&project, &options);
            println!(
                "{}",
                to_json_pretty(&graph).map_err(|error| error.to_string())?
            );
        }
        _ => return Err(usage()),
    }

    Ok(())
}

fn reject_extra_args(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(format!("unexpected argument: {}", args[0]))
    }
}

fn parse_index_options(args: &[String]) -> Result<DartIndexOptions, String> {
    let mut entries = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--env" => {
                let pair = args.get(index + 1).ok_or_else(|| {
                    "missing value for --env; expected --env key=value".to_string()
                })?;
                entries.push(parse_environment_entry(pair)?);
                index += 2;
            }
            argument => return Err(format!("unexpected argument: {argument}")),
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

fn parse_environment_entry(pair: &str) -> Result<(String, String), String> {
    let Some((key, value)) = pair.split_once('=') else {
        return Err(format!(
            "invalid --env value {pair:?}; expected --env key=value"
        ));
    };
    if key.is_empty() {
        return Err("invalid --env value: key cannot be empty".to_string());
    }
    Ok((key.to_string(), value.to_string()))
}

fn read_source(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("failed to read {path}: {error}"))
}

fn collect_project_input(root: &str) -> Result<DartProjectInput, String> {
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

fn resolve_project_root(root: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(root);
    let path = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map_err(|error| format!("failed to read current directory: {error}"))?
            .join(path)
    };

    let metadata = fs::metadata(&path)
        .map_err(|error| format!("failed to inspect project root {}: {error}", path.display()))?;
    if !metadata.is_dir() {
        return Err(format!(
            "project root is not a directory: {}",
            path.display()
        ));
    }

    Ok(path)
}

fn collect_sources(
    root: &Path,
    directory: &Path,
    files: &mut Vec<DartFileInput>,
    pubspecs: &mut Vec<PubspecInput>,
    package_configs: &mut Vec<PackageConfigInput>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("failed to read directory {}: {error}", directory.display()))?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to read directory entry in {}: {error}",
                directory.display()
            )
        })?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;

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
                    if package_config_path.is_file() {
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

fn read_path(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("failed to read {}: {error}", path.display()))
}

fn relative_path(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
}

fn is_skipped_directory(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".dart_tool" | ".git" | "build" | "target")
    )
}

fn usage() -> String {
    "usage: dartscope <analyze-file|pubspec|analyze-project|graphql-contracts|uri-graph> <path> [--env key=value ...]"
        .to_string()
}
