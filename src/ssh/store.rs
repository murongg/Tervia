use super::*;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

pub struct Store {
    path: PathBuf,
    source: Option<Vec<u8>>,
    loaded: bool,
}

impl Store {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            source: None,
            loaded: false,
        }
    }

    fn source(&self) -> Result<Option<Vec<u8>>, String> {
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        let mut source = Vec::new();
        file.take(1024 * 1024 + 1)
            .read_to_end(&mut source)
            .map_err(|e| e.to_string())?;
        if source.len() > 1024 * 1024 {
            return Err("SSH host file exceeds 1 MiB".into());
        }
        Ok(Some(source))
    }

    pub fn load(&mut self) -> Result<Vec<Host>, String> {
        self.loaded = false;
        let source = self.source()?;
        let hosts = if let Some(source) = &source {
            let value: Value = serde_json::from_slice(source).map_err(|e| e.to_string())?;
            if value["version"] != 1 {
                return Err("Unsupported SSH host file version".into());
            }
            let hosts = value["hosts"]
                .as_array()
                .filter(|h| h.len() <= 256)
                .ok_or("Invalid SSH host list")?;
            let hosts = hosts
                .iter()
                .map(Host::from_json)
                .collect::<Result<Vec<_>, _>>()?;
            validate(&hosts)?;
            hosts
        } else {
            Vec::new()
        };
        self.source = source;
        self.loaded = true;
        Ok(hosts)
    }

    pub fn save(&mut self, hosts: &[Host]) -> Result<(), String> {
        if !self.loaded {
            return Err("SSH hosts were not loaded; fix the file and restart Tervia".into());
        }
        validate(hosts)?;
        if self.source()? != self.source {
            return Err("SSH hosts changed on disk; restart Tervia before saving".into());
        }
        let updated = serde_json::to_vec_pretty(
            &json!({"version":1, "hosts":hosts.iter().map(Host::to_json).collect::<Vec<_>>()}),
        )
        .map_err(|e| e.to_string())?;
        if updated.len() > 1024 * 1024 {
            return Err("SSH host file exceeds 1 MiB".into());
        }
        let parent = self.path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let target = if self.source.is_some() {
            fs::canonicalize(&self.path).map_err(|e| e.to_string())?
        } else {
            self.path.clone()
        };
        let mut temporary = tempfile::NamedTempFile::new_in(target.parent().unwrap_or(parent))
            .map_err(|e| e.to_string())?;
        temporary.write_all(&updated).map_err(|e| e.to_string())?;
        temporary.as_file().sync_all().map_err(|e| e.to_string())?;
        if self.source()? != self.source {
            return Err("SSH hosts changed on disk; restart Tervia before saving".into());
        }
        if self.source.is_none() {
            temporary
                .persist_noclobber(&target)
                .map_err(|e| e.to_string())?;
        } else {
            temporary.persist(&target).map_err(|e| e.to_string())?;
        }
        self.source = Some(updated);
        Ok(())
    }
}

fn validate(hosts: &[Host]) -> Result<(), String> {
    if hosts.len() > 256 {
        return Err("At most 256 SSH hosts are supported".into());
    }
    let mut names = std::collections::HashSet::new();
    for host in hosts {
        host.validate()?;
        if !names.insert(&host.name) {
            return Err("SSH host names must be unique".into());
        }
    }
    Ok(())
}
