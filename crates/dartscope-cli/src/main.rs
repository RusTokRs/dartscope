use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use dartscope::{
    analyze_file, analyze_project, parse_pubspec, to_json_pretty, DartFileInput, DartProjectInput,
    PubspecInput,
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

    match command.as_str() {
        "analyze-file" => {
            let source = read_source(&path)?;
            let analysis = analyze_file(DartFileInput::new(path, source));
            println!(
                "{}",
                to_json_pretty(&analysis).map_err(|error| error.to_string())?
            );
        }
        "pubspec" => {
            let source = read_source(&path)?;
            let analysis = parse_pubspec(PubspecInput::new(path, source));
            println!(
                "{}",
                to_json_pretty(&analysis).map_err(|error| error.to_string())?
            );
        }
        "analyze-project" => {
            let input = collect_project_input(&path)?;
            let analysis = analyze_project(input);
            println!(
                "{}",
                to_json_pretty(&analysis).map_err(|error| error.to_string())?
            );
        }
        _ => return Err(usage()),
    }

    Ok(())
}

fn read_source(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("failed to read {path}: {error}"))
}

fn collect_project_input(root: &str) -> Result<DartProjectInput, String> {
    let root_path = resolve_project_root(root)?;
    let mut files = Vec::new();
    let mut pubspecs = Vec::new();

    collect_sources(&root_path, &root_path, &mut files, &mut pubspecs)?;

    files.sort_by(|left, right| left.path.cmp(&right.path));
    pubspecs.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(DartProjectInput::new(
        root_path.to_string_lossy().into_owned(),
        files,
        pubspecs,
    ))
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
                collect_sources(root, &path, files, pubspecs)?;
            }
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        let Some(relative_path) = relative_path(root, &path) else {
            continue;
        };

        match path.file_name().and_then(|name| name.to_str()) {
            Some("pubspec.yaml") => {
                let source = read_path(&path)?;
                pubspecs.push(PubspecInput::new(relative_path, source));
            }
            _ if path.extension().and_then(|extension| extension.to_str()) == Some("dart") => {
                let source = read_path(&path)?;
                files.push(DartFileInput::new(relative_path, source));
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
    "usage: dartscope <analyze-file|pubspec|analyze-project> <path>".to_string()
}
