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
        "name  age\nann   3\n"
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
        "name  age\nann   3\n"
    );
}

#[test]
fn truncates_long_cells_to_the_width() {
    let path = std::env::temp_dir().join(format!("csv-peek-wide-{}", std::process::id()));
    std::fs::write(&path, "name,city\nann,a very long city name\n").unwrap();
    let out = run(&["-w", "16", path.to_str().unwrap()]);
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "name  city\nann   a very lo…\n"
    );
}

#[test]
fn selects_columns() {
    let path = std::env::temp_dir().join(format!("csv-peek-cols-{}", std::process::id()));
    std::fs::write(&path, "name,age,city\nann,3,paris\n").unwrap();
    let out = run(&["--cols", "city, name", "-w", "40", path.to_str().unwrap()]);
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "city   name\nparis  ann\n"
    );
}

#[test]
fn prints_column_stats() {
    let path = std::env::temp_dir().join(format!("csv-peek-stats-{}", std::process::id()));
    std::fs::write(&path, "name,age\nann,10\nbob,\ncara,30\n").unwrap();
    let out = run(&["--stats", "-w", "80", path.to_str().unwrap()]);
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "\
rows: 3
column  type     empty  distinct  min  max  mean
name    string   0      3
age     integer  1      2         10   30   20
"
    );
}

#[test]
fn missing_column_fails() {
    let path = std::env::temp_dir().join(format!("csv-peek-missing-{}", std::process::id()));
    std::fs::write(&path, "name,age\nann,3\n").unwrap();
    let out = run(&["--cols", "nope", path.to_str().unwrap()]);
    let _ = std::fs::remove_file(&path);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("column not found: nope"), "{err}");
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
    assert!(text.contains("--cols"), "{text}");
    assert!(text.contains("--stats"), "{text}");
}
