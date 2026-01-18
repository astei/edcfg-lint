mod config;
mod ec;
mod error;
mod file;

use ignore::{WalkBuilder, WalkState};
use memchr::memchr;
use regex::RegexSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};

use std::io::{Read, Write};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use human_units::Size;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Don't emit specific errors.
    #[arg(short, long)]
    concise: bool,

    /// Files to exclude.
    #[arg(long)]
    exclude: Vec<String>,

    /// Skip files larger than this size. If set to 0, all files are checked irregardless of size.
    #[arg(long, default_value = "1M", value_parser = clap::value_parser!(Size))]
    max_file_size: Size,

    /// Path to a directory with files to check, or specific files to check.
    paths: Vec<PathBuf>,
}

fn main() {
    let args = Args::parse();

    // Start from current directory
    let first_path = if !args.paths.is_empty() {
        args.paths[0].clone()
    } else {
        std::env::current_dir().expect("Failed to get current directory")
    };

    let mut total_files = 0;

    // Walk files, respecting .gitignore
    let (sender, receiver) = channel();
    let excludes = config::get_default_excludes();

    let user_provided_excludes = match RegexSet::new(args.exclude) {
        Ok(user_provided_excludes) => user_provided_excludes,
        Err(err) => {
            let mut cmd = Args::command();
            cmd.error(
                ErrorKind::InvalidValue,
                format!("exclude pattern syntax is invalid: {}", err),
            )
            .exit();
        }
    };

    let mut walk_builder = WalkBuilder::new(&first_path);
    for path in args.paths.iter().skip(1) {
        walk_builder.add(path);
    }

    let max_file_size = match args.max_file_size {
        Size(0) => None,
        size => usize::try_from(size.0).ok(),
    };

    walk_builder
        .filter_entry(move |entry| {
            let path_str = entry.path().to_string_lossy();
            !excludes.is_match(&path_str) && !user_provided_excludes.is_match(&path_str)
        })
        .build_parallel()
        .run(|| {
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

                let result = check_file(&file_path, max_file_size);
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

    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red)));

    if !args.concise {
        files_with_errors
            .sort_by(|(file_path_a, _), (file_path_b, _)| file_path_a.cmp(file_path_b));
        for (file_path, errors) in files_with_errors.iter() {
            let _ = writeln!(&mut stdout, "✗ {}", file_path.display());
            let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)));

            for error in errors {
                println!("  {}", error);
            }

            let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red)));
        }
    }

    let _ = stdout.reset();

    println!(
        "\nChecked {} files, {} failed",
        total_files,
        files_with_errors.len()
    );
    if !files_with_errors.is_empty() {
        std::process::exit(1);
    }
}

fn check_file(path: &Path, max_file_size: Option<usize>) -> Result<(), Vec<error::CheckError>> {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return Err(vec![error::CheckError::IOError]),
    };

    let size = file
        .metadata()
        .map(|m| usize::try_from(m.len()).unwrap_or(usize::MAX))
        .ok();
    if max_file_size.is_some_and(|mfs| size.is_some_and(|file_size| file_size > mfs)) {
        return Ok(());
    }

    let mut content = Vec::with_capacity(size.unwrap_or(8000));
    match file.read_to_end(&mut content) {
        Ok(_) => (),
        Err(_) => return Err(vec![error::CheckError::IOError]),
    };

    // skip over potential binary files
    if memchr(b'\0', &content[..content.len().min(8000)]).is_some() {
        return Ok(());
    }

    let properties = ec::properties_of_cached(path).map_err(|_| vec![])?;

    let errors = file::check_file_against_editorconfig(&content, &properties);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
