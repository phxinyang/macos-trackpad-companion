//! Library facade so binaries beyond the main `companion` daemon can
//! reuse the gesture/output stack. Shared between `src/main.rs` (the
//! daemon) and `src/bin/scroll_replay.rs` (the captured-stream
//! playback tool).

#[cfg(target_os = "macos")]
pub mod app_context;
pub mod boot;
pub mod config;
pub mod descriptor;
pub mod gesture;
#[cfg(target_os = "macos")]
pub mod hid;
pub mod instance_lock;
pub mod net;
pub mod macos_preferences;
#[cfg(target_os = "macos")]
pub mod output;
#[cfg(not(target_os = "macos"))]
#[path = "output_portable.rs"]
pub mod output;
#[cfg(target_os = "macos")]
pub mod overlay;
pub mod report;
pub mod scan_clock;
pub(crate) mod scroll_policy;
pub mod time;
