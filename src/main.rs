use std::{env, io, process::ExitCode};

use mmonitor::{Execution, Monitor};

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("mmonitor: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn std::error::Error + Send + Sync>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if matches!(args.as_slice(), [flag] if flag == "--help" || flag == "-h") {
        println!("usage: mmonitor --config <path> check <id> [<id> ...]");
        return Ok(ExitCode::SUCCESS);
    }
    if args.len() < 4 || args[0] != "--config" || args[2] != "check" {
        return Err("usage: mmonitor --config <path> check <id> [<id> ...]".into());
    }

    let monitor = Monitor::from_path(&args[1])?;
    let results = monitor.run(&args[3..])?;
    serde_json::to_writer_pretty(io::stdout().lock(), &results)?;
    println!();

    Ok(
        if results
            .iter()
            .all(|result| result.execution == Execution::Completed)
        {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        },
    )
}
