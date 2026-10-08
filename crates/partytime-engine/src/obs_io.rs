//! Import and export of OBS scene collections.
//!
//! OBS keeps a scene collection as one JSON document whose `sources` array holds
//! both inputs and scenes: a scene is an entry that itself carries a `sources` array
//! of `{ source_uuid }` references. We read and write that exact shape so a creator can
//! move between OBS and this console without inventing a format on either side.
//!
//! Things OBS does not store in the scene collection — the OpenParty publish kind of
//! each source, the mixing channels, the encoder settings, and the publish behaviour —
//! are written to a `<name>.openparty.json` sidecar beside it. Import without a sidecar
//! is lossless for scenes and inputs and derives sensible defaults for the rest.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::profile::{
    AudioChannel, InputKind, OutputSettings, Profile, PublishKind, PublishSettings, Scene,
    SceneItem, Source, Transform,
};

/// One entry in an OBS scene collection's `sources` array.
///
/// The same type models an input and a scene; [`ObsSource::is_scene`] tells them apart
/// by the presence of a nested `sources` array.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObsSource {
    /// libobs source-type id, e.g. `display_capture` or `scene`.
    pub id: String,
    /// libobs uuid. Stable across renames, which is what we key the UI on.
    #[serde(rename = "source_uuid", default)]
    pub source_uuid: String,
    /// Display name.
    pub name: String,
    /// Source-type specific settings. Retained verbatim on round-trip so settings we
    /// do not model are not silently dropped from a creator's config.
    #[serde(default)]
    pub settings: serde_json::Value,
    /// Scene items, when this entry is a scene.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<ObsSourceRef>,
    /// Per-scene visibility of the referencing item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    /// Whether the input contributes audio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    /// Linear gain in decibels.
    #[serde(rename = "volume_db", default, skip_serializing_if = "Option::is_none")]
    pub volume_db: Option<f32>,
    /// Capture-device clock correction.
    #[serde(
        rename = "sync_offset_ms",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub sync_offset_ms: Option<i32>,
}

/// A reference from a scene to one of its inputs, as OBS stores it.
///
/// The placement fields are read and written so a scene imported from OBS arrives with
/// its layers where its author put them, rather than stacked at the origin.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObsSourceRef {
    /// libobs uuid of the referenced input.
    #[serde(rename = "source_uuid")]
    pub source_uuid: String,
    /// Whether this item is visible in the scene that holds it.
    #[serde(default = "visible_default")]
    pub visible: bool,
    /// OBS's own on/off flag. Optional because older collections omit it; treating a
    /// missing flag as "off" would silently hide every imported layer.
    #[serde(rename = "sceneItemEnabled", default)]
    pub scene_item_enabled: Option<bool>,
    /// Locked items cannot be moved in OBS's editor.
    #[serde(default)]
    pub locked: bool,
    /// Native width of the referenced source, used to clamp a crop on import.
    #[serde(rename = "sourceWidth", default)]
    pub source_width: u32,
    /// Native height of the referenced source.
    #[serde(rename = "sourceHeight", default)]
    pub source_height: u32,
    /// Explicit item width, 0 meaning "track the source".
    #[serde(default)]
    /// Explicit item width in canvas pixels; 0 means the item tracks its source.
    pub width: f32,
    /// Explicit item height, 0 meaning "track the source".
    #[serde(default)]
    /// Explicit item height in canvas pixels; 0 means the item tracks its source.
    pub height: f32,
    /// OBS anchor bitmask.
    #[serde(default)]
    /// OBS anchor bitmask; 0 in a file means the default, centre.
    pub alignment: u32,
    /// Horizontal offset in canvas pixels.
    #[serde(default)]
    /// Horizontal offset in canvas pixels, relative to `alignment`.
    pub x: f32,
    /// Vertical offset in canvas pixels.
    #[serde(default)]
    /// Vertical offset in canvas pixels, relative to `alignment`.
    pub y: f32,
    /// Clockwise rotation in degrees.
    #[serde(default)]
    /// Clockwise rotation in degrees.
    pub rotation: f32,
    #[serde(rename = "scaleX", default)]
    /// Horizontal scale; 0 in a file means 1.0, not zero scale.
    pub scale_x: f32,
    #[serde(rename = "scaleY", default)]
    /// Vertical scale; 0 in a file means 1.0, not zero scale.
    pub scale_y: f32,
    #[serde(rename = "croppingEnabled", default)]
    /// Whether the crop insets below apply.
    pub cropping_enabled: bool,
    #[serde(rename = "cropLeft", default)]
    /// Left crop inset in canvas pixels.
    pub crop_left: u32,
    #[serde(rename = "cropRight", default)]
    /// Right crop inset in canvas pixels.
    pub crop_right: u32,
    #[serde(rename = "cropTop", default)]
    /// Top crop inset in canvas pixels.
    pub crop_top: u32,
    #[serde(rename = "cropBottom", default)]
    /// Bottom crop inset in canvas pixels.
    pub crop_bottom: u32,
}

