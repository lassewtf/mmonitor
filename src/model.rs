use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Execution {
    Completed,
    TimedOut,
    SpawnFailed,
    InvalidOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Metric {
    pub name: String,
    pub value: f64,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CheckResult {
    pub id: String,
    pub execution: Execution,
    pub exit_code: Option<i32>,
    pub metrics: Vec<Metric>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub error: Option<String>,
}
