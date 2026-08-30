//! Library facade shared by the daemons, configuration helper, TUI, and
//! platform-neutral tests so they reuse the same gesture/output stack.

#[cfg(target_os = "macos")]
pub mod app_context;
pub mod boot;
pub mod config;
pub mod descriptor;
pub mod gesture;
#[cfg(target_os = "macos")]
pub mod hid;
pub mod instance_lock;
pub mod macos_preferences;
pub mod net;
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
