//! The files of the project that the client has not opened.
//!
//! The server stays free of filesystem access: whoever runs it hands it the project as text, once
//! when the session starts ([`DartLspServer::load_workspace`]) and again for every file that changes
//! on disk ([`DartLspServer::update_workspace_file`]). [`WorkspaceSource`] is how the protocol
//! layer asks for those files; `crate::fs_workspace::FsWorkspace` reads them from the filesystem and
//! tests supply them from memory.

use std::cell::OnceCell;
use std::cmp::Ordering;
use std::panic::{self, AssertUnwindSafe};

use dartscope_core::{
    DartDeclaration, DartDeclarationKind, DartFileInput, DartProjectInput, PackageConfigInput,
    PubspecInput,
};
use dartscope_index::DartWorkspaceIndex;

use super::{
    DartLspServer, Locator, MAX_NAVIGATION_BYTES, analyze_document, empty_index, index_path,
    symbol_kind,
};
use crate::types::{SymbolInformation, Url};

/// How many results `workspace/symbol` returns at most.
pub const MAX_WORKSPACE_SYMBOLS: usize = 500;

/// One file of the project: its absolute path with `/` separators and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFile {
    pub path: String,
    pub text: String,
}

/// What reading the project from disk produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceScan {
    /// Dart sources, `pubspec.yaml` files and `.dart_tool/package_config.json` files.
    pub files: Vec<WorkspaceFile>,
    /// What the reader left out or stopped at, worth telling the user (a limit was reached).
    pub notes: Vec<String>,
}

/// Where the protocol layer gets the files of the project from.
pub trait WorkspaceSource {
    /// The project files under the directory `root`: every Dart source, every `pubspec.yaml` and the
    /// `.dart_tool/package_config.json` next to it. A directory that does not exist has none.
    fn scan(&self, root: &str) -> WorkspaceScan;

    /// The current text of one file; `None` when it does not exist or cannot be read.
    fn read(&self, path: &str) -> Option<String>;
}

/// A source without files: the project is only what the client opens.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoWorkspace;

impl WorkspaceSource for NoWorkspace {
    fn scan(&self, _root: &str) -> WorkspaceScan {
        WorkspaceScan::default()
    }

    fn read(&self, _path: &str) -> Option<String> {
        None
    }
}

/// A Dart file of the project as it is on disk.
#[derive(Debug)]
pub(super) struct WorkspaceDocument {
    pub(super) uri: Url,
    pub(super) text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfigKind {
    Pubspec,
    PackageConfig,
}

/// A `pubspec.yaml` or `package_config.json` of the project as it is on disk.
#[derive(Debug)]
pub(super) struct ConfigFile {
    pub(super) kind: ConfigKind,
    pub(super) text: String,
}

/// What a path of the project is for the index.
enum FileKind {
    Dart,
    Config(ConfigKind),
}

fn file_kind(path: &str) -> Option<FileKind> {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.ends_with(".dart") {
        Some(FileKind::Dart)
    } else if name == "pubspec.yaml" {
        Some(FileKind::Config(ConfigKind::Pubspec))
    } else if name == "package_config.json" {
        Some(FileKind::Config(ConfigKind::PackageConfig))
    } else {
        None
    }
}

impl DartLspServer {
    /// The directories the client works in, as filesystem paths with `/` separators.
    pub fn workspace_roots(&self) -> &[String] {
        &self.roots
    }

    /// Whether the client can tell the server about file changes
    /// (`workspace/didChangeWatchedFiles`, registered dynamically).
    pub fn watches_files(&self) -> bool {
        self.watches_files
    }

    /// How many Dart files of the project were loaded from disk.
    pub fn workspace_file_count(&self) -> usize {
        self.workspace.len()
    }

    /// Adds the files of the project to what the server knows and rebuilds the index from them and
    /// from the open documents, an open document taking the place of its file. The project is
    /// analyzed in one pass, not file by file, because each incremental update of the index revisits
    /// everything that depends on the file.
    pub fn load_workspace(&mut self, files: Vec<WorkspaceFile>) {
        for file in files {
            self.store_workspace_file(&file.path, file.text);
        }
        self.rebuild_index();
    }

