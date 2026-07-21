use dashmap::DashMap;
use rustc_hash::FxBuildHasher;
use std::path::{Path, PathBuf};
use std::{
    borrow::Cow,
    io,
    sync::{Arc, OnceLock},
};

use ec4rs::{ConfigFile, ConfigParser, Error, Properties, PropertiesSource, Section};

const EDITORCONFIG_FILE_NAME: &str = ".editorconfig";

/// An eagerly-parsed version of `ec4rs::ConfigParser`. This is done primarily to improve
/// performance.
struct EagerlyParsedEditorConfig {
    loaded_from: PathBuf,
    is_root: bool,
    sections: Vec<Section>,
}

impl EagerlyParsedEditorConfig {
    /// Eagerly parses the configuration from the given `parser`, consuming it in the process.
    pub fn from_config_parser<R: io::BufRead>(
        from: &Path,
        parser: &mut ConfigParser<R>,
    ) -> Result<EagerlyParsedEditorConfig, Error> {
        let is_root = parser.is_root;
        let mut sections = vec![];

        for result in parser {
            if let Ok(section) = result {
                sections.push(section);
            } else if let Err(e) = result {
                return Err(Error::Parse(e));
            }
        }

        Ok(EagerlyParsedEditorConfig {
            // This should be safe: there is necessarily a parent directory that has this `.editorconfig`.
            loaded_from: from.parent().unwrap().to_path_buf(),
            is_root,
            sections,
        })
    }

    /// Eagerly parses the configuration from the given `cfg`, consuming its enclosed reader in the process.
    pub fn from_config_file(cfg: &mut ConfigFile) -> Result<EagerlyParsedEditorConfig, Error> {
        EagerlyParsedEditorConfig::from_config_parser(&cfg.path, &mut cfg.reader)
    }
}

impl PropertiesSource for &EagerlyParsedEditorConfig {
    fn apply_to(
        self,
        props: &mut Properties,
        path: impl AsRef<std::path::Path>,
    ) -> Result<(), Error> {
        let rel_path = path
            .as_ref()
            .strip_prefix(&self.loaded_from)
            .unwrap_or(path.as_ref());
        for section in self.sections.iter() {
            let _ = section.apply_to(props, rel_path);
        }
        Ok(())
    }
}

