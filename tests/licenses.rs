//! `bmseeker licenses` as a program runs it: the texts on stdout, exit code
//! 0 (the window and the web page show what it writes).

use std::process::Command;

fn licenses(json: bool) -> std::process::Output {
    // (a home of no settings file: the defaults)
    let home = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("licenses-home");
    std::fs::create_dir_all(&home).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bmseeker"));
    if json {
        cmd.arg("--json");
    }
    cmd.arg("licenses")
        .env_remove("BMSEEKER_CONFIG")
        .env_remove("APPDATA")
        .env("XDG_CONFIG_HOME", &home)
        .env("HOME", &home)
        .output()
        .unwrap()
}

#[test]
fn licenses_are_written() {
    let text = licenses(false);
    assert_eq!(text.status.code(), Some(0), "{text:?}");
    let stdout = String::from_utf8(text.stdout).unwrap();
    assert!(stdout.starts_with("MIT License\n"), "{}", &stdout[..80]);
    assert!(stdout.contains("\n- `github.com/egoist/mygo`: "));
    assert!(text.stderr.is_empty());

    let json = licenses(true);
    assert_eq!(json.status.code(), Some(0), "{json:?}");
    let stdout = String::from_utf8(json.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1);
    let record: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let written = record["text"].as_str().unwrap();
    assert!(written.starts_with("MIT License\n") && written.contains("\n- `serde` ("));
}