    /// A file of the project changed on disk: `Some(text)` when it was created or changed, `None`
    /// when it was deleted. The text of an open document is not replaced (the buffer is the truth
    /// until it is closed), but what the document is restored to on close is.
    pub fn update_workspace_file(&mut self, path: &str, text: Option<String>) {
        let key = index_path(path);
        let kind = file_kind(&key);
        match text {
            Some(text) => self.store_workspace_file(path, text),
            None => {
                self.workspace.remove(&key);
                self.configs.remove(&key);
            }
        }
        match kind {
            Some(FileKind::Config(_)) => self.rebuild_index(),
            Some(FileKind::Dart) => {
                if !self.documents.contains_key(&key) {
                    self.reindex_from_disk(&key);
                }
            }
            None => {}
        }
    }

    /// Records a file of the project without touching the index.
    fn store_workspace_file(&mut self, path: &str, text: String) {
        let key = index_path(path);
        match file_kind(&key) {
            Some(FileKind::Dart) => {
                if text.len() > MAX_NAVIGATION_BYTES {
                    self.workspace.remove(&key);
                    return;
                }
                let uri = Url::from_file_path(path);
                self.workspace.insert(key, WorkspaceDocument { uri, text });
            }
            Some(FileKind::Config(kind)) => {
                self.configs.insert(key, ConfigFile { kind, text });
            }
            None => {}
        }
    }

    /// Puts the file on disk, if the project has one at `path`, into the index in the place of
    /// whatever was there; without a file the path leaves the index.
    pub(super) fn reindex_from_disk(&mut self, path: &str) {
        let analysis = self.workspace.get(path).map(|document| {
            let text = document.text.clone();
            panic::catch_unwind(AssertUnwindSafe(|| analyze_document(path, text)))
        });
        match analysis {
            Some(Ok(analysis)) => {
                let _ = self.index.upsert_file_with_references(analysis);
            }
            // A bug in the analysis of one file must not end the session.
            Some(Err(_)) | None => {
                let _ = self.index.remove_file(path);
            }
        }
        self.context = OnceCell::new();
    }

    /// Rebuilds the whole index from the project on disk and the open documents.
    fn rebuild_index(&mut self) {
        let mut files: Vec<DartFileInput> = self
            .workspace
            .iter()
            .filter(|(path, _)| !self.documents.contains_key(*path))
            .map(|(path, document)| DartFileInput::new(path.as_str(), document.text.as_str()))
            .chain(
                self.documents
                    .iter()
                    .map(|(path, document)| DartFileInput::new(path.as_str(), document.text.as_str())),
            )
            .collect();
        files.sort_by(|left, right| left.path.cmp(&right.path));
        let mut pubspecs = Vec::new();
        let mut package_configs = Vec::new();
        for (path, config) in &self.configs {
            match config.kind {
                ConfigKind::Pubspec => {
                    pubspecs.push(PubspecInput::new(path.as_str(), config.text.as_str()));
                }
                ConfigKind::PackageConfig => {
                    package_configs.push(PackageConfigInput::new(
                        path.as_str(),
                        config.text.as_str(),
                    ));
                }
            }
        }
        let project = DartProjectInput::new(self.root.clone(), files, pubspecs)
            .with_package_configs(package_configs);
        let built = panic::catch_unwind(AssertUnwindSafe(|| {
            DartWorkspaceIndex::from_reference_project(
                dartscope_parse::analyze_project_with_references(project),
            )
        }));
        self.index = match built {
            Ok(index) => index,
            // One file that the analysis cannot handle must not leave the project without an index:
            // start from nothing and add the files one at a time, skipping the ones that fail.
            Err(_) => self.rebuild_index_file_by_file(),
        };
        for document in self.documents.values_mut() {
            document.analysis_failed = false;
        }
        self.context = OnceCell::new();
    }

    fn rebuild_index_file_by_file(&self) -> DartWorkspaceIndex {
        let mut index = empty_index(&self.root);
        let mut sources: Vec<(&str, &str)> = self
            .workspace
            .iter()
            .filter(|(path, _)| !self.documents.contains_key(*path))
            .map(|(path, document)| (path.as_str(), document.text.as_str()))
            .chain(
                self.documents
                    .iter()
                    .map(|(path, document)| (path.as_str(), document.text.as_str())),
            )
            .collect();
        sources.sort_unstable_by_key(|(path, _)| *path);
        for (path, text) in sources {
            let text = text.to_string();
            if let Ok(analysis) =
                panic::catch_unwind(AssertUnwindSafe(|| analyze_document(path, text)))
            {
                let _ = index.upsert_file_with_references(analysis);
            }
        }
        index
    }

