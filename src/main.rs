mod file;

use ignore::WalkBuilder;
use std::fs;
use std::path::Path;

fn main() {
    // Start from current directory
    let path = std::env::current_dir().expect("Failed to get current directory");

    let mut total_files = 0;
    let mut failed_files = 0;

    // Walk files, respecting .gitignore
    for result in WalkBuilder::new(&path).build() {
        let entry = match result {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!("Error walking directory: {}", e);
                continue;
            }
        };

        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }

        let file_path = entry.path();

        // Skip the eddy binary itself and target directory
        if file_path.starts_with(path.join("target")) {
            continue;
        }

        if let Err(e) = check_file(file_path) {
            println!("✗ {}: {}", file_path.display(), e);
            failed_files += 1;
        } else {
            println!("✓ {}", file_path.display());
        }
        total_files += 1;
    }

    println!("\nChecked {} files, {} failed", total_files, failed_files);
    if failed_files > 0 {
        std::process::exit(1);
    }
}

fn check_file(path: &Path) -> Result<(), String> {
    let properties =
        ec4rs::properties_of(path).map_err(|e| format!("Failed to load editorconfig: {}", e))?;

    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;

    if !file::check_file_against_editorconfig(&content, &properties) {
        return Err("failed editorconfig checks".to_string());
    }

    Ok(())
}
