use std::fs::File;
use std::io::Read;
use std::path::Path;

use super::CliError;

pub(super) const DEFAULT_INPUT_LIMITS: InputLimits =
    InputLimits::new(8 * 1024 * 1024, 20_000, 256 * 1024 * 1024)
        .with_traversal_limits(250_000, 25_000);
pub(super) const MAX_LINT_CONFIG_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) struct InputLimits {
    pub(super) max_file_bytes: u64,
    max_project_files: usize,
    max_project_bytes: u64,
    max_directory_entries: usize,
    max_pending_directories: usize,
}

impl InputLimits {
    pub(super) const fn new(
        max_file_bytes: u64,
        max_project_files: usize,
        max_project_bytes: u64,
    ) -> Self {
        Self {
            max_file_bytes,
            max_project_files,
            max_project_bytes,
            max_directory_entries: usize::MAX,
            max_pending_directories: usize::MAX,
        }
    }

    pub(super) const fn with_traversal_limits(
        mut self,
        max_directory_entries: usize,
        max_pending_directories: usize,
    ) -> Self {
        self.max_directory_entries = max_directory_entries;
        self.max_pending_directories = max_pending_directories;
        self
    }
}

#[derive(Debug, Default)]
pub(super) struct ProjectInputBudget {
    files: usize,
    bytes: u64,
}

impl ProjectInputBudget {
    fn ensure_can_add(
        &self,
        path: &Path,
        candidate_bytes: u64,
        limits: InputLimits,
    ) -> Result<(), CliError> {
        self.checked_totals(path, candidate_bytes, limits)
            .map(|_| ())
    }

    fn record(
        &mut self,
        path: &Path,
        actual_bytes: u64,
        limits: InputLimits,
    ) -> Result<(), CliError> {
        let (files, bytes) = self.checked_totals(path, actual_bytes, limits)?;
        self.files = files;
        self.bytes = bytes;
        Ok(())
    }

    fn checked_totals(
        &self,
        path: &Path,
        candidate_bytes: u64,
        limits: InputLimits,
    ) -> Result<(usize, u64), CliError> {
        let files = self.files.checked_add(1).ok_or_else(|| {
            project_limit_error(format!(
                "loading {} would overflow the source file counter",
                path.display()
            ))
        })?;
        if files > limits.max_project_files {
            return Err(project_limit_error(format!(
                "loading {} would raise the source file count to {files}, above the limit of {}",
                path.display(),
                limits.max_project_files
            )));
        }

        let bytes = self.bytes.checked_add(candidate_bytes).ok_or_else(|| {
            project_limit_error(format!(
                "loading {} would overflow the aggregate source byte counter",
                path.display()
            ))
        })?;
        if bytes > limits.max_project_bytes {
            return Err(project_limit_error(format!(
                "loading {} would raise loaded source bytes to {bytes}, above the source byte limit of {}",
                path.display(),
                limits.max_project_bytes
            )));
        }

        Ok((files, bytes))
    }
}

#[derive(Debug, Default)]
pub(super) struct ProjectTraversalBudget {
    directory_entries: usize,
}

impl ProjectTraversalBudget {
    pub(super) fn record_directory_entry(
        &mut self,
        path: &Path,
        limits: InputLimits,
    ) -> Result<(), CliError> {
        let directory_entries = self.directory_entries.checked_add(1).ok_or_else(|| {
            project_traversal_limit_error(format!(
                "inspecting {} would overflow the directory entry counter",
                path.display()
            ))
        })?;
        if directory_entries > limits.max_directory_entries {
            return Err(project_traversal_limit_error(format!(
                "inspecting {} would raise the directory entry count to {directory_entries}, above the directory entry limit of {}",
                path.display(),
                limits.max_directory_entries
            )));
        }
        self.directory_entries = directory_entries;
        Ok(())
    }

