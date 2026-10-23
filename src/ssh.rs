//! Host profiles use system OpenSSH for authentication, host trust and terminal sessions.
mod store;
pub use store::Store;

use serde_json::{Value, json};
use std::{ffi::OsString, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Host {
    pub name: String,
    pub address: String,
    pub user: String,
    pub port: Option<u16>,
    pub identity: Option<PathBuf>,
}

impl Host {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty()
            || self.name.len() > 256
            || self.name.chars().any(char::is_control)
        {
            return Err("Enter a host name (up to 256 bytes)".into());
        }
        // OpenSSH can substitute destinations into ProxyCommand; reject shell syntax even with argv isolation.
        if self.address.is_empty()
            || self.address.len() > 1024
            || self.address.starts_with('-')
            || !self
                .address
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-:[]%".contains(&c))
        {
            return Err("Enter an SSH alias, hostname or IP address".into());
        }
        if self.user.len() > 256
            || self.user.starts_with('-')
            || !self
                .user
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return Err("Invalid SSH user".into());
        }
        if self.port == Some(0) {
            return Err("Invalid port".into());
        }
        if self.identity.as_ref().is_some_and(|p| {
            p.as_os_str().is_empty() || p.to_str().is_none_or(|s| s.contains(['\0', '\n', '\r']))
        }) {
            return Err("Invalid identity file path".into());
        }
        Ok(())
    }

    pub fn from_json(value: &Value) -> Result<Self, String> {
        let text = |key: &str, default: &str| -> Result<String, String> {
            value.get(key).map_or(Ok(default.to_owned()), |v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("Invalid SSH {key}"))
            })
        };
        let host = Self {
            name: text("name", "")?,
            address: text("address", "")?,
            user: text("user", "")?,
            port: value
                .get("port")
                .filter(|v| !v.is_null())
                .map(|v| {
                    v.as_u64()
                        .and_then(|p| u16::try_from(p).ok())
                        .ok_or("Invalid SSH port")
                })
                .transpose()?,
            identity: value
                .get("identity")
                .filter(|v| !v.is_null())
                .map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string()))
                .transpose()?,
        };
        host.validate()?;
        Ok(host)
    }

    pub fn to_json(&self) -> Value {
        json!({"name":self.name, "address":self.address, "user":self.user, "port":self.port, "identity":self.identity})
    }

    pub fn arguments(&self) -> Result<Vec<OsString>, String> {
        self.validate()?;
        let mut args: Vec<OsString> = [
            "-o",
            "ConnectTimeout=10",
            "-o",
            "ServerAliveInterval=15",
            "-o",
            "ServerAliveCountMax=2",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        if !self.user.is_empty() {
            args.extend(["-l".into(), self.user.clone().into()]);
        }
        if let Some(port) = self.port {
            args.extend(["-p".into(), port.to_string().into()]);
        }
        if let Some(path) = &self.identity {
            args.extend(["-i".into(), path.as_os_str().to_owned()]);
        }
        args.extend(["--".into(), self.address.clone().into()]);
        Ok(args)
    }
}

/// Enumerate literal aliases only. OpenSSH itself resolves all options, includes and Match rules at connection time.
pub fn aliases(source: &str) -> Result<Vec<Host>, String> {
    if source.len() > 1024 * 1024 {
        return Err("SSH config exceeds 1 MiB".into());
    }
    let mut hosts = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        let Some(end) = line.find(|c: char| c.is_whitespace() || c == '=') else {
            continue;
        };
        if !line[..end].eq_ignore_ascii_case("host") {
            continue;
        }
        let words = shlex::split(line[end..].trim_start_matches(['=', ' ', '\t']))
            .ok_or("Invalid quoting in SSH Host entry")?;
        for alias in words {
            if alias.contains(['*', '?', '!']) || hosts.iter().any(|h: &Host| h.address == alias) {
                continue;
            }
            let host = Host {
                name: alias.clone(),
                address: alias,
                user: String::new(),
                port: None,
                identity: None,
            };
            if host.validate().is_ok() {
                hosts.push(host);
            }
            if hosts.len() > 256 {
                return Err("SSH config has more than 256 hosts".into());
            }
        }
    }
    Ok(hosts)
}
