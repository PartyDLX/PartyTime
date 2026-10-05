//! Where the console keeps its configuration, and what it remembers between launches.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

use crate::theme::ThemePreference;

/// Environment variable that overrides the configuration directory.
///
/// Used by tests and by anyone running two consoles side by side.
pub const CONFIG_DIR_ENV: &str = "PARTYTIME_CONFIG_DIR";

/// Default platform origin for the OpenParty app.
///
/// Overridable in [`ConsoleConfig::default_origin`]; production builds should set
/// `PARTYTIME_ORIGIN` at build time rather than shipping a development URL.
pub const DEFAULT_ORIGIN: &str = "https://openparty.dlxstudios.com";

/// Environment variable that overrides the platform origin.
pub const ORIGIN_ENV: &str = "PARTYTIME_ORIGIN";

/// The console's directories and files on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// Root of the console's configuration.
    pub config_dir: PathBuf,
    /// Where profiles live.
    pub profiles_dir: PathBuf,
    /// Where log files land.
    pub logs_dir: PathBuf,
}

impl Paths {
    /// The remembered settings file.
    #[must_use]
    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("console.json")
    }

    /// Resolves the configuration directory.
    ///
    /// `PARTYTIME_CONFIG_DIR` wins, then `XDG_CONFIG_HOME/partytime`, then
    /// `~/.config/partytime`. The platform origin is read separately: a console with
    /// no home directory is still usable with an explicit override.
    pub fn resolve() -> Result<Self, PathsError> {
        let config_dir = match std::env::var_os(CONFIG_DIR_ENV) {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => {
                let base = match std::env::var_os("XDG_CONFIG_HOME") {
                    Some(base) if !base.is_empty() => PathBuf::from(base),
                    _ => home_dir().ok_or(PathsError::NoHome)?.join(".config"),
                };
                base.join("partytime")
            }
        };
        Ok(Self {
            profiles_dir: config_dir.join("profiles"),
            logs_dir: config_dir.join("logs"),
            config_dir,
        })
    }

    /// Builds paths rooted at an explicit directory.
    #[must_use]
    pub fn under(config_dir: impl Into<PathBuf>) -> Self {
        let config_dir = config_dir.into();
        Self {
            profiles_dir: config_dir.join("profiles"),
            logs_dir: config_dir.join("logs"),
            config_dir,
        }
    }

    /// Creates the directories the console writes to.
    pub fn ensure(&self) -> Result<(), PathsError> {
        for dir in [&self.config_dir, &self.profiles_dir, &self.logs_dir] {
            std::fs::create_dir_all(dir).map_err(|source| PathsError::Create {
                path: dir.clone(),
                source,
            })?;
        }
        Ok(())
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// What the console remembers between launches.
///
/// This is what "already bootstrapped" means: a chosen profile and a chosen party. The
/// session cookie is deliberately not persisted — a creator signs in each launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConsoleConfig {
    /// Platform origin every request goes to.
    pub origin: String,
    /// Profile to load on launch.
    pub profile: Option<String>,
    /// Party to publish into.
    pub party: Option<String>,
    /// The OpenParty account the console opens with.
    pub account: Option<String>,
    /// Which appearance the console renders in.
    ///
    /// Deserialised leniently on purpose. Every other field failing takes the whole file
    /// and sends the creator back through onboarding; losing a profile and a party to an
    /// appearance value written by a newer build would be a poor trade.
    #[serde(default, deserialize_with = "appearance_from_str")]
    pub appearance: ThemePreference,
}

impl Default for ConsoleConfig {
    fn default() -> Self {
        Self {
            origin: default_origin(),
            profile: None,
            party: None,
            account: None,
            appearance: ThemePreference::System,
        }
    }
}

impl ConsoleConfig {
    /// Whether the console may skip onboarding.
    ///
    /// Both a profile and a party must be remembered. One without the other sends the
    /// creator back through the step they never finished, which is cheaper than opening
    /// a console pointed at a party they did not choose.
    #[must_use]
    pub fn is_bootstrapped(&self) -> bool {
        self.profile.as_ref().is_some_and(|p| !p.is_empty())
            && self.party.as_ref().is_some_and(|p| !p.is_empty())
    }

    /// Reads the settings file, falling back to defaults when it is absent.
    ///
    /// A corrupt settings file is not fatal: the console starts unconfigured and the
    /// user is sent through onboarding, which is recoverable. Losing the file is not.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Writes the settings file.
    pub fn save(&self, path: &Path) -> Result<(), PathsError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| PathsError::Create {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|source| PathsError::Serialize {
            source: Box::new(source),
        })?;
        std::fs::write(path, text).map_err(|source| PathsError::Write {
            path: path.to_path_buf(),
            source,
        })
    }
}

/// Reads an appearance value, falling back to following the system.
fn appearance_from_str<'de, D>(deserializer: D) -> Result<ThemePreference, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(ThemePreference::parse(&raw))
}

/// Records the appearance choice in the settings file, leaving every other field alone.
///
/// Read-modify-write rather than a separate file so the console keeps one settings
/// document. A failure is reported to the caller and the caller decides how loudly to
/// complain: not being able to remember a preference must never stop the user choosing it.
pub fn remember_appearance(preference: ThemePreference) -> Result<(), PathsError> {
    remember_appearance_in(&Paths::resolve()?, preference)
}

