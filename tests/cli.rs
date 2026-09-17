use std::{fs, process::Command};

#[test]
fn runs_native_checks_end_to_end() {
    let config = std::env::temp_dir().join(format!("mmonitor-test-{}.toml", std::process::id()));
    let database =
        std::env::temp_dir().join(format!("mmonitor-test-{}.sqlite3", std::process::id()));
    let _ = fs::remove_file(&database);
    fs::write(
        &config,
        format!(
            r#"
            [storage]
            path = "{}"

            [checks.memory]
            kind = "memory"
            program = "{}"
            timeout_ms = 3000

            [checks.macos_version]
            kind = "macos_version"
            program = "/usr/bin/sw_vers"
            timeout_ms = 1000
            "#,
            database.display(),
            env!("CARGO_BIN_EXE_check_mmonitor_memory")
        ),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_mmonitor"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "check",
            "memory",
            "macos_version",
        ])
        .output()
        .unwrap();
    assert!(!database.exists());

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let results: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(results[0]["id"], "memory");
    assert_eq!(results[0]["execution"], "completed");
    assert_eq!(results[0]["metrics"].as_array().unwrap().len(), 11);
    assert_eq!(results[1]["id"], "macos_version");
    assert_eq!(results[1]["facts"].as_array().unwrap().len(), 3);

    let first_collection = Command::new(env!("CARGO_BIN_EXE_mmonitor"))
        .args(["--config", config.to_str().unwrap(), "collect"])
        .output()
        .unwrap();
    assert!(
        first_collection.status.success(),
        "{}",
        String::from_utf8_lossy(&first_collection.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&first_collection.stdout)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let second_collection = Command::new(env!("CARGO_BIN_EXE_mmonitor"))
        .args(["--config", config.to_str().unwrap(), "collect"])
        .output()
        .unwrap();
    assert!(second_collection.status.success());
    assert_eq!(second_collection.stdout, b"[]\n");

    let connection = rusqlite::Connection::open(&database).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        2
    );
    drop(connection);
    let _ = fs::remove_file(config);
    let _ = fs::remove_file(database);
}
