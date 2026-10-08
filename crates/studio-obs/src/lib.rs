//! The unsafe boundary around the in-process OBS runtime.
//!
//! `studio-engine` remains pure profile data. This crate owns libobs's process-global
//! lifecycle so callers cannot accidentally start or shut down the engine from separate
//! code paths. The first entry point is an executable smoke: initialize audio and video,
//! compose one scene containing one nested scene source, then shut down without creating
//! an output.

#![deny(unsafe_op_in_unsafe_fn)]

use std::{
    ffi::{CStr, CString, NulError},
    path::PathBuf,
    ptr::{self, NonNull},
    sync::Mutex,
};

use thiserror::Error;

static OBS_LIFECYCLE: Mutex<()> = Mutex::new(());

/// Errors returned while starting libobs or exercising its scene graph.
#[derive(Debug, Error)]
pub enum ObsError {
    /// Another owner has already initialized libobs in this process.
    #[error("libobs is already initialized in this process")]
    AlreadyInitialized,
    /// The Rust process does not have permission to serialize OBS lifecycle calls.
    #[error("the libobs lifecycle mutex was poisoned")]
    LifecyclePoisoned,
    /// The locale or path contained an interior NUL byte.
    #[error("invalid C string: {0}")]
    CString(#[from] NulError),
    /// OBS refused to start.
    #[error("obs_startup returned false")]
    Startup,
    /// The installed OBS resource directory was not present.
    #[error("libobs resource directory is missing: {0}")]
    MissingDataDirectory(PathBuf),
    /// OBS could not initialize its audio backend.
    #[error("obs_reset_audio2 returned false")]
    AudioReset,
    /// OBS could not initialize its graphics backend; the value is OBS's status code.
    #[error("obs_reset_video returned status {0}")]
    VideoReset(i32),
    /// OBS failed to create one of the scene objects needed by the smoke.
    #[error("libobs could not create the smoke scene graph")]
    SceneCreation,
    /// Failed to create the dedicated OBS actor thread.
    #[error("could not spawn OBS actor thread: {0}")]
    ActorSpawn(#[source] std::io::Error),
    /// The OBS actor panicked before returning the smoke result.
    #[error("OBS actor thread panicked")]
    ActorPanicked,
    /// OBS returned a version string that was not valid UTF-8.
    #[error("libobs returned a non-UTF-8 version string")]
    InvalidVersion,
}

/// The observable result of a real libobs startup and scene-graph smoke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmokeReport {
    /// The version string returned by the linked libobs library.
    pub libobs_version: String,
    /// The number of sources attached to the smoke scene.
    pub scene_sources: usize,
    /// Whether the smoke deliberately avoided creating any OBS output.
    pub created_output: bool,
}

/// Starts the linked libobs on a dedicated actor thread, initializes audio and video, and
/// attaches one scene source.
///
/// The scene source is a nested scene, which exercises libobs's built-in scene source
/// implementation without requiring the optional OBS plugin catalog. This smoke creates
/// no output. A process-wide mutex serializes libobs's global lifecycle.
pub fn smoke() -> Result<SmokeReport, ObsError> {
    std::thread::Builder::new()
        .name("partytime-obs-actor".into())
        .spawn(smoke_on_actor)
        .map_err(ObsError::ActorSpawn)?
        .join()
        .map_err(|_| ObsError::ActorPanicked)?
}

fn smoke_on_actor() -> Result<SmokeReport, ObsError> {
    let _lifecycle = OBS_LIFECYCLE
        .lock()
        .map_err(|_| ObsError::LifecyclePoisoned)?;

    // SAFETY: access to process-global libobs lifecycle state is serialized by the mutex
    // above; this check happens before any initialization owned by this call.
    if unsafe { libobs::obs_initialized() } {
        return Err(ObsError::AlreadyInitialized);
    }

    let locale = CString::new("en-US")?;
    // SAFETY: `locale` is a live, NUL-terminated string. The two optional paths are null
    // as permitted by obs_startup; no other code initializes libobs under this mutex.
    if !unsafe { libobs::obs_startup(locale.as_ptr(), ptr::null(), ptr::null_mut()) } {
        return Err(ObsError::Startup);
    }
    let _shutdown = ObsShutdown;

    let version_ptr = unsafe { libobs::obs_get_version_string() };
    if version_ptr.is_null() {
        return Err(ObsError::InvalidVersion);
    }
    // SAFETY: libobs returns a static version string valid until shutdown; the pointer was
    // checked non-null and is copied before the shutdown guard runs.
    let version = unsafe { CStr::from_ptr(version_ptr) }
        .to_str()
        .map_err(|_| ObsError::InvalidVersion)?
        .to_owned();

    let prefix = obs_prefix();
    let data_dir = prefix.join("share/obs/libobs");
    if !data_dir.is_dir() {
        return Err(ObsError::MissingDataDirectory(data_dir));
    }
    let data_dir = CString::new(data_dir.to_string_lossy().as_bytes())?;
    // SAFETY: data_dir remains alive through the call; libobs copies the path.
    unsafe { libobs::obs_add_data_path(data_dir.as_ptr()) };

    #[cfg(target_os = "linux")]
    {
        let platform = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            libobs::obs_nix_platform_type_OBS_NIX_PLATFORM_WAYLAND
        } else {
            libobs::obs_nix_platform_type_OBS_NIX_PLATFORM_X11_EGL
        };
        // SAFETY: selects the current process's host display backend before video reset.
        unsafe { libobs::obs_set_nix_platform(platform) };
    }

