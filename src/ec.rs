use dashmap::DashMap;
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
    is_root: bool,
    sections: Vec<Section>,
}

impl EagerlyParsedEditorConfig {
    /// Eagerly parses the configuration from the given `parser`, consuming it in the process.
    pub fn from_config_parser<R: io::BufRead>(
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

        Ok(EagerlyParsedEditorConfig { is_root, sections })
    }

    /// Eagerly parses the configuration from the given `cfg`, consuming its enclosed reader in the process.
    pub fn from_config_file(cfg: &mut ConfigFile) -> Result<EagerlyParsedEditorConfig, Error> {
        EagerlyParsedEditorConfig::from_config_parser(&mut cfg.reader)
    }
}

impl PropertiesSource for &EagerlyParsedEditorConfig {
    fn apply_to(
        self,
        props: &mut Properties,
        path: impl AsRef<std::path::Path>,
    ) -> Result<(), Error> {
        let path = path.as_ref();
        for section in self.sections.iter() {
            let _ = section.apply_to(props, path);
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
        DashMap<PathBuf, Option<Arc<EagerlyParsedEditorConfig>>>,
    > = OnceLock::new();
    let cache = PARSED_EDITORCONFIG_CACHE.get_or_init(DashMap::new);

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

    while let Some(dir) = current.parent() {
        let config_path = dir.join(EDITORCONFIG_FILE_NAME);

        // Try to get from cache if possible. We cache both positive (this directory has an editorconfig
        // and it's been parsed) and negative (this directory lacks an editorconfig) results.
        if let Some(maybe_found_config) = cache.get(&config_path) {
            if let Some(config) = maybe_found_config.clone() {
                let is_root = config.is_root;
                config.apply_to(&mut properties, abs_path.as_ref())?;
                if is_root {
                    break;
                }
            } else {
                // File doesn't exist or failed to parse, skip
                current = dir;
                continue;
            }
        }

        // Assume we need to load. It is OK if we do duplicate parsing - it's wasted work, but it is
        // fairly cheap. We eagerly parse the editorconfig in each case as we can quickly borrow a
        // reference to it.
        let loaded_config = match ConfigFile::open(&config_path) {
            Ok(mut opened_config) => {
                let parsed = EagerlyParsedEditorConfig::from_config_file(&mut opened_config)?;
                let parsed_arc = Arc::new(parsed);
                cache.insert(config_path.clone(), Some(parsed_arc.clone()));
                parsed_arc
            }
            Err(_) => {
                // File doesn't exist or failed to parse, skip
                current = dir;
                cache.insert(config_path.clone(), None);
                continue;
            }
        };

        let is_root = loaded_config.is_root;
        loaded_config.apply_to(&mut properties, abs_path.as_ref())?;
        if is_root {
            break;
        }

        current = dir;
    }

    Ok(properties)
}
