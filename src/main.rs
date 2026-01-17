mod error;
mod file;

use ignore::{WalkBuilder, WalkState};
use regex::RegexSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::sync::OnceLock;

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};

static DEFAULT_EXCLUDES: OnceLock<RegexSet> = OnceLock::new();

fn get_default_excludes() -> &'static RegexSet {
    DEFAULT_EXCLUDES.get_or_init(|| {
        let patterns = vec![
            // source control related files and folders
            r"\.git/",
            r"\.jj/",
            // package manager, generated, & lock files
            // Cargo (Rust)
            r"Cargo\.lock$",
            r"/target/",
            // Composer (PHP)
            r"composer\.lock$",
            // RubyGems (Ruby)
            r"Gemfile\.lock$",
            // Go Modules (Go)
            r"go\.(mod|sum|work|work\.sum)$",
            // Gradle (Java)
            r"gradle/wrapper/gradle-wrapper\.properties$",
            r"gradlew(\.bat)?$",
            r"(buildscript-)?gradle\.lockfile?$",
            // Maven (Java)
            r"\.mvn/wrapper/maven-wrapper\.properties$",
            r"\.mvn/wrapper/MavenWrapperDownloader\.java$",
            r"mvnw(\.cmd)?$",
            // NodeJS
            r"/node_modules/",
            // npm (NodeJS)
            r"npm-shrinkwrap\.json$",
            r"package-lock\.json$",
            // pip (Python)
            r"Pipfile\.lock$",
            // Poetry (Python)
            r"poetry\.lock$",
            // pnpm (NodeJS)
            r"pnpm-lock\.yaml$",
            // Terraform & OpenTofu
            r"\.terraform\.lock\.hcl$",
            // uv (Python)
            r"uv\.lock$",
            // yarn (NodeJS)
            r"\.pnp\.c?js$",
            r"\.pnp\.loader\.mjs$",
            r"\.yarn/",
            r"yarn\.lock$",
            // font files
            r"\.eot$",
            r"\.otf$",
            r"\.ttf$",
            r"\.woff2?$",
            // image & video formats
            r"\.avif$",
            r"\.gif$",
            r"\.ico$",
            r"\.jpe?g$",
            r"\.pcm$",
            r"\.mp3$",
            r"\.mp4$",
            r"\.p[bgnp]m$",
            r"\.png$",
            r"\.svg$",
            r"\.tiff?$",
            r"\.webp$",
            r"\.wmv$",
            // other binary or container formats
            r"\.bak$",
            r"\.bin$",
            r"\.docx?$",
            r"\.exe$",
            r"\.pdf$",
            r"\.snap$",
            r"\.xlsx?$",
            // archive formats
            r"\.7z$",
            r"\.bz2$",
            r"\.gz$",
            r"\.jar$",
            r"\.tar$",
            r"\.tgz$",
            r"\.war$",
            r"\.zip$",
            // log & (git) patch files
            r"\.log$",
            r"\.patch$",
            // generated or minified CSS and JavaScript files
            r"\.(css|js)\.map$",
            r"min\.(css|js)$",
            // emacs backup files
            r"~$",
        ];
        RegexSet::new(patterns).expect("Failed to compile default exclude patterns")
    })
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Don't emit specific errors.
    #[arg(short, long)]
    concise: bool,

    /// Files to exclude.
    #[arg(long)]
    exclude: Vec<String>,

    /// Path to a directory with files to check, or specific files to check.
    paths: Vec<PathBuf>,
}

fn main() {
    let args = Args::parse();

    // Start from current directory
    let first_path = if args.paths.len() > 0 { args.paths[0].clone() } else { std::env::current_dir().expect("Failed to get current directory") };

    let mut total_files = 0;

    // Walk files, respecting .gitignore
    let (sender, receiver) = channel();
    let excludes = get_default_excludes();

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

    if !args.concise {
        files_with_errors.sort_by_key(|(file_path, _entries)| file_path.to_owned());
        for (file_path, errors) in files_with_errors.iter() {
            println!("✗ {}", file_path.display());
            for error in errors {
                println!("  {}", error);
            }
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

    let content = fs::read(path).map_err(|_| vec![])?;

    let errors = file::check_file_against_editorconfig(&content, &properties);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