    /// The declarations of the project whose names match `query`, best match first.
    ///
    /// A name matches when it contains the query in any letter case, or has its letters in order
    /// (`hbc` finds `HomeBloc`). An empty query lists everything. Exact matches come first, then
    /// prefixes, substrings and the rest, and at most [`MAX_WORKSPACE_SYMBOLS`] are returned. Local
    /// variables are not symbols of the workspace, and neither is a file the server has no text of.
    pub fn workspace_symbols(&self, query: &str) -> Vec<SymbolInformation> {
        let query = query.trim().to_lowercase();
        let snapshot = self.index.snapshot();
        let mut candidates: Vec<(u8, &str, &DartDeclaration)> = Vec::new();
        for file in &snapshot.project().files {
            for declaration in &file.declarations {
                if declaration.kind == DartDeclarationKind::LocalVariable
                    || declaration.name.is_empty()
                {
                    continue;
                }
                let Some(rank) = match_rank(&declaration.name, &query) else {
                    continue;
                };
                candidates.push((rank, file.path.as_str(), declaration));
            }
        }
        candidates.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.2.name.len().cmp(&right.2.name.len()))
                .then_with(|| left.2.name.cmp(&right.2.name))
                .then_with(|| left.1.cmp(right.1))
                .then_with(|| compare_spans(left.2, right.2))
        });
        let mut locator = Locator::new(&self.documents, &self.workspace);
        let mut symbols = Vec::new();
        for (_, path, declaration) in candidates {
            if symbols.len() >= MAX_WORKSPACE_SYMBOLS {
                break;
            }
            let span = declaration
                .declaration_span
                .as_ref()
                .unwrap_or(&declaration.span);
            let Some(location) = locator.name_location(path, span, &declaration.name) else {
                continue;
            };
            let parent = declaration.parent_symbol_id.as_deref().and_then(|id| {
                snapshot
                    .project()
                    .files
                    .binary_search_by(|file| file.path.as_str().cmp(path))
                    .ok()
                    .and_then(|index| {
                        snapshot.project().files[index]
                            .declarations
                            .iter()
                            .find(|other| other.symbol_id.as_deref() == Some(id))
                    })
            });
            symbols.push(SymbolInformation {
                name: declaration.name.clone(),
                kind: symbol_kind(declaration.kind, parent.map(|parent| parent.kind)),
                location,
                container_name: parent.map(|parent| parent.name.clone()),
            });
        }
        symbols
    }
}

fn compare_spans(left: &DartDeclaration, right: &DartDeclaration) -> Ordering {
    left.span
        .byte_start
        .cmp(&right.span.byte_start)
        .then_with(|| left.kind.cmp(&right.kind))
}

