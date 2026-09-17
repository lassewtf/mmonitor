use std::{
    io::{self, Read},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{Execution, config::CheckDefinition};

const OUTPUT_LIMIT: usize = 64 * 1024;

pub(crate) struct RawCheckResult {
    pub execution: Execution,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub error: Option<String>,
}

pub(crate) fn run(check: &CheckDefinition) -> RawCheckResult {
    let started = Instant::now();
    let mut child = match Command::new(&check.program)
        .args(&check.args)
        .stdin(Stdio::null())
        .env("LC_ALL", "C")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return failed(Execution::SpawnFailed, started, error.to_string()),
    };

    let stdout = capture(child.stdout.take().expect("stdout is piped"));
    let stderr = capture(child.stderr.take().expect("stderr is piped"));
    let deadline = started + Duration::from_millis(check.timeout_ms);

    let (status, timed_out) = loop {
        match child.try_wait() {
            Ok(Some(status)) => break (Some(status), false),
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let kill_error = child.kill().err();
                let status = child.wait().ok();
                if let Some(error) = kill_error {
                    return failed(Execution::TimedOut, started, error.to_string());
                }
                break (status, true);
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return failed(Execution::SpawnFailed, started, error.to_string());
            }
        }
    };

    let stdout = match stdout.join() {
        Ok(result) => result,
        Err(_) => return failed(Execution::InvalidOutput, started, "stdout reader panicked"),
    };
    let stderr = match stderr.join() {
        Ok(result) => result,
        Err(_) => return failed(Execution::InvalidOutput, started, "stderr reader panicked"),
    };

    let mut errors = Vec::new();
    let stdout = decode(stdout, "stdout", &mut errors);
    let stderr = decode(stderr, "stderr", &mut errors);

    if timed_out {
        errors.push(format!("timed out after {} ms", check.timeout_ms));
    }

    RawCheckResult {
        execution: if timed_out {
            Execution::TimedOut
        } else if errors.is_empty() {
            Execution::Completed
        } else {
            Execution::InvalidOutput
        },
        exit_code: status.and_then(|status| status.code()),
        stdout,
        stderr,
        duration_ms: elapsed_ms(started),
        error: (!errors.is_empty()).then(|| errors.join("; ")),
    }
}

fn capture(
    mut reader: impl Read + Send + 'static,
) -> thread::JoinHandle<io::Result<(Vec<u8>, bool)>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut chunk = [0; 8192];
        let mut exceeded = false;
        loop {
            let read = reader.read(&mut chunk)?;
            if read == 0 {
                return Ok((bytes, exceeded));
            }
            let remaining = OUTPUT_LIMIT.saturating_sub(bytes.len());
            bytes.extend_from_slice(&chunk[..read.min(remaining)]);
            exceeded |= read > remaining;
        }
    })
}

fn decode(result: io::Result<(Vec<u8>, bool)>, stream: &str, errors: &mut Vec<String>) -> String {
    let (bytes, exceeded) = match result {
        Ok(result) => result,
        Err(error) => {
            errors.push(format!("failed reading {stream}: {error}"));
            return String::new();
        }
    };
    if exceeded {
        errors.push(format!("{stream} exceeded {OUTPUT_LIMIT} bytes"));
    }
    match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => {
            errors.push(format!("{stream} is not UTF-8"));
            String::from_utf8_lossy(error.as_bytes()).into_owned()
        }
    }
}

fn failed(execution: Execution, started: Instant, error: impl Into<String>) -> RawCheckResult {
    RawCheckResult {
        execution,
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        duration_ms: elapsed_ms(started),
        error: Some(error.into()),
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use crate::config::CheckKind;

    use super::*;

    fn check(program: &str, args: &[&str], timeout_ms: u64) -> CheckDefinition {
        CheckDefinition {
            kind: CheckKind::Memory,
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            timeout_ms,
            interval_seconds: None,
            store: None,
        }
    }

    #[test]
    fn reports_spawn_failure() {
        let result = run(&check("/does/not/exist", &[], 100));

        assert_eq!(result.execution, Execution::SpawnFailed);
    }

    #[test]
    fn terminates_a_timed_out_process() {
        let result = run(&check("/bin/sleep", &["1"], 10));

        assert_eq!(result.execution, Execution::TimedOut);
    }
}
