mod error;
mod file;

use ignore::{WalkBuilder, WalkState};
use std::fs;
use std::path::Path;
use std::sync::mpsc::channel;

fn main() {
    // Start from current directory
    let path = std::env::current_dir().expect("Failed to get current directory");

    let mut total_files = 0;

    // Walk files, respecting .gitignore
    let (sender, receiver) = channel();

    WalkBuilder::new(&path).build_parallel().run(|| {
        let my_sender = sender.clone();
        Box::new(move |result| {
            let entry = match result {
                Ok(entry) => entry,
                Err(e) => {
                    eprintln!("Error walking directory: {}", e);
                    return WalkState::Continue;
                }
            };

            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                return WalkState::Continue;
            }

            let file_path = entry.path().to_path_buf();

            if let Ok(Some(mime_type)) = infer::get_from_path(&file_path) {
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
                    return WalkState::Continue;
                }
            }

            let result = check_file(&file_path);
            let _ = my_sender.send((file_path, result));
            WalkState::Continue
        })
    });

    drop(sender);

    let mut files_with_errors = vec![];

    for (file_path, result) in receiver {
        if let Err(errors) = result {
            files_with_errors.push((file_path, errors));
        }
        total_files += 1;
    }

    files_with_errors.sort_by_key(|(file_path, _entries)| file_path.to_owned());
    for (file_path, errors) in files_with_errors.iter() {
        println!("✗ {}", file_path.display());
        for error in errors {
            println!("  {}", error);
        }
    }

    println!(
        "\nChecked {} files, {} failed",
        total_files,
        files_with_errors.len()
    );
    if !files_with_errors.is_empty() {
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
