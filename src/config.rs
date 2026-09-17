use std::{
    collections::BTreeMap,
    error::Error,
    fs, io,
    path::{Path, PathBuf},
};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Config {
    #[serde(default)]
    pub storage: StorageConfig,
    pub checks: BTreeMap<String, CheckDefinition>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StorageConfig {
    #[serde(default = "default_storage_path")]
    pub path: PathBuf,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: default_storage_path(),
        }
    }
}

fn default_storage_path() -> PathBuf {
    "/opt/ma/mmonitor/data/mmonitor.sqlite3".into()
}

#[derive(Debug, Deserialize)]
pub(crate) struct CheckDefinition {
    pub kind: CheckKind,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub timeout_ms: u64,
    pub interval_seconds: Option<u64>,
    pub store: Option<StoreStrategy>,
}

impl CheckDefinition {
    pub fn interval_seconds(&self) -> u64 {
        self.interval_seconds.unwrap_or(match self.kind {
            CheckKind::SystemDisk => 300,
            CheckKind::CpuLoad | CheckKind::Memory => 60,
            CheckKind::MacosVersion => 3600,
        })
    }

    pub fn store(&self) -> StoreStrategy {
        self.store.unwrap_or(match self.kind {
            CheckKind::MacosVersion => StoreStrategy::OnChange,
            _ => StoreStrategy::Always,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StoreStrategy {
    Always,
    OnChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    SystemDisk,
    CpuLoad,
    Memory,
    MacosVersion,
}

pub(crate) fn load(path: impl AsRef<Path>) -> Result<Config, Box<dyn Error + Send + Sync>> {
    let source = fs::read_to_string(path)?;
    let config: Config = toml::from_str(&source)?;

    if !config.storage.path.is_absolute() {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "storage has a relative path").into(),
        );
    }

    for (id, check) in &config.checks {
        if !Path::new(&check.program).is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("check {id} has a relative program path"),
            )
            .into());
        }
        if check.timeout_ms == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("check {id} has a zero timeout"),
            )
            .into());
        }
        if check.interval_seconds == Some(0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("check {id} has a zero interval"),
            )
            .into());
        }
        if check.args.iter().any(|arg| arg.contains('\0')) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("check {id} has an argument containing NUL"),
            )
            .into());
        }
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_four_check_kinds() {
        let config: Config = toml::from_str(
            r#"
            [checks.disk]
            kind = "system_disk"
            program = "/check_disk"
            timeout_ms = 3000

            [checks.cpu]
            kind = "cpu_load"
            program = "/check_load"
            args = ["-r"]
            timeout_ms = 3000

            [checks.memory]
            kind = "memory"
            program = "/check_memory"
            timeout_ms = 3000

            [checks.macos_version]
            kind = "macos_version"
            program = "/usr/bin/sw_vers"
            timeout_ms = 1000
            "#,
        )
        .unwrap();

        assert_eq!(config.checks.len(), 4);
        assert!(config.checks["disk"].args.is_empty());
        assert_eq!(config.checks["cpu"].args, ["-r"]);
        assert_eq!(config.checks["disk"].interval_seconds(), 300);
        assert_eq!(config.checks["memory"].interval_seconds(), 60);
        assert_eq!(
            config.checks["macos_version"].store(),
            StoreStrategy::OnChange
        );
        assert_eq!(config.storage.path, default_storage_path());
    }
}
