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
        println!("usage: mmonitor --config <path> <check <id> [<id> ...] | collect>");
        return Ok(ExitCode::SUCCESS);
    }
    if args.len() < 3 || args[0] != "--config" {
        return Err("usage: mmonitor --config <path> <check <id> [<id> ...] | collect>".into());
    }

    let monitor = Monitor::from_path(&args[1])?;
    let results = match args[2].as_str() {
        "check" if args.len() >= 4 => monitor.run(&args[3..])?,
        "collect" if args.len() == 3 => monitor.collect()?,
        _ => {
            return Err("usage: mmonitor --config <path> <check <id> [<id> ...] | collect>".into());
        }
    };
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