const fn visible_default() -> bool {
    true
}

impl ObsSource {
    /// Whether this entry is a scene rather than an input.
    #[must_use]
    pub fn is_scene(&self) -> bool {
        !self.sources.is_empty() || self.id == "scene"
    }
}

/// A whole OBS scene collection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObsSceneCollection {
    /// Collection name.
    pub name: String,
    /// Inputs and scenes.
    pub sources: Vec<ObsSource>,
}

/// The sidecar holding everything the scene collection cannot express.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sidecar {
    /// Publish kind per source uuid.
    #[serde(default)]
    pub source_kind: std::collections::BTreeMap<String, PublishKind>,
    /// Mixing channels.
    #[serde(default)]
    pub audio_channels: Vec<AudioChannel>,
    /// Encoder settings.
    #[serde(default)]
    pub output: OutputSettings,
    /// OpenParty publish behaviour.
    #[serde(default)]
    pub publish: PublishSettings,
}

/// Maps a libobs source-type id onto the input kind this console understands.
///
/// Returns `None` for types the console cannot drive (browsers, NDI, 3DNET, …); those
/// are kept in the raw collection on export but are not shown as inputs.
#[must_use]
pub fn input_kind_for_source_id(id: &str) -> Option<InputKind> {
    match id {
        "display_capture" | "monitor_capture" | "wayland_source" | "xshm_input" => {
            Some(InputKind::Display)
        }
        "win-capture" | "window_capture" | "screencast-source" => Some(InputKind::Window),
        "game_capture" | "screen_capture" => Some(InputKind::Game),
        "dshow_input" | "avfoundation_input" | "v4l2_input" | "pipewire_input" => {
            Some(InputKind::Camera)
        }
        "wasapi_input_capture"
        | "pulse_input_capture"
        | "coreaudio_input_capture"
        | "pulse_input_output_capture" => Some(InputKind::Microphone),
        "media_source" | "image_source" | "ffmpeg_source" => Some(InputKind::Media),
        _ => None,
    }
}

