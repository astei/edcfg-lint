use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_DIR_ID: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(test_name: &str) -> Self {
        let id = NEXT_TEMP_DIR_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "edcfg-lint-cli-{test_name}-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(path: impl AsRef<Path>, contents: &str) {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_edcfg-lint"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

#[test]
fn hidden_files_are_checked_only_when_requested() {
    let temp = TempDir::new("hidden");
    write(
        temp.path().join(".editorconfig"),
        "root = true\n\n[*]\ntrim_trailing_whitespace = true\n",
    );
    write(temp.path().join("visible.txt"), "clean\n");
    write(temp.path().join(".hidden.txt"), "trailing whitespace  \n");

    let root = temp.path().to_str().unwrap();
    let default_output = run(&[root]);
    let hidden_output = run(&["--hidden", root]);

    assert!(
        default_output.status.success(),
        "{}",
        stdout(&default_output)
    );
    assert!(stdout(&default_output).contains("Checked 1 files, 0 failed"));

    assert_eq!(hidden_output.status.code(), Some(1));
    assert!(
        stdout(&hidden_output).contains(&temp.path().join(".hidden.txt").display().to_string())
    );
    assert!(stdout(&hidden_output).contains("Checked 3 files, 1 failed"));
}

#[test]
fn config_relative_glob_only_checks_matching_files() {
    let temp = TempDir::new("relative-glob");
    write(
        temp.path().join(".editorconfig"),
        "root = true\n\n[src/*.txt]\ntrim_trailing_whitespace = true\n",
    );
    let matching = temp.path().join("src/matching.txt");
    let non_matching = temp.path().join("other/non_matching.txt");
    write(&matching, "trailing  \n");
    write(&non_matching, "trailing  \n");

    let output = run(&[temp.path().to_str().unwrap()]);
    let stdout = stdout(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout.contains(&matching.display().to_string()));
    assert!(!stdout.contains(&non_matching.display().to_string()));
    assert!(stdout.contains("Checked 2 files, 1 failed"));
}

#[test]
fn multiple_explicit_paths_are_checked_and_failures_are_sorted() {
    let temp = TempDir::new("multiple-paths");
    write(
        temp.path().join(".editorconfig"),
        "root = true\n\n[*]\ntrim_trailing_whitespace = true\n",
    );
    let first = temp.path().join("z-last.txt");
    let second = temp.path().join("a-first.txt");
    write(&first, "trailing  \n");
    write(&second, "also trailing  \n");

    let output = run(&[first.to_str().unwrap(), second.to_str().unwrap()]);
    let stdout = stdout(&output);

    assert_eq!(output.status.code(), Some(1));
    let first_position = stdout.find("a-first.txt").unwrap();
    let last_position = stdout.find("z-last.txt").unwrap();
    assert!(first_position < last_position);
    assert!(stdout.contains("Checked 2 files, 2 failed"));
}

#[test]
fn binary_and_oversized_files_are_excluded_from_the_checked_count() {
    let temp = TempDir::new("skipped-count");
    let clean = temp.path().join("clean.txt");
    let binary = temp.path().join("binary.dat");
    let oversized = temp.path().join("oversized.txt");
    write(&clean, "clean\n");
    write(&binary, "binary \0 content\n");
    write(
        &oversized,
        "this file is deliberately larger than thirty-two bytes\n",
    );

    let output = run(&["--max-file-size", "32", temp.path().to_str().unwrap()]);
    let stdout = stdout(&output);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("Checked 1 files, 0 failed"));
    assert!(!stdout.contains(&binary.display().to_string()));
    assert!(!stdout.contains(&oversized.display().to_string()));
}
