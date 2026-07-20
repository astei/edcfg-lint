mod config;
mod ec;
mod error;
mod file;

use ignore::WalkBuilder;
#[cfg(not(feature = "bench-serial-walk"))]
use ignore::WalkState;
use memchr::memchr;
use regex::RegexSet;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(not(feature = "bench-serial-walk"))]
use std::sync::mpsc::channel;

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};

use std::io::{Read, Write};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use human_units::Size;

// musl is a minimal C standard library implementation. As such, it lacks the depth and
// complexity of other system C libraries. Unfortunately, one of the most important things
// it lacks is a fast memory allocator. When linting a large codebase, the default allocator
// has particularly poor performance. Using jemalloc and gets musl's performance *much* more
// competitive with that of glibc.
//
// Performance of using jemalloc is otherwise all over the place. I found results such as
// "20% speedup" (on my M1 Max running Asahi Linux), "22% slower" (on a lower-end VPS with
// shared Broadwell cores), and "no difference" (on a high-end VPS with dedicated Ice Lake
// cores). Thus, I find it prudent just to only enable it when targeting musl.
#[cfg(target_env = "musl")]
use tikv_jemallocator::Jemalloc;

#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Rmit only the total number of files with errors.
    #[arg(short, long)]
    count: bool,

    /// How many threads to use. By default, uses all cores.
    #[arg(short, long, default_value = "0")]
    jobs: usize,

    /// Examine "hidden" directories as well.
    #[arg(long, default_value = "false")]
    hidden: bool,

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

    if args.jobs > 0 {
        walk_builder.threads(args.jobs);
    }

    walk_builder.hidden(!args.hidden);

    let max_file_size = match args.max_file_size {
        Size(0) => None,
        size => usize::try_from(size.0).ok(),
    };

    walk_builder.filter_entry(move |entry| {
        let path_str = entry.path().to_string_lossy();
        !excludes.is_match(&path_str) && !user_provided_excludes.is_match(&path_str)
    });

    let mut files_with_errors = vec![];

    #[cfg(feature = "bench-serial-walk")]
    for result in walk_builder.build() {
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

        let file_path = entry.path().to_path_buf();
        let result = check_file_with_optional_probe(&file_path, max_file_size);
        record_result(file_path, result, &mut total_files, &mut files_with_errors);
    }

    #[cfg(not(feature = "bench-serial-walk"))]
    {
        let (sender, receiver) = channel();

        walk_builder.build_parallel().run(|| {
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
                let result = check_file_with_optional_probe(&file_path, max_file_size);
                let _ = my_sender.send((file_path, result));
                WalkState::Continue
            })
        });

        drop(sender);

        for (file_path, result) in receiver {
            record_result(file_path, result, &mut total_files, &mut files_with_errors);
        }
    }

    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red)));

    if !args.count {
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

fn record_result(
    file_path: PathBuf,
    result: Result<(), Vec<error::CheckError>>,
    total_files: &mut usize,
    files_with_errors: &mut Vec<(PathBuf, Vec<error::CheckError>)>,
) {
    match result {
        Ok(()) => *total_files += 1,
        Err(errors) => {
            if errors.as_slice() != [error::CheckError::Skipped] {
                files_with_errors.push((file_path, errors));
                *total_files += 1;
            }
        }
    }
}

fn check_file_with_optional_probe(
    path: &Path,
    max_file_size: Option<usize>,
) -> Result<(), Vec<error::CheckError>> {
    #[cfg(feature = "bench-legacy-mime-probe")]
    let _ = std::hint::black_box(infer::get_from_path(path));

    check_file(path, max_file_size)
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
        return Err(vec![error::CheckError::Skipped]);
    }

    let mut content = Vec::with_capacity(size.unwrap_or(8000));
    match file.read_to_end(&mut content) {
        Ok(_) => (),
        Err(_) => return Err(vec![error::CheckError::IOError]),
    };

    // skip over potential binary files
    let has_unicode_bom = encoding_rs::Encoding::for_bom(&content).is_some();
    if !has_unicode_bom && memchr(b'\0', &content[..content.len().min(8000)]).is_some() {
        return Err(vec![error::CheckError::Skipped]);
    }

    #[cfg(not(feature = "bench-uncached-resolver"))]
    let properties = ec::properties_of_cached(path).map_err(|_| vec![])?;

    #[cfg(feature = "bench-uncached-resolver")]
    let properties = ec::properties_of_uncached(path).map_err(|_| vec![])?;

    let errors = file::check_file_against_editorconfig(&content, &properties);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CheckError;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_DIR_ID: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(test_name: &str) -> Self {
            let id = NEXT_TEMP_DIR_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "edcfg-lint-harness-{test_name}-{}-{id}",
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

    fn write(path: impl AsRef<Path>, contents: &[u8]) {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn bom_marked_utf16_files_are_checked_instead_of_skipped_as_binary() {
        for (name, charset, contents) in [
            (
                "utf16le.txt",
                "utf-16le",
                b"\xff\xfeB\x00a\x00d\x00 \x00 \x00\n\x00".as_slice(),
            ),
            (
                "utf16be.txt",
                "utf-16be",
                b"\xfe\xff\x00B\x00a\x00d\x00 \x00 \x00\n".as_slice(),
            ),
        ] {
            let temp = TempDir::new(name);
            write(
                temp.path().join(".editorconfig"),
                format!(
                    "root = true\n\n[*]\ncharset = {charset}\ntrim_trailing_whitespace = true\n"
                )
                .as_bytes(),
            );
            let target = temp.path().join(name);
            write(&target, contents);

            let errors = check_file(&target, None).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|error| matches!(error, CheckError::TrailingWhitespace { line: 1 })),
                "expected {name} to reach the text checker, got {errors:?}"
            );
        }
    }

    #[test]
    fn nul_bytes_without_a_bom_are_skipped_as_binary() {
        let temp = TempDir::new("binary");
        write(
            temp.path().join(".editorconfig"),
            b"root = true\n\n[*]\ntrim_trailing_whitespace = true\n",
        );
        let target = temp.path().join("binary.dat");
        write(&target, b"would have trailing whitespace  \0\n");

        assert_eq!(check_file(&target, None), Err(vec![CheckError::Skipped]));
    }

    #[test]
    fn absent_properties_do_not_enable_content_checks() {
        let temp = TempDir::new("unset-properties");
        write(
            temp.path().join(".editorconfig"),
            b"root = true\n\n[*]\nmax_line_length = off\n",
        );
        let target = temp.path().join("unchecked.txt");
        write(&target, b"\tcontent with trailing whitespace  \r\n");

        assert_eq!(check_file(&target, None), Ok(()));
    }

    #[test]
    fn files_larger_than_the_limit_are_skipped_before_content_checks() {
        let temp = TempDir::new("max-size");
        write(
            temp.path().join(".editorconfig"),
            b"root = true\n\n[*]\ntrim_trailing_whitespace = true\n",
        );
        let target = temp.path().join("large.txt");
        write(&target, b"trailing whitespace  \n");

        assert_eq!(check_file(&target, Some(1)), Err(vec![CheckError::Skipped]));
    }
}