/// Builds a [`Profile`] from an OBS scene collection and its sidecar.
///
/// Unknown source types stay in `raw` so they survive a re-export; they are simply not
/// offered as inputs.
#[must_use]
pub fn profile_from_obs(collection: &ObsSceneCollection, sidecar: Option<&Sidecar>) -> Profile {
    let mut sources = Vec::new();
    let mut scenes = Vec::new();

    for entry in &collection.sources {
        if entry.is_scene() {
            let items = entry
                .sources
                .iter()
                .map(|r| SceneItem {
                    source_uuid: r.source_uuid.clone(),
                    visible: r.visible && r.scene_item_enabled.unwrap_or(true),
                    transform: Transform {
                        position_x: r.x,
                        position_y: r.y,
                        // OBS writes 0 for an unset alignment; its default is centre.
                        alignment: if r.alignment == 0 { 5 } else { r.alignment },
                        // 0 means "track the source", not zero scale.
                        scale_x: if r.scale_x == 0.0 { 1.0 } else { r.scale_x },
                        scale_y: if r.scale_y == 0.0 { 1.0 } else { r.scale_y },
                        rotation: r.rotation,
                        width: (r.width > 0.0).then_some(r.width),
                        height: (r.height > 0.0).then_some(r.height),
                        crop_enabled: r.cropping_enabled,
                        crop_left: r.crop_left,
                        crop_right: r.crop_right,
                        crop_top: r.crop_top,
                        crop_bottom: r.crop_bottom,
                    },
                })
                .collect();
            scenes.push(Scene {
                name: entry.name.clone(),
                items,
            });
            continue;
        }
        let Some(input) = input_kind_for_source_id(&entry.id) else {
            continue;
        };
        let uuid = if entry.source_uuid.is_empty() {
            slugify(&entry.name)
        } else {
            entry.source_uuid.clone()
        };
        let kind = sidecar
            .and_then(|s| s.source_kind.get(&uuid).copied())
            .unwrap_or_else(|| input.default_publish_kind());
        let mut source = Source::new(uuid, entry.name.clone(), input);
        source.visible = entry.visible.unwrap_or(true);
        source.muted = entry.muted.unwrap_or(false);
        source.gain_db = entry.volume_db.unwrap_or(0.0);
        source.sync_offset_ms = entry.sync_offset_ms.unwrap_or(0);
        source.kind = kind;
        source.clamp_gain();
        sources.push(source);
    }

    let audio_channels = sidecar
        .map(|s| s.audio_channels.clone())
        .unwrap_or_else(|| derive_channels(&sources));
    let output = sidecar.map_or_else(OutputSettings::default, |s| s.output);
    let publish = sidecar.map_or_else(PublishSettings::default, |s| s.publish.clone());

    Profile {
        name: collection.name.clone(),
        output,
        publish,
        sources,
        scenes,
        audio_channels,
    }
}

/// Renders a [`Profile`] back into an OBS scene collection plus sidecar.
///
/// Entries whose input kind has no libobs equivalent are dropped from the scene
/// collection; everything else round-trips, including source-type settings the console
/// does not model.
#[must_use]
pub fn profile_to_obs(
    profile: &Profile,
    raw: &ObsSceneCollection,
) -> (ObsSceneCollection, Sidecar) {
    let mut out = Vec::new();

    for scene in &profile.scenes {
        let items = scene
            .items
            .iter()
            .filter_map(|item| {
                profile
                    .source(&item.source_uuid)
                    .map(|source| (source, item))
            })
            .map(|(source, item)| {
                let tf = &item.transform;
                ObsSourceRef {
                    source_uuid: source.uuid.clone(),
                    visible: item.visible,
                    scene_item_enabled: Some(item.visible),
                    locked: false,
                    source_width: 0,
                    source_height: 0,
                    width: tf.width.unwrap_or(0.0),
                    height: tf.height.unwrap_or(0.0),
                    alignment: tf.alignment,
                    x: tf.position_x,
                    y: tf.position_y,
                    rotation: tf.rotation,
                    scale_x: tf.scale_x,
                    scale_y: tf.scale_y,
                    cropping_enabled: tf.crop_enabled,
                    crop_left: tf.crop_left,
                    crop_right: tf.crop_right,
                    crop_top: tf.crop_top,
                    crop_bottom: tf.crop_bottom,
                }
            })
            .collect();
        out.push(ObsSource {
            id: "scene".into(),
            source_uuid: format!("scene-{}", slugify(&scene.name)),
            name: scene.name.clone(),
            settings: serde_json::Value::Null,
            sources: items,
            visible: None,
            muted: None,
            volume_db: None,
            sync_offset_ms: None,
        });
    }

    let mut source_kind = std::collections::BTreeMap::new();
    for source in &profile.sources {
        let preserved = raw
            .sources
            .iter()
            .find(|e| e.source_uuid == source.uuid && !e.is_scene());
        out.push(ObsSource {
            id: libobs_source_id(source.input).into(),
            source_uuid: source.uuid.clone(),
            name: source.name.clone(),
            settings: preserved
                .map(|e| e.settings.clone())
                .unwrap_or(serde_json::Value::Null),
            sources: Vec::new(),
            visible: Some(source.visible),
            muted: Some(source.muted),
            volume_db: Some(source.gain_db),
            sync_offset_ms: Some(source.sync_offset_ms),
        });
        source_kind.insert(source.uuid.clone(), source.kind);
    }

    let sidecar = Sidecar {
        source_kind,
        audio_channels: profile.audio_channels.clone(),
        output: profile.output,
        publish: profile.publish.clone(),
    };
    (
        ObsSceneCollection {
            name: profile.name.clone(),
            sources: out,
        },
        sidecar,
    )
}

