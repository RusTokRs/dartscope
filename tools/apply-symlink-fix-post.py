from pathlib import Path

main_path = Path("crates/dartscope-cli/src/main.rs")
main_source = main_path.read_text()
obsolete_helper = '''
fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
}
'''
if obsolete_helper not in main_source:
    raise SystemExit("obsolete is_regular_file helper not found")
main_path.write_text(main_source.replace(obsolete_helper, "", 1))

contract_path = Path("crates/dartscope-cli/tests/cli_contract.rs")
contract = contract_path.read_text()
legacy_fixture = '''
    #[cfg(unix)]
    let linked = {
        use std::os::unix::fs::symlink;
        let external = TempDirectory::new("external symlink target");
        write_file(&external.path().join("linked.dart"), "void linked() {}\n");
        symlink(external.path(), project.path().join("linked-source")).expect("create symlink");
        external
    };

'''
if legacy_fixture not in contract:
    raise SystemExit("legacy symlink fixture not found")
contract = contract.replace(legacy_fixture, "", 1)
legacy_assertions = '''    assert!(!json.contains("ignored.dart"));
    assert!(!json.contains("linked.dart"));

    #[cfg(unix)]
    drop(linked);
}
'''
replacement = '''    assert!(!json.contains("ignored.dart"));
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
'''
if legacy_assertions not in contract:
    raise SystemExit("legacy symlink assertions not found")
contract_path.write_text(contract.replace(legacy_assertions, replacement, 1))
