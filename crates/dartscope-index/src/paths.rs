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

/// Decodes the `%XX` escapes of a relative URI reference into the file name they spell.
///
/// An escape that is not two hexadecimal digits is kept as written, so `100%.dart` still names that
/// file. `None` when decoding would change the structure of the path (an escaped `/`, `\` or NUL) or
/// produce bytes that are not UTF-8: such a reference cannot name a file.
pub(crate) fn percent_decode_relative(uri: &str) -> Option<String> {
    let bytes = uri.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(byte) = bytes.get(index + 1..index + 3).and_then(hex_byte)
        {
            if matches!(byte, b'/' | b'\\' | 0) {
                return None;
            }
            decoded.push(byte);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_byte(pair: &[u8]) -> Option<u8> {
    let high = char::from(*pair.first()?).to_digit(16)?;
    let low = char::from(*pair.get(1)?).to_digit(16)?;
    u8::try_from(high * 16 + low).ok()
}

/// Whether `relative`, resolved against `base_directory`, climbs above the root that both are
/// relative to. Normalizing such a path would silently drop the surplus `..` segments and name a
/// file inside the root that the reference does not mean.
pub(crate) fn climbs_out_of_root(base_directory: &str, relative: &str) -> bool {
    if relative.starts_with('/') {
        return false;
    }
    let mut depth = base_directory
        .split('/')
        .filter(|segment| !matches!(*segment, "" | "."))
        .count();
    for segment in relative.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if depth == 0 {
                    return true;
                }
                depth -= 1;
            }
            _ => depth += 1,
        }
    }
    false
}