/// Retrieves the [`ec4rs::Properties`] for a file at the given path.
///
/// This function is similar to [`ec4rs::properties_of`], except all
/// intermediate steps are cached to improve performance.
///
/// This function does not canonicalize the path,
/// but will join relative paths onto the current working directory.
///
/// EditorConfig files are assumed to be named `.editorconfig`.
pub fn properties_of_cached(path: impl AsRef<Path>) -> Result<Properties, Error> {
    static PARSED_EDITORCONFIG_CACHE: OnceLock<
        DashMap<PathBuf, Option<Arc<EagerlyParsedEditorConfig>>, FxBuildHasher>,
    > = OnceLock::new();
    let cache = PARSED_EDITORCONFIG_CACHE.get_or_init(DashMap::default);

    // Get absolute path
    let mut abs_path = Cow::from(path.as_ref());
    if abs_path.is_relative() {
        abs_path = std::env::current_dir()
            .map_err(Error::InvalidCwd)?
            .join(&path)
            .into();
    }

    // Walk up the directory tree collecting and applying config files
    let mut current = abs_path.as_ref();
    let mut properties = Properties::new();

    let mut to_apply = vec![];

    while let Some(dir) = current.parent() {
        let config_path = dir.join(EDITORCONFIG_FILE_NAME);

        // Try to get from cache if possible. We cache both positive (this directory has an editorconfig
        // and it's been parsed) and negative (this directory lacks an editorconfig) results.
        if let Some(maybe_found_config) = cache.get(&config_path) {
            if let Some(config) = maybe_found_config.clone() {
                let is_root = config.is_root;
                to_apply.push(config);
                if is_root {
                    break;
                }
            } else {
                // File doesn't exist or failed to parse, skip
                current = dir;
                continue;
            }
        } else {
            // Assume we need to load. It is OK if we do duplicate parsing - it's wasted work, but it is
            // fairly cheap. We eagerly parse the editorconfig in each case as we can quickly borrow a
            // reference to it.
            let maybe_loaded_config = match ConfigFile::open(&config_path) {
                Ok(mut opened_config) => {
                    let parsed = EagerlyParsedEditorConfig::from_config_file(&mut opened_config)?;
                    Some(Arc::new(parsed))
                }
                Err(_) => {
                    // File doesn't exist or failed to parse, skip
                    None
                }
            };

            cache.insert(config_path.clone(), maybe_loaded_config.clone());

            if let Some(loaded_config) = maybe_loaded_config {
                let is_root = loaded_config.is_root;
                to_apply.push(loaded_config);
                if is_root {
                    break;
                }
            }
        }

        current = dir;
    }

    for config in to_apply.iter().rev() {
        config.apply_to(&mut properties, abs_path.as_ref())?;
    }

    Ok(properties)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ec4rs::property::{Charset, MaxLineLen, TrimTrailingWs};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_DIR_ID: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(test_name: &str) -> Self {
            let id = NEXT_TEMP_DIR_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "edcfg-lint-{test_name}-{}-{id}",
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

    #[test]
    fn nearer_config_overrides_parent_and_cached_result_is_identical() {
        let temp = TempDir::new("precedence");
        write(
            temp.path().join(EDITORCONFIG_FILE_NAME),
            "[*]\ntrim_trailing_whitespace = true\n",
        );
        write(
            temp.path().join("child/.editorconfig"),
            "[*]\ntrim_trailing_whitespace = false\n",
        );
        let target = temp.path().join("child/file.txt");
        write(&target, "content\n");

        let first = properties_of_cached(&target).unwrap();
        write(
            temp.path().join("child/.editorconfig"),
            "[*]\ntrim_trailing_whitespace = true\n",
        );
        let second = properties_of_cached(&target).unwrap();

        assert_eq!(
            first.get::<TrimTrailingWs>().unwrap(),
            TrimTrailingWs::Value(false)
        );
        // Rewriting the config between calls proves the second result came from the
        // positive cache instead of producing the same result via another parse.
        assert_eq!(first, second);
    }

    #[test]
    fn root_config_stops_ancestor_lookup() {
        let temp = TempDir::new("root");
        write(
            temp.path().join(EDITORCONFIG_FILE_NAME),
            "[*]\ntrim_trailing_whitespace = true\n",
        );
        write(
            temp.path().join("child/.editorconfig"),
            "root = true\n\n[*]\ncharset = utf-8\n",
        );
        let target = temp.path().join("child/nested/file.txt");
        write(&target, "content\n");

        let properties = properties_of_cached(&target).unwrap();

        assert_eq!(properties.get::<Charset>().unwrap(), Charset::Utf8);
        assert!(properties.get::<TrimTrailingWs>().is_err());
    }

    #[test]
    fn slash_patterns_are_relative_to_the_config_directory() {
        let temp = TempDir::new("relative-glob");
        write(
            temp.path().join(EDITORCONFIG_FILE_NAME),
            "root = true\n\n[src/*.rs]\nmax_line_length = 80\n",
        );
        let matching = temp.path().join("src/matching.rs");
        let non_matching = temp.path().join("other/non_matching.rs");
        write(&matching, "fn main() {}\n");
        write(&non_matching, "fn main() {}\n");

        let matching_properties = properties_of_cached(&matching).unwrap();
        let non_matching_properties = properties_of_cached(&non_matching).unwrap();

        assert_eq!(
            matching_properties.get::<MaxLineLen>().unwrap(),
            MaxLineLen::Value(80)
        );
        assert!(non_matching_properties.get::<MaxLineLen>().is_err());
    }
}
