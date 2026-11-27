use std::io::Write;
use std::process::{Command, Output, Stdio};

fn bin() -> Command {
    // Cargo's unused-deps check compiles this file without the variable set.
    let path = option_env!("CARGO_BIN_EXE_csv-peek").unwrap_or("csv-peek");
    Command::new(path)
}

fn run(args: &[&str]) -> Output {
    bin().args(args).output().unwrap()
}

#[test]
fn previews_a_file() {
    let path = std::env::temp_dir().join(format!("csv-peek-basic-{}", std::process::id()));
    std::fs::write(&path, "name,age\nann,3\n").unwrap();
    let out = run(&[path.to_str().unwrap()]);
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "name  age\nann  3\n"
    );
}

#[test]
fn reads_stdin_when_given_a_dash() {
    let mut child = bin()
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"name;age\nann;3\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "name  age\nann  3\n"
    );
}

#[test]
fn missing_file_is_an_error() {
    let out = run(&["does-not-exist.csv"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("does-not-exist.csv"), "{err}");
    assert!(err.starts_with("csv-peek:"), "{err}");
}

#[test]
fn help_lists_the_row_limit() {
    let out = run(&["--help"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("--rows"), "{text}");
    assert!(text.contains("--delimiter"), "{text}");
}