/// How well `name` matches `query` (already lower case): 0 equal, 1 prefix, 2 substring, 3 letters in
/// order, `None` when it does not match.
fn match_rank(name: &str, query: &str) -> Option<u8> {
    if query.is_empty() {
        return Some(3);
    }
    let lowered = name.to_lowercase();
    if lowered == query {
        return Some(0);
    }
    if lowered.starts_with(query) {
        return Some(1);
    }
    if lowered.contains(query) {
        return Some(2);
    }
    let mut wanted = query.chars();
    let mut next = wanted.next();
    for letter in lowered.chars() {
        if next == Some(letter) {
            next = wanted.next();
        }
    }
    next.is_none().then_some(3)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::coordinates::byte_offset_to_lsp_position;
    use crate::types::{
        DidCloseTextDocumentParams, DidOpenTextDocumentParams, InitializeParams, Position,
        SymbolKind, TextDocumentIdentifier, TextDocumentItem,
    };

    fn started() -> DartLspServer {
        let mut server = DartLspServer::new(".");
        let params: InitializeParams = serde_json::from_value(json!({
            "rootUri": "file:///work/app",
            "capabilities": {},
        }))
        .unwrap();
        server.initialize(params).unwrap();
        server
    }

    fn file(path: &str, text: &str) -> WorkspaceFile {
        WorkspaceFile {
            path: path.to_string(),
            text: text.to_string(),
        }
    }

    fn uri(path: &str) -> Url {
        Url::from_file_path(path)
    }

    fn open(server: &mut DartLspServer, path: &str, text: &str) {
        server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri(path),
                language_id: "dart".to_string(),
                version: 1,
                text: text.to_string(),
            },
        });
    }

    fn close(server: &mut DartLspServer, path: &str) {
        server.did_close(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: uri(path) },
        });
    }

    fn position_of(text: &str, needle: &str) -> Position {
        byte_offset_to_lsp_position(text, text.find(needle).unwrap())
    }

    fn names(server: &DartLspServer, query: &str) -> Vec<String> {
        server
            .workspace_symbols(query)
            .into_iter()
            .map(|symbol| symbol.name)
            .collect()
    }

    const WIDGET: &str = "class Widget {\n  void paint() {}\n}\n";
    const MAIN: &str = "import 'widget.dart';\n\nvoid main() {\n  Widget().paint();\n}\n";

    #[test]
    fn definition_reaches_a_file_the_client_has_not_opened() {
        let mut server = started();
        server.load_workspace(vec![
            file("/work/app/lib/widget.dart", WIDGET),
            file("/work/app/lib/main.dart", "void main() {}\n"),
        ]);
        open(&mut server, "/work/app/lib/main.dart", MAIN);

        let at = position_of(MAIN, "Widget()");
        let locations = server
            .definition(&uri("/work/app/lib/main.dart"), at)
            .unwrap()
            .expect("the class is declared in the project");

        assert_eq!(locations[0].uri.as_str(), "file:///work/app/lib/widget.dart");
        assert_eq!(
            locations[0].range.start,
            Position {
                line: 0,
                character: 6
            }
        );
    }

    #[test]
    fn references_include_files_the_client_has_not_opened() {
        let mut server = started();
        server.load_workspace(vec![
            file("/work/app/lib/widget.dart", WIDGET),
            file(
                "/work/app/lib/other.dart",
                "import 'widget.dart';\nvoid other() { Widget(); }\n",
            ),
        ]);
        open(&mut server, "/work/app/lib/main.dart", MAIN);

        let at = position_of(MAIN, "Widget()");
        let locations = server
            .references(&uri("/work/app/lib/main.dart"), at)
            .unwrap()
            .expect("references");

        let files: Vec<&str> = locations
            .iter()
            .map(|location| location.uri.as_str())
            .collect();
        assert!(files.contains(&"file:///work/app/lib/other.dart"), "{files:?}");
        assert!(files.contains(&"file:///work/app/lib/main.dart"), "{files:?}");
    }

    #[test]
    fn an_open_buffer_takes_the_place_of_its_file_until_it_is_closed() {
        let mut server = started();
        server.load_workspace(vec![file("/work/app/lib/a.dart", "class A {}\n")]);
        assert_eq!(names(&server, ""), ["A"]);

        open(&mut server, "/work/app/lib/a.dart", "class B {}\n");
        assert_eq!(names(&server, ""), ["B"]);

        close(&mut server, "/work/app/lib/a.dart");
        assert_eq!(names(&server, ""), ["A"]);

        // A buffer that was never a file leaves nothing behind when it closes.
        open(&mut server, "/work/app/lib/scratch.dart", "class Scratch {}\n");
        assert_eq!(names(&server, "scratch"), ["Scratch"]);
        close(&mut server, "/work/app/lib/scratch.dart");
        assert!(names(&server, "scratch").is_empty());
    }

    #[test]
    fn changes_on_disk_create_replace_and_remove_files() {
        let mut server = started();
        server.load_workspace(vec![file("/work/app/lib/a.dart", "class A {}\n")]);

        server.update_workspace_file("/work/app/lib/a.dart", Some("class A2 {}\n".into()));
        server.update_workspace_file("/work/app/lib/c.dart", Some("class C {}\n".into()));
        assert_eq!(names(&server, ""), ["C", "A2"]);

        server.update_workspace_file("/work/app/lib/a.dart", None);
        assert_eq!(names(&server, ""), ["C"]);
        assert_eq!(server.workspace_file_count(), 1);

        // The buffer of an open document is not replaced by the file changing under it.
        open(&mut server, "/work/app/lib/c.dart", "class FromBuffer {}\n");
        server.update_workspace_file("/work/app/lib/c.dart", Some("class FromDisk {}\n".into()));
        assert_eq!(names(&server, ""), ["FromBuffer"]);
        close(&mut server, "/work/app/lib/c.dart");
        assert_eq!(names(&server, ""), ["FromDisk"]);
    }

    #[test]
    fn package_imports_resolve_through_the_pubspec_of_the_project() {
        let main = "import 'package:app/util.dart';\n\nvoid main() {\n  Util();\n}\n";
        let mut server = started();
        server.load_workspace(vec![
            file("/work/app/pubspec.yaml", "name: app\n"),
            file("/work/app/lib/util.dart", "class Util {}\n"),
        ]);
        open(&mut server, "/work/app/lib/main.dart", main);

        let locations = server
            .definition(&uri("/work/app/lib/main.dart"), position_of(main, "Util()"))
            .unwrap()
            .expect("the package import resolves");

        assert_eq!(locations[0].uri.as_str(), "file:///work/app/lib/util.dart");
    }

    #[test]
    fn package_imports_resolve_through_package_config_json() {
        let main = "import 'package:lib_a/a.dart';\n\nvoid main() {\n  FromA();\n}\n";
        let config = r#"{
  "configVersion": 2,
  "packages": [
    { "name": "lib_a", "rootUri": "../packages/lib_a", "packageUri": "lib/" },
    { "name": "app", "rootUri": "../", "packageUri": "lib/" }
  ]
}"#;
        let mut server = started();
        server.load_workspace(vec![
            file("/work/app/pubspec.yaml", "name: app\n"),
            file("/work/app/.dart_tool/package_config.json", config),
            file("/work/app/packages/lib_a/lib/a.dart", "class FromA {}\n"),
        ]);
        open(&mut server, "/work/app/lib/main.dart", main);

        let locations = server
            .definition(&uri("/work/app/lib/main.dart"), position_of(main, "FromA()"))
            .unwrap()
            .expect("the package configuration resolves the import");

        assert_eq!(
            locations[0].uri.as_str(),
            "file:///work/app/packages/lib_a/lib/a.dart"
        );
    }

    #[test]
    fn workspace_symbols_rank_exact_prefix_substring_and_subsequence_matches() {
        let mut server = started();
        server.load_workspace(vec![
            file(
                "/work/app/lib/a.dart",
                "class HomeBloc {\n  void load() {}\n}\nclass Home {}\nclass Other {\n  int home = 0;\n}\nvoid f() {\n  var local = 1;\n}\nclass MyHomePage {}\n",
            ),
            file("/work/app/lib/b.dart", "class HomeBlocState {}\n"),
        ]);

        // Exact matches (in any letter case) first, then prefixes, then substrings.
        assert_eq!(
            names(&server, "home"),
            ["Home", "home", "HomeBloc", "HomeBlocState", "MyHomePage"]
        );
        // Letters in order.
        assert_eq!(names(&server, "hbs"), ["HomeBlocState"]);
        assert_eq!(names(&server, "  HOMEBLOC "), ["HomeBloc", "HomeBlocState"]);
        // Locals are not symbols of the workspace.
        assert!(names(&server, "local").is_empty());

        let load = server
            .workspace_symbols("load")
            .into_iter()
            .find(|symbol| symbol.name == "load")
            .unwrap();
        assert_eq!(load.kind, SymbolKind::Method);
        assert_eq!(load.container_name.as_deref(), Some("HomeBloc"));
        assert_eq!(load.location.uri.as_str(), "file:///work/app/lib/a.dart");
        assert_eq!(
            load.location.range.start,
            Position {
                line: 1,
                character: 7
            }
        );
    }

    #[test]
    fn a_file_over_the_size_limit_is_not_part_of_the_workspace() {
        let mut server = started();
        let mut big = String::new();
        while big.len() <= MAX_NAVIGATION_BYTES {
            big.push_str("class Big {}\n");
        }
        server.load_workspace(vec![
            file("/work/app/lib/big.dart", &big),
            file("/work/app/lib/small.dart", "class Small {}\n"),
        ]);

        assert_eq!(server.workspace_file_count(), 1);
        assert_eq!(names(&server, ""), ["Small"]);
    }

    #[test]
    fn a_windows_drive_letter_is_one_file_however_the_editor_spells_it() {
        let mut server = started();
        server.load_workspace(vec![file("C:/proj/lib/a.dart", "class A {}\n")]);

        open(&mut server, "/c:/proj/lib/a.dart", "class B {}\n");

        assert_eq!(names(&server, ""), ["B"]);
    }

    #[test]
    fn the_server_without_a_project_knows_only_what_the_client_opens() {
        let mut server = started();
        server.load_workspace(Vec::new());
        assert!(names(&server, "").is_empty());
        open(&mut server, "/work/app/lib/a.dart", "class A {}\n");
        assert_eq!(names(&server, ""), ["A"]);
    }
}
