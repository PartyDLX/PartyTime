//! On-disk profile storage for the console.
//!
//! Profiles live under the app config directory in the console's own JSON form, which
//! is the [`Profile`] struct verbatim. OBS scene collections import into this form and
//! export back out; see [`crate::obs_io`].

use std::path::{Path, PathBuf};

use crate::{Profile, obs_io};

/// A name plus enough metadata to render a profile picker without loading every file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSummary {
    /// Profile name, also the file stem.
    pub name: String,
    /// Number of scenes in the profile.
    pub scenes: usize,
    /// Number of inputs in the profile.
    pub sources: usize,
}

/// Reads and writes profiles in a directory.
///
/// A missing directory is not an error: it is an empty store, which is exactly the
/// state a first launch is in.
#[derive(Debug, Clone)]
pub struct ProfileStore {
    root: PathBuf,
}

impl ProfileStore {
    /// Opens (or will create) a store rooted at `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The directory this store reads and writes.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether the store holds at least one profile.
    ///
    /// This is the "already bootstrapped" test the splash screen routes on.
    #[must_use]
    pub fn is_bootstrapped(&self) -> bool {
        self.list().is_ok_and(|profiles| !profiles.is_empty())
    }

    /// Every profile in the store, ordered by name.
    pub fn list(&self) -> Result<Vec<ProfileSummary>, StoreError> {
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(StoreError::Read { source: err }),
        };

        let mut summaries = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|err| StoreError::Read { source: err })?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if path.file_stem().and_then(|s| s.to_str()).is_none() {
                continue;
            }
            let Ok(profile) = read_profile(&path) else {
                continue;
            };
            summaries.push(ProfileSummary {
                name: profile.name,
                scenes: profile.scenes.len(),
                sources: profile.sources.len(),
            });
        }
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(summaries)
    }

    /// Loads a profile by name.
    pub fn load(&self, name: &str) -> Result<Profile, StoreError> {
        read_profile(&self.path_for(name))
    }

    /// Writes a profile, creating the store directory if needed.
    pub fn save(&mut self, profile: &Profile) -> Result<(), StoreError> {
        std::fs::create_dir_all(&self.root).map_err(|source| StoreError::Write { source })?;
        let text =
            serde_json::to_string_pretty(profile).map_err(|source| StoreError::Serialize {
                source: Box::new(source),
            })?;
        std::fs::write(self.path_for(&profile.name), text)
            .map_err(|source| StoreError::Write { source })
    }

    /// Copies a profile in from an OBS scene collection on disk.
    ///
    /// Returns the imported profile, already saved into the store.
    pub fn import_from_obs(&mut self, collection_path: &Path) -> Result<Profile, StoreError> {
        let (mut profile, raw) =
            obs_io::import_obs_file(collection_path).map_err(|err| StoreError::ObsImport {
                message: err.to_string(),
            })?;

        // OBS scene collections are named "Untitled"; the console names after the file.
        let stem = collection_path
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(&profile.name);
        profile.name = stem.to_string();

        // Keep the raw collection beside our copy so a later export preserves
        // source-type settings this console does not model.
        let raw_path = self.path_for(&profile.name).with_extension("obs.json");
        let text = serde_json::to_string_pretty(&raw).map_err(|source| StoreError::Serialize {
            source: Box::new(source),
        })?;
        std::fs::create_dir_all(&self.root).map_err(|source| StoreError::Write { source })?;
        std::fs::write(&raw_path, text).map_err(|source| StoreError::Write { source })?;
        self.save(&profile)?;
        Ok(profile)
    }

    /// Writes a profile back out as an OBS scene collection at `destination`.
    pub fn export_to_obs(&self, profile: &Profile, destination: &Path) -> Result<(), StoreError> {
        let raw_path = self.path_for(&profile.name).with_extension("obs.json");
        let raw = std::fs::read_to_string(&raw_path)
            .ok()
            .and_then(|text| serde_json::from_str::<obs_io::ObsSceneCollection>(&text).ok())
            .unwrap_or_default();
        obs_io::export_obs_file(profile, &raw, destination).map_err(|err| StoreError::ObsExport {
            message: err.to_string(),
        })
    }

    /// Deletes a profile and its preserved raw collection.
    pub fn remove(&self, name: &str) -> Result<(), StoreError> {
        std::fs::remove_file(self.path_for(name)).map_err(|source| StoreError::Write { source })?;
        let raw = self.path_for(name).with_extension("obs.json");
        if raw.exists() {
            std::fs::remove_file(raw).map_err(|source| StoreError::Write { source })?;
        }
        Ok(())
    }

    fn path_for(&self, name: &str) -> PathBuf {
        self.root.join(format!("{name}.json"))
    }
}

fn read_profile(path: &Path) -> Result<Profile, StoreError> {
    let text = std::fs::read_to_string(path).map_err(|source| StoreError::Read { source })?;
    serde_json::from_str(&text).map_err(|source| StoreError::Parse {
        path: path.display().to_string(),
        source: Box::new(source),
    })
}

