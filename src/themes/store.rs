use super::*;
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_BYTES: u64 = 128 * 1024;

#[derive(Clone)]
pub struct Store {
    root: PathBuf,
}
#[derive(Clone)]
pub struct Document {
    pub data: Arc<Definition>,
    pub source: String,
}
#[derive(Default)]
pub struct Catalog {
    pub themes: Vec<Document>,
    pub errors: Vec<String>,
}

impl Store {
    pub fn new(config: PathBuf) -> Self {
        Self {
            root: config.with_file_name("themes"),
        }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn path(&self, id: &str) -> Result<PathBuf, String> {
        if !valid_id(id) {
            return Err("Invalid custom theme identifier".into());
        }
        Ok(self.root.join(format!("{id}.toml")))
    }
    pub fn load(&self, id: &str) -> Result<Document, String> {
        let path = self.path(id)?;
        let mut source = String::new();
        fs::File::open(&path)
            .and_then(|file| file.take(MAX_BYTES + 1).read_to_string(&mut source))
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if source.len() as u64 > MAX_BYTES {
            return Err(format!("{}: Theme is too large", path.display()));
        }
        let data =
            Definition::parse(&source).map_err(|error| format!("{}: {error}", path.display()))?;
        if data.id() != id {
            return Err(format!(
                "{}: Theme ID must match its filename",
                path.display()
            ));
        }
        Ok(Document {
            data: Arc::new(data),
            source,
        })
    }
    pub fn list(&self) -> Result<Catalog, String> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Catalog::default()),
            Err(error) => return Err(format!("{}: {error}", self.root.display())),
        };
        let mut catalog = Catalog::default();
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "toml") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            match self.load(id) {
                Ok(document) => catalog.themes.push(document),
                Err(error) => catalog.errors.push(error),
            }
        }
        catalog.themes.sort_by(|a, b| {
            a.data
                .name()
                .cmp(b.data.name())
                .then_with(|| a.data.id().cmp(b.data.id()))
        });
        Ok(catalog)
    }
    pub fn create(&self, data: &Definition) -> Result<Document, String> {
        fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        let mut file = tempfile::Builder::new()
            .prefix(".theme-")
            .suffix(".tmp")
            .tempfile_in(&self.root)
            .map_err(|error| error.to_string())?;
        let mut data = data.clone();
        let suffix = file
            .path()
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.strip_prefix(".theme-"))
            .ok_or("Invalid theme filename")?;
        data.id = format!("user-{suffix}");
        let source = data.to_toml()?;
        file.write_all(source.as_bytes())
            .and_then(|_| file.as_file().sync_all())
            .map_err(|error| error.to_string())?;
        file.persist_noclobber(self.path(data.id())?)
            .map_err(|error| error.to_string())?;
        Ok(Document {
            data: Arc::new(data),
            source,
        })
    }
    pub fn save(&self, previous: &Document, data: &Definition) -> Result<Document, String> {
        if data.id() != previous.data.id() {
            return Err("Theme identity cannot change when editing".into());
        }
        let path = self.path(data.id())?;
        self.check_source(&path, &previous.source)?;
        let target = fs::canonicalize(&path).map_err(|error| error.to_string())?;
        let source = data.update_toml(&previous.source)?;
        let mut file = tempfile::NamedTempFile::new_in(
            target.parent().ok_or("Theme has no parent directory")?,
        )
        .map_err(|error| error.to_string())?;
        file.as_file()
            .set_permissions(
                fs::metadata(&target)
                    .map_err(|error| error.to_string())?
                    .permissions(),
            )
            .map_err(|error| error.to_string())?;
        file.write_all(source.as_bytes())
            .and_then(|_| file.as_file().sync_all())
            .map_err(|error| error.to_string())?;
        // Recheck immediately before replacement, preserving changes made by another editor.
        self.check_source(&path, &previous.source)?;
        file.persist(&target).map_err(|error| error.to_string())?;
        Ok(Document {
            data: Arc::new(data.clone()),
            source,
        })
    }
    pub fn delete(&self, document: &Document) -> Result<(), String> {
        let path = self.path(document.data.id())?;
        self.check_source(&path, &document.source)?;
        fs::remove_file(path).map_err(|error| error.to_string())
    }
    fn check_source(&self, path: &Path, source: &str) -> Result<(), String> {
        let current = fs::read_to_string(path).map_err(|error| error.to_string())?;
        if current != source {
            return Err("Theme changed on disk; reopen it before saving or deleting".into());
        }
        Ok(())
    }
}
