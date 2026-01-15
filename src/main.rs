mod error;
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

        if let Ok(Some(mime_type)) = infer::get_from_path(file_path) {
            let mime_type_accepted = [
                "text/",
                "application/octet-stream",
                "application/ecmascript",
                "application/json",
                "application/x-ndjson",
                "application/xml",
                "+json",
                "+xml",
            ];
            if !mime_type_accepted
                .iter()
                .any(|mt| mime_type.mime_type().contains(mt))
            {
                continue;
            }
        }

        // Skip the eddy binary itself and target directory
        if file_path.starts_with(path.join("target")) {
            continue;
        }

        match check_file(file_path) {
            Err(errors) => {
                println!("✗ {}", file_path.display());
                for error in errors {
                    println!("  {}", error);
                }
                failed_files += 1;
            }
            Ok(()) => {
                println!("✓ {}", file_path.display());
            }
        }
        total_files += 1;
    }

    println!("\nChecked {} files, {} failed", total_files, failed_files);
    if failed_files > 0 {
        std::process::exit(1);
    }
}

fn check_file(path: &Path) -> Result<(), Vec<error::CheckError>> {
    let properties = ec4rs::properties_of(path).map_err(|_| vec![])?;

    let content = fs::read_to_string(path).map_err(|_| vec![])?;

    let errors = file::check_file_against_editorconfig(&content, &properties);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