/// The libobs source-type id for an input kind.
#[must_use]
pub const fn libobs_source_id(kind: InputKind) -> &'static str {
    match kind {
        InputKind::Display => "display_capture",
        InputKind::Window => "win-capture",
        InputKind::Game => "game_capture",
        InputKind::Camera => "dshow_input",
        InputKind::Microphone => "wasapi_input_capture",
        InputKind::Media => "media_source",
    }
}

/// Groups sources into channels when the collection has no sidecar.
///
/// Video lands in `Default`; audio lands in `Mic/Aux` for a microphone and
/// `Music/Aux` for anything else, which is where OBS's own defaults put them.
fn derive_channels(sources: &[Source]) -> Vec<AudioChannel> {
    let mut default = Vec::new();
    let mut mic = Vec::new();
    let mut music = Vec::new();
    for source in sources {
        if source.input.is_audio() {
            if source.kind == PublishKind::Mic {
                mic.push(source.uuid.clone());
            } else {
                music.push(source.uuid.clone());
            }
        } else {
            default.push(source.uuid.clone());
        }
    }
    let mut channels = vec![AudioChannel {
        name: "Default".into(),
        sources: default,
    }];
    if !mic.is_empty() {
        channels.push(AudioChannel {
            name: "Mic/Aux".into(),
            sources: mic,
        });
    }
    if !music.is_empty() {
        channels.push(AudioChannel {
            name: "Music/Aux".into(),
            sources: music,
        });
    }
    channels
}

/// A filesystem-stable identifier derived from a display name.
///
/// Used only when an imported entry has no libobs uuid, so that a fresh import still
/// has a stable identity across a save/load cycle.
fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_dash = true;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() { "source".into() } else { out }
}

