mod config;
mod nagios;
mod normalize;
mod runner;

pub mod model;

use std::{error::Error, fmt, path::Path};

pub use config::CheckKind;
pub use model::{CheckResult, Execution, Metric};

pub struct Monitor {
    config: config::Config,
}

impl Monitor {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Box<dyn Error + Send + Sync>> {
        Ok(Self {
            config: config::load(path)?,
        })
    }

    pub fn check_ids(&self) -> impl Iterator<Item = &str> {
        self.config.checks.keys().map(String::as_str)
    }

    pub fn check_kind(&self, id: &str) -> Option<CheckKind> {
        self.config.checks.get(id).map(|check| check.kind)
    }

    pub fn run<I, S>(&self, ids: I) -> Result<Vec<CheckResult>, MonitorError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let ids = ids
            .into_iter()
            .map(|id| id.as_ref().to_owned())
            .collect::<Vec<_>>();

        for id in &ids {
            if !self.config.checks.contains_key(id) {
                return Err(MonitorError::UnknownCheck(id.clone()));
            }
        }

        Ok(ids
            .into_iter()
            .map(|id| self.run_one(&id))
            .collect::<Vec<_>>())
    }

    fn run_one(&self, id: &str) -> CheckResult {
        let check = &self.config.checks[id];
        let mut raw = runner::run(check);
        let metrics = if raw.execution == Execution::Completed {
            match nagios::parse(&raw.stdout)
                .and_then(|output| normalize::normalize(check.kind, &output))
            {
                Ok(metrics) => metrics,
                Err(error) => {
                    raw.execution = Execution::InvalidOutput;
                    raw.error = Some(error);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        CheckResult {
            id: id.to_owned(),
            execution: raw.execution,
            exit_code: raw.exit_code,
            metrics,
            stdout: raw.stdout,
            stderr: raw.stderr,
            duration_ms: raw.duration_ms,
            error: raw.error,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorError {
    UnknownCheck(String),
}

impl fmt::Display for MonitorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCheck(id) => write!(formatter, "unknown check: {id}"),
        }
    }
}

impl Error for MonitorError {}