    pub(super) fn ensure_can_queue_directory(
        &self,
        path: &Path,
        current_pending_directories: usize,
        limits: InputLimits,
    ) -> Result<(), CliError> {
        let pending_directories = current_pending_directories.checked_add(1).ok_or_else(|| {
            project_traversal_limit_error(format!(
                "queueing {} would overflow the pending directory counter",
                path.display()
            ))
        })?;
        self.ensure_pending_directories(path, pending_directories, limits)
    }

    pub(super) fn ensure_pending_directories(
        &self,
        path: &Path,
        pending_directories: usize,
        limits: InputLimits,
    ) -> Result<(), CliError> {
        if pending_directories > limits.max_pending_directories {
            return Err(project_traversal_limit_error(format!(
                "queueing {} would raise pending directories to {pending_directories}, above the pending directory limit of {}",
                path.display(),
                limits.max_pending_directories
            )));
        }
        Ok(())
    }
}

pub(super) fn read_path(
    read_path: &Path,
    display_path: &Path,
    max_bytes: u64,
) -> Result<String, CliError> {
    read_path_with_label(read_path, display_path, max_bytes, None)
}

pub(super) fn read_labeled_path(
    read_path: &Path,
    display_path: &Path,
    max_bytes: u64,
    label: &str,
) -> Result<String, CliError> {
    read_path_with_label(read_path, display_path, max_bytes, Some(label))
}

fn read_path_with_label(
    read_path: &Path,
    display_path: &Path,
    max_bytes: u64,
    label: Option<&str>,
) -> Result<String, CliError> {
    let (file, declared_bytes) = open_regular_file(read_path, display_path, label)?;
    ensure_file_limit(display_path, declared_bytes, max_bytes)?;
    read_opened_file(file, display_path, max_bytes, label)
}

pub(super) fn read_project_path(
    read_path: &Path,
    display_path: &Path,
    limits: InputLimits,
    budget: &mut ProjectInputBudget,
) -> Result<String, CliError> {
    let (file, declared_bytes) = open_regular_file(read_path, display_path, None)?;
    ensure_file_limit(display_path, declared_bytes, limits.max_file_bytes)?;
    budget.ensure_can_add(display_path, declared_bytes, limits)?;

    let source = read_opened_file(file, display_path, limits.max_file_bytes, None)?;
    budget.record(display_path, source.len() as u64, limits)?;
    Ok(source)
}

fn open_regular_file(
    read_path: &Path,
    display_path: &Path,
    label: Option<&str>,
) -> Result<(File, u64), CliError> {
    let file = File::open(read_path).map_err(|error| read_error(display_path, label, error))?;
    let metadata = file
        .metadata()
        .map_err(|error| read_error(display_path, label, error))?;
    if !metadata.is_file() {
        return Err(CliError::input(format!(
            "input_file_rejected: input path is not a regular file: {}",
            display_path.display()
        )));
    }
    Ok((file, metadata.len()))
}

fn read_opened_file(
    file: File,
    display_path: &Path,
    max_bytes: u64,
    label: Option<&str>,
) -> Result<String, CliError> {
    let mut source = String::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_string(&mut source)
        .map_err(|error| read_error(display_path, label, error))?;
    ensure_file_limit(display_path, source.len() as u64, max_bytes)?;
    Ok(source)
}

fn ensure_file_limit(display_path: &Path, bytes: u64, max_bytes: u64) -> Result<(), CliError> {
    if bytes > max_bytes {
        return Err(CliError::input(format!(
            "input_file_too_large: input file {} is {bytes} bytes; limit is {max_bytes} bytes",
            display_path.display()
        )));
    }
    Ok(())
}

fn read_error(display_path: &Path, label: Option<&str>, error: std::io::Error) -> CliError {
    match label {
        Some(label) => CliError::input(format!(
            "failed to read {label} {}: {error}",
            display_path.display()
        )),
        None => CliError::input(format!(
            "failed to read {}: {error}",
            display_path.display()
        )),
    }
}

fn project_limit_error(message: String) -> CliError {
    CliError::input(format!("project_input_limit_exceeded: {message}"))
}

fn project_traversal_limit_error(message: String) -> CliError {
    CliError::input(format!("project_traversal_limit_exceeded: {message}"))
}
