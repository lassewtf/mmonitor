use std::{fs, process::Command};

#[test]
fn runs_native_checks_end_to_end() {
    let config = std::env::temp_dir().join(format!("mmonitor-test-{}.toml", std::process::id()));
    fs::write(
        &config,
        format!(
            r#"
            [checks.memory]
            kind = "memory"
            program = "{}"
            timeout_ms = 3000

            [checks.macos_version]
            kind = "macos_version"
            program = "/usr/bin/sw_vers"
            timeout_ms = 1000
            "#,
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
    let _ = fs::remove_file(config);

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
}
