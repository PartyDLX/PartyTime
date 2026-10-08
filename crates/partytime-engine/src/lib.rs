//! Profile model, OBS configuration import/export, and the media-engine seam.
//!
//! The console owns editing; this crate owns the configuration and what the engine is.
//! Everything here is pure data and file I/O so the console can be built and tested
//! without libobs present.

#![forbid(unsafe_code)]

pub mod obs_io;
pub mod profile;
pub mod status;
pub mod store;

pub use obs_io::{ObsIoError, ObsSceneCollection, Sidecar};
pub use profile::{
    AspectRatio, AudioChannel, InputKind, OutputSettings, Profile, PublishKind, PublishSettings,
    Scene, SceneItem, Source, Transform,
};
pub use status::EngineStatus;
pub use store::ProfileStore;
