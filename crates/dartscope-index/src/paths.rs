use std::path::{Component, Path, PathBuf};

pub(crate) fn parent_path(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|parent| parent.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

pub(crate) fn normalize_joined_path(base: &str, relative: &str) -> String {
    let path = Path::new(base).join(relative);
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized.to_string_lossy().replace('\\', "/")
}

pub(crate) fn has_uri_scheme(uri: &str) -> bool {
    uri.find(':')
        .is_some_and(|colon| !uri[..colon].contains('/'))
}