    let audio = libobs::obs_audio_info2 {
        samples_per_sec: 48_000,
        speakers: libobs::speaker_layout_SPEAKERS_STEREO,
        max_buffering_ms: 0,
        fixed_buffering: false,
    };
    // SAFETY: `audio` is fully initialized and remains live for the duration of reset.
    if !unsafe { libobs::obs_reset_audio2(&audio) } {
        return Err(ObsError::AudioReset);
    }

    let graphics_module = CString::new("libobs-opengl")?;
    let mut video = libobs::obs_video_info {
        adapter: 0,
        graphics_module: graphics_module.as_ptr(),
        fps_num: 30,
        fps_den: 1,
        base_width: 1280,
        base_height: 720,
        output_width: 1280,
        output_height: 720,
        output_format: libobs::video_format_VIDEO_FORMAT_NV12,
        gpu_conversion: true,
        colorspace: libobs::video_colorspace_VIDEO_CS_DEFAULT,
        range: libobs::video_range_type_VIDEO_RANGE_DEFAULT,
        scale_type: libobs::obs_scale_type_OBS_SCALE_BILINEAR,
    };
    // SAFETY: `video` and graphics_module remain live for the call. The OpenGL backend
    // is installed in the same prefix as libobs and exposed through `LD_LIBRARY_PATH` at run.
    let video_status = unsafe { libobs::obs_reset_video(&mut video) };
    if video_status != 0 {
        return Err(ObsError::VideoReset(video_status));
    }

    let root_name = CString::new("PartyTime smoke scene")?;
    // SAFETY: libobs is initialized and video is active; the name is valid for the call.
    let root = Scene::new(unsafe { libobs::obs_scene_create(root_name.as_ptr()) })?;

    let source_name = CString::new("PartyTime smoke source scene")?;
    // SAFETY: same as above; a nested scene is a built-in libobs source and needs no
    // optional plugin module.
    let source_scene = Scene::new(unsafe { libobs::obs_scene_create(source_name.as_ptr()) })?;
    // SAFETY: source_scene is a live scene returned by obs_scene_create; this returns an
    // owned source reference that is released by Source::drop.
    let source = Source::new(unsafe { libobs::obs_scene_get_source(source_scene.as_ptr()) })?;
    // SAFETY: both pointers are live; obs_scene_add retains the source for the item.
    let item = unsafe { libobs::obs_scene_add(root.as_ptr(), source.as_ptr()) };
    if item.is_null() {
        return Err(ObsError::SceneCreation);
    }

    // Source and scene guards release the local references; root's scene item owns the
    // remaining reference until root is released. No output is created anywhere here.
    drop(source);
    drop(source_scene);
    drop(root);
    drop(_shutdown);

    Ok(SmokeReport {
        libobs_version: version,
        scene_sources: 1,
        created_output: false,
    })
}

fn obs_prefix() -> PathBuf {
    if let Some(prefix) = std::env::var_os("PARTYTIME_OBS_PREFIX") {
        return PathBuf::from(prefix);
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cache/partytime-obs")
}

struct ObsShutdown;

impl Drop for ObsShutdown {
    fn drop(&mut self) {
        // SAFETY: this guard is created only after successful obs_startup and dropped
        // after scene/source guards; the lifecycle mutex is still held by smoke().
        unsafe { libobs::obs_shutdown() };
    }
}

struct Scene(NonNull<libobs::obs_scene_t>);

impl Scene {
    fn new(pointer: *mut libobs::obs_scene_t) -> Result<Self, ObsError> {
        NonNull::new(pointer)
            .map(Self)
            .ok_or(ObsError::SceneCreation)
    }

    fn as_ptr(&self) -> *mut libobs::obs_scene_t {
        self.0.as_ptr()
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns the scene reference returned by
        // obs_scene_create; it is released exactly once before libobs shutdown.
        unsafe { libobs::obs_scene_release(self.as_ptr()) };
    }
}

struct Source(NonNull<libobs::obs_source_t>);

impl Source {
    fn new(pointer: *mut libobs::obs_source_t) -> Result<Self, ObsError> {
        NonNull::new(pointer)
            .map(Self)
            .ok_or(ObsError::SceneCreation)
    }

    fn as_ptr(&self) -> *mut libobs::obs_source_t {
        self.0.as_ptr()
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        // SAFETY: obs_scene_get_source returns an owned reference, released exactly once
        // after obs_scene_add has retained its own reference.
        unsafe { libobs::obs_source_release(self.as_ptr()) };
    }
}
