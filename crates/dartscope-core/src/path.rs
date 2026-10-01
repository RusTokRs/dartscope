//! Path normalization shared by the crates.

pub fn normalize_path(path: String) -> String {
    path.replace('\\', "/")
}
