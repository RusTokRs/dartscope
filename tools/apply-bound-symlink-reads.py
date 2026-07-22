from pathlib import Path

path = Path("crates/dartscope-cli/src/main.rs")
source = path.read_text(encoding="utf-8")

old = """            if !source_file_is_allowed(root, &path, &file_type)? {
                continue;
            }
"""
new = """            let Some(source_read_path) = source_file_read_path(root, &path, &file_type)? else {
                continue;
            };
"""
if source.count(old) != 1:
    raise SystemExit("source-file validation call not found exactly once")
source = source.replace(old, new, 1)

source = source.replace(
    "let source = read_path(&path)?;",
    "let source = read_path(&source_read_path, &path)?;",
)
if source.count("read_path(&source_read_path, &path)?") != 4:
    raise SystemExit("unexpected project source read replacement count")

collect_start = source.index("fn collect_sources(")
package_at = source.index("if optional_source_file_is_allowed(", collect_start)
package_line_start = source.rfind("\n", 0, package_at) + 1
package_indent = source[package_line_start:package_at]
package_read_at = source.index(
    "let source = read_path(&package_config_path)?;",
    package_at,
)
package_end = source.index("\n", package_read_at) + 1
new_package = (
    package_indent + "if let Some(package_config_read_path) =\n"
    + package_indent + "    optional_source_file_read_path(root, &package_config_path)?\n"
    + package_indent + "{\n"
    + package_indent
    + "    let source = read_path(&package_config_read_path, &package_config_path)?;\n"
)
source = source[:package_line_start] + new_package + source[package_end:]

helper_start = source.index("fn optional_source_file_is_allowed(")
helper_end = source.index("fn relative_path(", helper_start)
helpers = """fn optional_source_file_read_path(
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
        CliError::input(format!("failed to read {}: {error}", display_path.display()))
    })
}

"""
source = source[:helper_start] + helpers + source[helper_end:]

marker = """    #[test]
    fn cli_collects_sources_from_deep_directory_trees_without_recursion() {
"""
test = """    #[test]
    fn cli_reads_the_validated_symlink_target_after_the_link_is_retargeted() {
        let temp = TempDirectory::new("retargeted-symlink");
        let root_path = temp.path.join("project");
        fs::create_dir_all(root_path.join("lib")).unwrap();
        fs::write(root_path.join("inside.txt"), "void inside() {}\\n").unwrap();
        fs::write(temp.path.join("outside.dart"), "void outside() {}\\n").unwrap();
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
            "void inside() {}\\n"
        );
    }

"""
if source.count(marker) != 1:
    raise SystemExit("deep-directory test marker not found")
source = source.replace(marker, test + marker, 1)

path.write_text(source, encoding="utf-8")
