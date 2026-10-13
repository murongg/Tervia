use std::{fs, path::Path};
mod reads;
mod tree;
mod watch;
pub use reads::{ReadOutcome, Reads};
pub use tree::{Entry, EntryKind, Row, Tree, list};
pub use watch::{Changes, Update, Watch, reconcile};

pub fn text(root: &Path, path: &Path) -> Result<String, String> {
    let bytes = read(root, path)?;
    if bytes.contains(&0) {
        return Err("Binary files cannot be edited as text".into());
    }
    String::from_utf8(bytes).map_err(|_| "File is not UTF-8 text".to_owned())
}

pub fn read(root: &Path, path: &Path) -> Result<Vec<u8>, String> {
    if path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("Path is outside the project".into());
    }
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let absolute = root
        .join(path)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !absolute.starts_with(&root) {
        return Err("File points outside the project".into());
    }
    if fs::metadata(&absolute)
        .map_err(|error| error.to_string())?
        .len()
        > 4 * 1024 * 1024
    {
        return Err("File exceeds the 4 MiB editor limit".into());
    }
    fs::read(absolute).map_err(|error| error.to_string())
}
pub fn write(root: &Path, path: &Path, contents: &str, expected: &str) -> Result<(), String> {
    // Keep external terminal/editor changes instead of overwriting them
    // with a draft based on an older disk snapshot.
    if read(root, path)? != expected.as_bytes() {
        return Err(
            "File changed on disk. Reopen it before saving; your draft is preserved.".into(),
        );
    }
    if fs::symlink_metadata(root.join(path))
        .map_err(|error| error.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Edit the symlink target directly".into());
    }
    fs::write(root.join(path), contents).map_err(|error| error.to_string())
}
