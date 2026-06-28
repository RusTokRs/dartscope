use std::env;
use std::fs;
use std::process::ExitCode;

use dartscope::{analyze_file, parse_pubspec, to_json_pretty, DartFileInput, PubspecInput};

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
    let source =
        fs::read_to_string(&path).map_err(|error| format!("failed to read {path}: {error}"))?;

    match command.as_str() {
        "analyze-file" => {
            let analysis = analyze_file(DartFileInput::new(path, source));
            println!(
                "{}",
                to_json_pretty(&analysis).map_err(|error| error.to_string())?
            );
        }
        "pubspec" => {
            let analysis = parse_pubspec(PubspecInput::new(path, source));
            println!(
                "{}",
                to_json_pretty(&analysis).map_err(|error| error.to_string())?
            );
        }
        _ => return Err(usage()),
    }

    Ok(())
}

fn usage() -> String {
    "usage: dartscope <analyze-file|pubspec> <path>".to_string()
}