/// Reads an OBS scene collection from `path`.
///
/// Returns the raw collection alongside the profile so an unchanged re-export can
/// carry source-type settings this console does not model.
pub fn import_obs_file(path: &Path) -> Result<(Profile, ObsSceneCollection), ObsIoError> {
    let text = std::fs::read_to_string(path).map_err(|source| ObsIoError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let collection: ObsSceneCollection =
        serde_json::from_str(&text).map_err(|source| ObsIoError::Parse {
            path: path.display().to_string(),
            source: Box::new(source),
        })?;
    let sidecar_path = sidecar_path_for(path);
    let sidecar = std::fs::read_to_string(&sidecar_path)
        .ok()
        .and_then(|text| serde_json::from_str::<Sidecar>(&text).ok());
    let profile = profile_from_obs(&collection, sidecar.as_ref());
    Ok((profile, collection))
}

/// Writes a profile to `path` as an OBS scene collection, plus its sidecar.
pub fn export_obs_file(
    profile: &Profile,
    raw: &ObsSceneCollection,
    path: &Path,
) -> Result<(), ObsIoError> {
    let (collection, sidecar) = profile_to_obs(profile, raw);
    let text =
        serde_json::to_string_pretty(&collection).map_err(|source| ObsIoError::Serialize {
            path: path.display().to_string(),
            source,
        })?;
    std::fs::write(path, text).map_err(|source| ObsIoError::Write {
        path: path.display().to_string(),
        source,
    })?;
    let sidecar_text =
        serde_json::to_string_pretty(&sidecar).map_err(|source| ObsIoError::Serialize {
            path: sidecar_path_for(path).display().to_string(),
            source,
        })?;
    std::fs::write(sidecar_path_for(path), sidecar_text).map_err(|source| ObsIoError::Write {
        path: sidecar_path_for(path).display().to_string(),
        source,
    })
}

/// The sidecar path that belongs to a scene-collection path.
#[must_use]
pub fn sidecar_path_for(collection_path: &Path) -> std::path::PathBuf {
    let mut name = collection_path
        .file_name()
        .unwrap_or_default()
        .to_os_string();
    name.push(".openparty.json");
    collection_path.with_file_name(name)
}

/// Failure while reading or writing an OBS collection.
#[derive(Debug, thiserror::Error)]
pub enum ObsIoError {
    /// The file could not be read.
    #[error("could not read {path}")]
    Read {
        /// Path that failed.
        path: String,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The file was not a scene collection.
    #[error("{path} is not an OBS scene collection: {source}")]
    Parse {
        /// Path that failed.
        path: String,
        /// Underlying error.
        #[source]
        source: Box<serde_json::Error>,
    },
    /// The document could not be serialised.
    #[error("could not serialise {path}")]
    Serialize {
        /// Path that failed.
        path: String,
        /// Underlying error.
        #[source]
        source: serde_json::Error,
    },
    /// The file could not be written.
    #[error("could not write {path}")]
    Write {
        /// Path that failed.
        path: String,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "name": "Friday Night",
      "sources": [
        { "id": "scene", "source_uuid": "s1", "name": "Party Scene",
          "sources": [
            { "source_uuid": "a1", "visible": true },
            { "source_uuid": "a2", "visible": false }
          ] },
        { "id": "game_capture", "source_uuid": "a1", "name": "Game Capture",
          "settings": { "capture_mode": "any" }, "muted": false, "volume_db": -3.0,
          "sync_offset_ms": 12 },
        { "id": "dshow_input", "source_uuid": "a2", "name": "Camera", "settings": {} },
        { "id": "wasapi_input_capture", "source_uuid": "a3", "name": "Mic", "settings": {} },
        { "id": "browser_source", "source_uuid": "a4", "name": "Ticker", "settings": {} }
      ]
    }"#;

    fn sample() -> ObsSceneCollection {
        serde_json::from_str(SAMPLE).expect("sample parses")
    }

    #[test]
    fn import_maps_known_types_and_skips_unknown_ones() {
        let profile = profile_from_obs(&sample(), None);
        let uuids: Vec<&str> = profile.sources.iter().map(|s| s.uuid.as_str()).collect();
        assert_eq!(
            uuids,
            vec!["a1", "a2", "a3"],
            "browser_source is not a console input"
        );
        assert_eq!(profile.scenes.len(), 1);
        assert_eq!(profile.scenes[0].items.len(), 2);
    }

    #[test]
    fn import_preserves_gain_offset_and_item_visibility() {
        let profile = profile_from_obs(&sample(), None);
        let game = profile.source("a1").expect("game capture imported");
        assert!((game.gain_db + 3.0).abs() < f32::EPSILON);
        assert_eq!(game.sync_offset_ms, 12);
        assert!(profile.scenes[0].items[0].visible);
        assert!(
            !profile.scenes[0].items[1].visible,
            "hidden item stays hidden"
        );
    }

    #[test]
    fn import_derives_publish_kinds_and_channels_without_a_sidecar() {
        let profile = profile_from_obs(&sample(), None);
        assert_eq!(
            profile.source("a1").expect("a1").kind,
            PublishKind::Gameplay
        );
        assert_eq!(profile.source("a2").expect("a2").kind, PublishKind::Camera);
        assert_eq!(profile.source("a3").expect("a3").kind, PublishKind::Mic);

        let names: Vec<&str> = profile
            .audio_channels
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, vec!["Default", "Mic/Aux"]);
        assert_eq!(profile.channel_sources("Mic/Aux")[0].uuid, "a3");
    }

    #[test]
    fn sidecar_overrides_derived_kinds() {
        let sidecar = Sidecar {
            source_kind: [("a1".to_string(), PublishKind::Camera)]
                .into_iter()
                .collect(),
            ..Sidecar::default()
        };
        let profile = profile_from_obs(&sample(), Some(&sidecar));
        assert_eq!(profile.source("a1").expect("a1").kind, PublishKind::Camera);
    }

    #[test]
    fn round_trip_preserves_scenes_sources_and_unmodelled_settings() {
        let raw = sample();
        let profile = profile_from_obs(&raw, None);
        let (out, sidecar) = profile_to_obs(&profile, &raw);

        assert_eq!(out.name, "Friday Night");
        let scene = out
            .sources
            .iter()
            .find(|s| s.is_scene())
            .expect("scene exported");
        assert_eq!(scene.name, "Party Scene");
        assert_eq!(scene.sources.len(), 2);

        let game = out
            .sources
            .iter()
            .find(|s| s.source_uuid == "a1")
            .expect("game exported");
        assert_eq!(game.id, "game_capture");
        assert_eq!(
            game.settings.get("capture_mode").and_then(|v| v.as_str()),
            Some("any"),
            "settings the console does not model must survive a re-export"
        );

        assert_eq!(sidecar.source_kind.get("a2"), Some(&PublishKind::Camera));

        // Re-importing the export reproduces the profile.
        let again = profile_from_obs(&out, Some(&sidecar));
        assert_eq!(again.scenes, profile.scenes);
        assert_eq!(again.sources.len(), profile.sources.len());
    }

    #[test]
    fn round_trip_preserves_a_declared_kind_that_is_not_the_input_default() {
        let raw = sample();
        let mut profile = profile_from_obs(&raw, None);
        profile.source_mut("a1").expect("a1").kind = PublishKind::PartyAudio;
        let (_, sidecar) = profile_to_obs(&profile, &raw);
        let again = profile_from_obs(&out_from(&profile, &raw), Some(&sidecar));
        assert_eq!(
            again.source("a1").expect("a1").kind,
            PublishKind::PartyAudio
        );
    }

    fn out_from(profile: &Profile, raw: &ObsSceneCollection) -> ObsSceneCollection {
        profile_to_obs(profile, raw).0
    }

    #[test]
    fn import_file_round_trips_through_disk_including_the_sidecar() {
        let dir = std::env::temp_dir().join(format!("pt-obs-io-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("scenes.json");
        let raw = sample();
        let mut profile = profile_from_obs(&raw, None);
        profile.source_mut("a2").expect("a2").kind = PublishKind::PartyAudio;

        export_obs_file(&profile, &raw, &path).expect("export");
        assert!(sidecar_path_for(&path).exists(), "sidecar must be written");

        let (loaded, _) = import_obs_file(&path).expect("import");
        assert_eq!(
            loaded.source("a2").expect("a2").kind,
            PublishKind::PartyAudio
        );
        assert_eq!(loaded.scenes, profile.scenes);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn importing_a_non_collection_reports_the_path() {
        let dir = std::env::temp_dir().join(format!("pt-obs-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("scenes.json");
        std::fs::write(&path, "{ not json").expect("write");
        let err = import_obs_file(&path).expect_err("must fail");
        assert!(matches!(err, ObsIoError::Parse { .. }));
        assert!(err.to_string().contains("scenes.json"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn entries_without_a_uuid_get_a_stable_slug() {
        let collection: ObsSceneCollection = serde_json::from_str(
            r#"{"name":"N","sources":[{"id":"game_capture","name":"Game Capture","settings":{}}]}"#,
        )
        .expect("parses");
        let profile = profile_from_obs(&collection, None);
        assert_eq!(profile.sources[0].uuid, "game-capture");
    }

    #[test]
    fn scene_item_transforms_survive_a_round_trip() {
        // The point of carrying a transform: an imported scene keeps its layout
        // instead of stacking every layer at the origin.
        let collection: ObsSceneCollection = serde_json::from_str(
            r#"{"name":"N","sources":[
                {"id":"scene","source_uuid":"s1","name":"Scene","sources":[
                  {"source_uuid":"a1","visible":true,"sceneItemEnabled":true,
                   "x":120.5,"y":-40.0,"alignment":1,"scaleX":0.5,"scaleY":0.25,
                   "rotation":15.0,"width":960.0,"height":540.0,
                   "croppingEnabled":true,"cropLeft":10,"cropRight":20,
                   "cropTop":5,"cropBottom":6}]},
                {"id":"game_capture","source_uuid":"a1","name":"Game","settings":{}}]}"#,
        )
        .expect("parses");

        let profile = profile_from_obs(&collection, None);
        let tf = profile.scene("Scene").expect("scene").items[0].transform;
        assert_eq!(tf.position_x, 120.5);
        assert_eq!(tf.position_y, -40.0);
        assert_eq!(tf.alignment, 1, "a non-default anchor is preserved");
        assert_eq!(tf.scale_x, 0.5);
        assert_eq!(tf.rotation, 15.0);
        assert_eq!(tf.width, Some(960.0));
        assert!(tf.crop_enabled);
        assert_eq!(tf.crop_left, 10);
        assert_eq!(tf.crop_bottom, 6);

        let (out, _) = profile_to_obs(&profile, &collection);
        let scene = out
            .sources
            .iter()
            .find(|s| s.is_scene())
            .expect("scene out");
        let back = &scene.sources[0];
        assert_eq!(back.x, 120.5);
        assert_eq!(back.alignment, 1);
        assert_eq!(back.scale_y, 0.25);
        assert_eq!(back.rotation, 15.0);
        assert_eq!(back.width, 960.0);
        assert_eq!(back.crop_top, 5);
    }

    #[test]
    fn an_unset_alignment_and_scale_read_as_obs_defaults_not_as_zero() {
        // OBS writes 0 for both when they are untouched; taking that literally would
        // scale every layer to nothing and anchor it top-left.
        let collection: ObsSceneCollection = serde_json::from_str(
            r#"{"name":"N","sources":[
                {"id":"scene","source_uuid":"s1","name":"Scene","sources":[
                  {"source_uuid":"a1","visible":true,"sceneItemEnabled":true,
                   "x":0,"y":0,"alignment":0,"scaleX":0,"scaleY":0}]},
                {"id":"game_capture","source_uuid":"a1","name":"Game","settings":{}}]}"#,
        )
        .expect("parses");
        let profile = profile_from_obs(&collection, None);
        let tf = profile.scene("Scene").expect("scene").items[0].transform;
        assert_eq!(tf.alignment, 5, "OBS's default anchor is centre");
        assert_eq!(tf.scale_x, 1.0);
        assert_eq!(tf.scale_y, 1.0);
        assert!(tf.is_default());
    }

    #[test]
    fn every_source_id_we_emit_maps_back_to_an_input_kind() {
        for kind in [
            InputKind::Display,
            InputKind::Window,
            InputKind::Game,
            InputKind::Camera,
            InputKind::Microphone,
            InputKind::Media,
        ] {
            assert_eq!(
                input_kind_for_source_id(libobs_source_id(kind)),
                Some(kind),
                "{} does not round-trip",
                libobs_source_id(kind)
            );
        }
    }
}