/// Failure while reading or writing a profile.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The profile directory or file could not be read.
    #[error("could not read the profile store")]
    Read {
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// A profile could not be written.
    #[error("could not write the profile store")]
    Write {
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// A profile file was not a profile.
    #[error("{path} is not a profile")]
    Parse {
        /// Path that failed.
        path: String,
        /// Underlying error.
        #[source]
        source: Box<serde_json::Error>,
    },
    /// A profile could not be serialised.
    #[error("could not serialise a profile")]
    Serialize {
        /// Underlying error.
        #[source]
        source: Box<serde_json::Error>,
    },
    /// An OBS scene collection could not be imported.
    #[error("{message}")]
    ObsImport {
        /// Message from the importer.
        message: String,
    },
    /// A profile could not be exported as an OBS scene collection.
    #[error("{message}")]
    ObsExport {
        /// Message from the exporter.
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(tag: &str) -> (ProfileStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("pt-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        (ProfileStore::new(&dir), dir)
    }

    #[test]
    fn an_absent_store_is_empty_not_broken() {
        let (store, dir) = temp_store("empty");
        assert_eq!(store.list().expect("list"), vec![]);
        assert!(!store.is_bootstrapped());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_load_round_trips_the_profile() {
        let (mut store, dir) = temp_store("roundtrip");
        let profile = Profile::starter("Friday Night");
        store.save(&profile).expect("save");
        assert!(store.is_bootstrapped());

        let loaded = store.load("Friday Night").expect("load");
        assert_eq!(loaded, profile);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_reports_counts_and_sorts_by_name() {
        let (mut store, dir) = temp_store("list");
        store.save(&Profile::starter("Zulu")).expect("save");
        store.save(&Profile::starter("Alpha")).expect("save");

        let summaries = store.list().expect("list");
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].name, "Alpha");
        assert_eq!(summaries[0].scenes, 1);
        assert_eq!(summaries[0].sources, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_profile_is_skipped_rather_than_poisoning_the_list() {
        let (mut store, dir) = temp_store("corrupt");
        store.save(&Profile::starter("Good")).expect("save");
        std::fs::write(store.root().join("Broken.json"), "{ nope").expect("write");

        let summaries = store.list().expect("list");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].name, "Good");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn importing_an_obs_collection_names_the_profile_after_the_file() {
        let (mut store, dir) = temp_store("import");
        std::fs::create_dir_all(&dir).expect("dir");
        let src = dir.join("Friday Night.json");
        std::fs::write(
            &src,
            r#"{"name":"Untitled","sources":[
                {"id":"scene","source_uuid":"s1","name":"Party Scene",
                 "sources":[{"source_uuid":"a1","visible":true}]},
                {"id":"game_capture","source_uuid":"a1","name":"Game Capture","settings":{}}]}"#,
        )
        .expect("write");

        let imported = store.import_from_obs(&src).expect("import");
        assert_eq!(imported.name, "Friday Night");
        assert_eq!(store.list().expect("list")[0].name, "Friday Night");

        // The raw collection is kept so a later export can preserve settings.
        assert!(store.root().join("Friday Night.obs.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn importing_a_file_that_is_not_a_collection_reports_the_reason() {
        let (mut store, dir) = temp_store("badimport");
        std::fs::create_dir_all(&dir).expect("dir");
        let src = dir.join("bad.json");
        std::fs::write(&src, "not json").expect("write");

        let err = store.import_from_obs(&src).expect_err("must fail");
        assert!(matches!(err, StoreError::ObsImport { .. }));
        assert!(err.to_string().contains("bad.json"), "{}", err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_after_import_preserves_unmodelled_source_settings() {
        let (mut store, dir) = temp_store("export");
        std::fs::create_dir_all(&dir).expect("dir");
        let src = dir.join("Coll.json");
        std::fs::write(
            &src,
            r#"{"name":"Untitled","sources":[
                {"id":"game_capture","source_uuid":"a1","name":"Game Capture",
                 "settings":{"capture_mode":"any"}}]}"#,
        )
        .expect("write");

        let imported = store.import_from_obs(&src).expect("import");
        let dest = dir.join("out.json");
        store.export_to_obs(&imported, &dest).expect("export");

        let text = std::fs::read_to_string(&dest).expect("read");
        assert!(
            text.contains("capture_mode"),
            "unmodelled settings must survive"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_deletes_the_profile_and_its_raw_collection() {
        let (mut store, dir) = temp_store("remove");
        std::fs::create_dir_all(&dir).expect("dir");
        let src = dir.join("Gone.json");
        std::fs::write(&src, r#"{"name":"Untitled","sources":[]}"#).expect("write");
        store.import_from_obs(&src).expect("import");

        store.remove("Gone").expect("remove");
        assert!(!store.is_bootstrapped());
        assert!(!store.root().join("Gone.obs.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
