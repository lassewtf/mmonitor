use std::{fs, os::unix::fs::symlink, process::Command};

#[test]
fn mutable_file_validation_rejects_symlinks_without_root_mutation() {
    let installer = include_str!("../install.sh");
    assert!(installer.contains("stat -f '%HT|%Su|%Sg|%Lp'"));
    assert!(installer.contains("Regular File|$SERVICE_USER|$SERVICE_GROUP|640"));
    assert!(!installer.contains("chown \"$SERVICE_USER:$SERVICE_GROUP\" \"$mutable_file\""));
    assert!(!installer.contains("chmod 0640 \"$mutable_file\""));

    let directory =
        std::env::temp_dir().join(format!("mmonitor-install-security-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("target"), "unchanged").unwrap();
    symlink(directory.join("target"), directory.join("database")).unwrap();

    let output = Command::new("/usr/bin/stat")
        .args(["-f", "%HT|%Su|%Sg|%Lp"])
        .arg(directory.join("database"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with("Symbolic Link|")
    );

    fs::remove_dir_all(directory).unwrap();
}
