use std::{collections::BTreeMap, error::Error, fs, io, path::Path};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Config {
    pub checks: BTreeMap<String, CheckDefinition>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CheckDefinition {
    pub kind: CheckKind,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    SystemDisk,
    CpuLoad,
    Memory,
}

pub(crate) fn load(path: impl AsRef<Path>) -> Result<Config, Box<dyn Error + Send + Sync>> {
    let source = fs::read_to_string(path)?;
    let config: Config = toml::from_str(&source)?;

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
    fn parses_the_three_check_kinds() {
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
            "#,
        )
        .unwrap();

        assert_eq!(config.checks.len(), 3);
        assert!(config.checks["disk"].args.is_empty());
        assert_eq!(config.checks["cpu"].args, ["-r"]);
    }
}