/// [`remember_appearance`] against an explicit directory.
///
/// The split exists so the write itself can be tested without mutating process-wide
/// environment state, which is `unsafe` in this edition and racy across parallel tests.
pub fn remember_appearance_in(
    paths: &Paths,
    preference: ThemePreference,
) -> Result<(), PathsError> {
    let mut config = ConsoleConfig::load(&paths.settings_file());
    config.appearance = preference;
    config.save(&paths.settings_file())
}

/// The platform origin, honouring [`ORIGIN_ENV`].
#[must_use]
pub fn default_origin() -> String {
    std::env::var(ORIGIN_ENV)
        .ok()
        .filter(|origin| !origin.is_empty())
        .unwrap_or_else(|| DEFAULT_ORIGIN.to_string())
}

/// Failure while resolving or writing the console's configuration.
#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    /// No home directory and no explicit override.
    #[error("no home directory is set and PARTYTIME_CONFIG_DIR is not set")]
    NoHome,
    /// A directory could not be created.
    #[error("could not create {path}")]
    Create {
        /// Path that failed.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The settings file could not be written.
    #[error("could not write {path}")]
    Write {
        /// Path that failed.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The settings file could not be serialised.
    #[error("could not serialise the console settings")]
    Serialize {
        /// Underlying error.
        #[source]
        source: Box<serde_json::Error>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pt-paths-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn paths_derive_the_profile_and_log_directories_from_the_config_directory() {
        let paths = Paths::under("/tmp/x");
        assert_eq!(paths.profiles_dir, PathBuf::from("/tmp/x/profiles"));
        assert_eq!(paths.logs_dir, PathBuf::from("/tmp/x/logs"));
        assert_eq!(paths.settings_file(), PathBuf::from("/tmp/x/console.json"));
    }

    #[test]
    fn ensure_creates_every_directory_the_console_writes_to() {
        let dir = temp("ensure");
        let paths = Paths::under(&dir);
        paths.ensure().expect("ensure");
        assert!(paths.config_dir.is_dir());
        assert!(paths.profiles_dir.is_dir());
        assert!(paths.logs_dir.is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_absent_settings_file_loads_as_defaults_rather_than_failing() {
        let dir = temp("absent");
        let config = ConsoleConfig::load(&dir.join("console.json"));
        assert_eq!(config, ConsoleConfig::default());
        assert!(!config.is_bootstrapped());
    }

    #[test]
    fn a_corrupt_settings_file_loads_as_defaults_rather_than_failing_the_launch() {
        let dir = temp("corrupt");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("console.json"), "{ not json").expect("write");
        let config = ConsoleConfig::load(&dir.join("console.json"));
        assert!(
            !config.is_bootstrapped(),
            "a bad file sends the user through onboarding"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_round_trips_through_the_settings_file() {
        let dir = temp("roundtrip");
        let config = ConsoleConfig {
            origin: "http://localhost:5174".into(),
            profile: Some("Friday Night".into()),
            party: Some("party:abc".into()),
            account: Some("ada".into()),
            ..ConsoleConfig::default()
        };
        config.save(&dir.join("console.json")).expect("save");
        assert_eq!(ConsoleConfig::load(&dir.join("console.json")), config);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_appearance_choice_round_trips_and_leaves_every_other_field_alone() {
        let dir = temp("appearance");
        let paths = Paths::under(&dir);
        ConsoleConfig {
            profile: Some("Friday Night".into()),
            party: Some("party:abc".into()),
            ..ConsoleConfig::default()
        }
        .save(&paths.settings_file())
        .expect("save");

        for preference in [
            ThemePreference::Dark,
            ThemePreference::Light,
            ThemePreference::System,
        ] {
            remember_appearance_in(&paths, preference).expect("remember");
            let loaded = ConsoleConfig::load(&paths.settings_file());
            assert_eq!(loaded.appearance, preference);
            assert_eq!(loaded.profile.as_deref(), Some("Friday Night"));
            assert_eq!(loaded.party.as_deref(), Some("party:abc"));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unreadable_appearance_falls_back_to_following_the_system() {
        let config: ConsoleConfig =
            serde_json::from_str(r#"{"appearance":"chartreuse"}"#).expect("parses");
        assert_eq!(config.appearance, ThemePreference::System);
        let absent: ConsoleConfig = serde_json::from_str("{}").expect("parses");
        assert_eq!(absent.appearance, ThemePreference::System);
    }

    #[test]
    fn bootstrapping_needs_both_a_profile_and_a_party() {
        let base = ConsoleConfig::default();
        assert!(!base.is_bootstrapped());

        let only_profile = ConsoleConfig {
            profile: Some("Default".into()),
            ..base.clone()
        };
        assert!(
            !only_profile.is_bootstrapped(),
            "no party means onboarding still owes a step"
        );

        let only_party = ConsoleConfig {
            party: Some("party:abc".into()),
            ..base.clone()
        };
        assert!(!only_party.is_bootstrapped());

        let both = ConsoleConfig {
            profile: Some("Default".into()),
            party: Some("party:abc".into()),
            ..base
        };
        assert!(both.is_bootstrapped());
    }

    #[test]
    fn an_empty_choice_does_not_count_as_bootstrapped() {
        let config = ConsoleConfig {
            profile: Some(String::new()),
            party: Some("party:abc".into()),
            ..ConsoleConfig::default()
        };
        assert!(!config.is_bootstrapped());
    }
}
