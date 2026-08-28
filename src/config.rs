//! TOML configuration. Loaded once at startup; the only CLI knobs are
//! `--config PATH` (override file location) and `-v` (override
//! `[log].level`). Missing file → all defaults.
//!
//! Default location: `$XDG_CONFIG_HOME/macos-trackpad-companion/config.toml`,
//! falling back to `$HOME/.config/macos-trackpad-companion/config.toml`.
//!
//! See `README.md` for full syntax. Quick reference:
//!
//! ```toml
//! [device]                # optional; omit for any PTP digitizer
//! # vid = 0x1234
//! # pid = 0x5678
//!
//! [net]                   # companion-net network input transport
//! # listen_ip = "0.0.0.0" # UDP+TCP bind address
//! # port      = 4242      # UDP frames arrive here; the touchpad web
//!                         # page is served over TCP on the same port
//! # token     = "..."     # optional bearer token for network clients
//!
//! [log]
//! level = "info"
//! # file  = "~/Library/Logs/macos-trackpad-companion.log"
//!
//! [macos]
//! sync_system_settings = true  # use System Settings when a field is not explicit below
//! haptic_feedback = "auto"     # auto | on | off
//!
//! [cursor]
//! sensitivity   = 28.0
//! accel_exponent = 1.35
//! accel_ref     = 70.0
//!
//! [scroll]
//! sensitivity = 20.0
//! natural     = true
//! enable      = true
//! horizontal  = true
//! momentum    = true
//! # modifier_zoom_mask = 262144  # optional Quartz modifier mask
//!
//! [gestures.pinch]                 # enable = "on" | "off" |
//! enable = "on"                    #   { only = ["bundle.id", ..] } |
//! [gestures.rotate]                #   { except = ["bundle.id", ..] }
//! enable = "on"
//! [gestures.swipe.horizontal]
//! enable  = "on"
//! backend = "synthetic"            # synthetic | notification | off
//! [gestures.swipe.vertical]
//! enable  = "on"
//! backend = "synthetic"
//! [gestures.press_and_hold_drag]
//! enable = "off"                  # optional; stock macOS default is off
//!
//! [overlay]                        # debug HUD; off by default
//! enable      = false
//! duration_ms = 600
//! ```

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub device: Device,
    pub net: Net,
    pub log: Log,
    pub cursor: Cursor,
    pub scroll: Scroll,
    pub gestures: Gestures,
    pub overlay: Overlay,
    pub macos: Macos,
    /// Paths explicitly present in the TOML source. This is provenance
    /// metadata used when macOS settings are merged after parsing.
    #[serde(skip)]
    explicit: ExplicitConfig,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct Macos {
    /// Read the user's macOS trackpad preferences at startup and use them as
    /// defaults for companion behavior. Explicit TOML fields still win.
    pub sync_system_settings: bool,
    /// Whether companion emits semantic haptic confirmations. `auto` follows
    /// the system's `ActuateDetents` preference when available.
    pub haptic_feedback: HapticSetting,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HapticSetting {
    Auto,
    On,
    Off,
}

impl Default for Macos {
    fn default() -> Self {
        Self {
            sync_system_settings: true,
            haptic_feedback: HapticSetting::Auto,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct ExplicitConfig {
    paths: BTreeSet<String>,
}

impl Config {
    /// Parse TOML while retaining which leaf fields were explicitly written.
    /// The normal `toml::from_str::<Config>` path remains supported for tests
    /// and callers that do not need system-preference merging.
    pub fn parse_str(source: &str) -> Result<Self> {
        let mut cfg: Self = toml::from_str(source).context("parse config")?;
        let value: toml::Value = toml::from_str(source).context("parse config metadata")?;
        collect_explicit_paths(&value, "", &mut cfg.explicit.paths);
        Ok(cfg)
    }

    pub(crate) fn has_explicit(&self, path: &str) -> bool {
        self.explicit.paths.contains(path)
    }
}

fn collect_explicit_paths(value: &toml::Value, prefix: &str, paths: &mut BTreeSet<String>) {
    let toml::Value::Table(table) = value else {
        return;
    };
    for (key, child) in table {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        if child.is_table() {
            collect_explicit_paths(child, &path, paths);
        } else {
            paths.insert(path);
        }
    }
}

/// `[net]` — companion-net's transport bindings. HID-device ingestion
/// ([`Device`]) and network ingestion are exclusive by instance lock.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct Net {
    /// Bind address for both the UDP frame port and the TCP listener
    /// that serves the touchpad web page + WebSocket endpoint.
    pub listen_ip: Option<String>,
    pub port: u16,
    /// Optional bearer token. When set, WebSocket clients must provide
    /// `Authorization: Bearer <token>` or `?token=<token>` and UDP clients
    /// must wrap ATP1 in the documented ATK1 envelope.
    pub token: Option<String>,
}

impl Default for Net {
    fn default() -> Self {
        Self {
            listen_ip: None,
            port: 4242,
            token: None,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct Overlay {
    pub enable: bool,
    pub duration_ms: u32,
}

impl Default for Overlay {
    fn default() -> Self {
        Self {
            enable: false,
            duration_ms: 600,
        }
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct Device {
    pub vid: Option<u16>,
    pub pid: Option<u16>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct Log {
    pub level: String,
    /// If set, logs are appended to this path instead of stderr. A
    /// leading `~/` is expanded against `$HOME`. Parent directories
    /// are created on demand.
    pub file: Option<PathBuf>,
}

impl Default for Log {
    fn default() -> Self {
        Self {
            level: "info".into(),
            file: None,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct Cursor {
    pub sensitivity: f64,
    pub accel_exponent: f64,
    pub accel_ref: f64,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            sensitivity: 28.0,
            accel_exponent: 1.35,
            accel_ref: 70.0,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct Scroll {
    pub sensitivity: f64,
    pub natural: bool,
    /// Whether two-finger translation emits scroll events.
    pub enable: bool,
    /// Whether horizontal scroll deltas are preserved.
    pub horizontal: bool,
    /// Whether a lift seeds a momentum-phase scroll coast.
    pub momentum: bool,
    /// Optional Quartz modifier mask for scroll-to-zoom routing. `None`
    /// keeps the built-in Command/Control compatibility mask.
    pub modifier_zoom_mask: Option<u64>,
}

impl Default for Scroll {
    fn default() -> Self {
        Self {
            sensitivity: 20.0,
            natural: true,
            enable: true,
            horizontal: true,
            momentum: true,
            modifier_zoom_mask: None,
        }
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct Gestures {
    /// Tap-to-click (the Point & Click "Tap to click" setting).
    pub tap_to_click: GestureEnable,
    /// Two-finger secondary click.
    pub secondary_click: GestureEnable,
    /// Two-finger smart zoom.
    pub smart_zoom: GestureEnable,
    /// Three-finger dictionary lookup.
    pub dictionary_lookup: GestureEnable,
    /// Two-finger right-edge swipe to Notification Center.
    pub right_edge_swipe: GestureEnable,
    pub pinch: Pinch,
    pub rotate: Rotate,
    pub swipe: Swipe,
    pub three_finger_drag: ThreeFingerDrag,
    pub one_finger_tap_drag: OneFingerTapDrag,
    pub press_and_hold_drag: PressAndHoldDrag,
}

/// `[gestures.one_finger_tap_drag]` — companion-net's 拖移样式 = 单指双击拖移.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct OneFingerTapDrag {
    pub enable: GestureEnable,
}

impl Default for OneFingerTapDrag {
    fn default() -> Self {
        Self {
            enable: GestureEnable::On,
        }
    }
}

/// `[gestures.press_and_hold_drag]` — optional accessibility-style drag.
/// This is deliberately off by default: ordinary macOS trackpad settings do
/// not turn a stationary single finger into a held mouse button.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct PressAndHoldDrag {
    pub enable: GestureEnable,
}

impl Default for PressAndHoldDrag {
    fn default() -> Self {
        Self {
            enable: GestureEnable::Off,
        }
    }
}

/// `[gestures.three_finger_drag]` — companion-net's 拖移样式 = 三指拖移.
/// While on, three-finger motion drags (left-button held) instead of firing
/// Dock swipes; four fingers keep the full swipe surface. Both HID and
/// network binaries use the same resolved options via `boot::gesture_options`.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct ThreeFingerDrag {
    pub enable: GestureEnable,
    pub release_delay_ms: u64,
}

impl Default for ThreeFingerDrag {
    fn default() -> Self {
        Self {
            enable: GestureEnable::On,
            // Default 500ms drag-lock delay allows lifting fingers and re-gripping (换把悬停)
            // to continue dragging across large screens. Set to 0 for instant lift-to-release.
            release_delay_ms: 500,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct Pinch {
    pub enable: GestureEnable,
}

impl Default for Pinch {
    fn default() -> Self {
        Self {
            enable: GestureEnable::On,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct Rotate {
    pub enable: GestureEnable,
}

impl Default for Rotate {
    fn default() -> Self {
        Self {
            enable: GestureEnable::On,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct Swipe {
    pub horizontal: SwipeAxisCfg,
    pub vertical: SwipeAxisCfg,
}

impl Default for Swipe {
    fn default() -> Self {
        Self {
            horizontal: SwipeAxisCfg {
                enable: GestureEnable::On,
                backend: SwipeBackend::Synthetic,
            },
            vertical: SwipeAxisCfg {
                enable: GestureEnable::On,
                backend: SwipeBackend::Notification,
            },
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct SwipeAxisCfg {
    pub enable: GestureEnable,
    pub backend: SwipeBackend,
}

impl Default for SwipeAxisCfg {
    fn default() -> Self {
        Self {
            enable: GestureEnable::On,
            backend: SwipeBackend::Synthetic,
        }
    }
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SwipeBackend {
    Synthetic,
    Notification,
    Off,
}

/// Per-gesture enable policy. Polymorphic in TOML so the common case
/// stays terse and the under-cursor filter lives in one key:
///
/// ```toml
/// enable = "on"                                # always
/// enable = "off"                               # never
/// enable = { only   = ["com.apple.Safari"] }   # allowlist by under-cursor app
/// enable = { except = ["com.apple.Terminal"] } # denylist by under-cursor app
/// ```
///
/// Matched against the bundle ID of the application owning the topmost
/// normal window under the cursor at gesture start; that decision is
/// held for the duration of the touch so a mid-gesture window switch
/// can't kill its own gesture. Mirrors how macOS itself dispatches
/// pinch/rotate/scroll/click — to the window under the cursor, not
/// strictly the frontmost app.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GestureEnable {
    #[default]
    On,
    Off,
    Only(Vec<String>),
    Except(Vec<String>),
}

impl<'de> Deserialize<'de> for GestureEnable {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct EnableTable {
            #[serde(default)]
            only: Option<Vec<String>>,
            #[serde(default)]
            except: Option<Vec<String>>,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Str(String),
            Table(EnableTable),
        }
        match Repr::deserialize(de)? {
            Repr::Str(s) => match s.as_str() {
                "on" => Ok(Self::On),
                "off" => Ok(Self::Off),
                other => Err(serde::de::Error::custom(format!(
                    "expected \"on\" or \"off\", got \"{other}\""
                ))),
            },
            Repr::Table(EnableTable { only, except }) => match (only, except) {
                (Some(only), None) => Ok(Self::Only(only)),
                (None, Some(except)) => Ok(Self::Except(except)),
                (Some(_), Some(_)) => Err(serde::de::Error::custom(
                    "`only` and `except` are mutually exclusive",
                )),
                (None, None) => Err(serde::de::Error::custom(
                    "expected `only` or `except` in enable table",
                )),
            },
        }
    }
}

/// Expand a leading `~/` (or bare `~`) against `$HOME`. Other path
/// forms pass through untouched.
pub fn expand_tilde(p: &Path) -> PathBuf {
    let s = match p.to_str() {
        Some(s) => s,
        None => return p.to_path_buf(),
    };
    let home = match std::env::var_os("HOME") {
        Some(h) if !h.is_empty() => PathBuf::from(h),
        _ => return p.to_path_buf(),
    };
    if s == "~" {
        home
    } else if let Some(rest) = s.strip_prefix("~/") {
        home.join(rest)
    } else {
        p.to_path_buf()
    }
}

/// Resolve `$XDG_CONFIG_HOME/macos-trackpad-companion/config.toml`,
/// falling back to `$HOME/.config/...` when XDG_CONFIG_HOME is unset
/// (the common case on macOS).
pub fn default_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            PathBuf::from(home).join(".config")
        });
    base.join("macos-trackpad-companion").join("config.toml")
}

/// Load `path` as TOML, or `default_path()` if `path` is `None`. A
/// missing file resolves to defaults — running with no config at all
/// is a supported mode. Parse errors are returned with file context.
pub fn load(path: Option<&Path>) -> Result<(Config, PathBuf)> {
    let resolved = path.map(PathBuf::from).unwrap_or_else(default_path);
    if !resolved.exists() {
        return Ok((Config::default(), resolved));
    }
    let s = std::fs::read_to_string(&resolved)
        .with_context(|| format!("read config {}", resolved.display()))?;
    let cfg = Config::parse_str(&s)
        .with_context(|| format!("parse config {}", resolved.display()))?;
    Ok((cfg, resolved))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_defaults() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.cursor.sensitivity, 28.0);
        assert_eq!(cfg.cursor.accel_exponent, 1.35);
        assert_eq!(cfg.cursor.accel_ref, 70.0);
        assert_eq!(cfg.scroll.sensitivity, 20.0);
        assert!(cfg.scroll.natural);
        assert!(cfg.scroll.enable);
        assert!(cfg.scroll.horizontal);
        assert_eq!(cfg.gestures.tap_to_click, GestureEnable::On);
        assert_eq!(cfg.gestures.secondary_click, GestureEnable::On);
        assert_eq!(cfg.gestures.smart_zoom, GestureEnable::On);
        assert_eq!(cfg.gestures.dictionary_lookup, GestureEnable::On);
        assert_eq!(cfg.gestures.right_edge_swipe, GestureEnable::On);
        assert_eq!(cfg.gestures.pinch.enable, GestureEnable::On);
        assert_eq!(
            cfg.gestures.swipe.horizontal.backend,
            SwipeBackend::Synthetic
        );
        assert_eq!(cfg.gestures.press_and_hold_drag.enable, GestureEnable::Off);
        assert_eq!(cfg.macos.haptic_feedback, HapticSetting::Auto);
    }

    #[test]
    fn enable_string_forms() {
        let cfg: Config = toml::from_str(
            r#"
            [gestures.pinch]
            enable = "off"
            [gestures.rotate]
            enable = "on"
        "#,
        )
        .unwrap();
        assert_eq!(cfg.gestures.pinch.enable, GestureEnable::Off);
        assert_eq!(cfg.gestures.rotate.enable, GestureEnable::On);
    }

    #[test]
    fn enable_only_table() {
        let cfg: Config = toml::from_str(
            r#"
            [gestures.pinch]
            enable = { only = ["com.apple.Safari", "com.apple.Photos"] }
        "#,
        )
        .unwrap();
        assert_eq!(
            cfg.gestures.pinch.enable,
            GestureEnable::Only(vec!["com.apple.Safari".into(), "com.apple.Photos".into()])
        );
    }

    #[test]
    fn enable_except_table() {
        let cfg: Config = toml::from_str(
            r#"
            [gestures.rotate]
            enable = { except = ["com.apple.Terminal"] }
        "#,
        )
        .unwrap();
        assert_eq!(
            cfg.gestures.rotate.enable,
            GestureEnable::Except(vec!["com.apple.Terminal".into()])
        );
    }

    #[test]
    fn press_and_hold_drag_is_opt_in() {
        let cfg: Config = toml::from_str(
            r#"
            [gestures.press_and_hold_drag]
            enable = "on"
        "#,
        )
        .unwrap();
        assert_eq!(cfg.gestures.press_and_hold_drag.enable, GestureEnable::On);
    }

    #[test]
    fn enable_only_and_except_is_error() {
        let err = toml::from_str::<Config>(
            r#"
            [gestures.pinch]
            enable = { only = ["a"], except = ["b"] }
        "#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("mutually exclusive"), "got: {err}");
    }

    #[test]
    fn enable_unknown_string_is_error() {
        let err = toml::from_str::<Config>(
            r#"
            [gestures.pinch]
            enable = "maybe"
        "#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("expected \"on\" or \"off\""), "got: {err}");
    }

    #[test]
    fn unknown_top_level_key_rejected() {
        let err = toml::from_str::<Config>(
            r#"
            [misnamed]
            sensitivity = 25.0
        "#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("unknown field"), "got: {err}");
    }

    #[test]
    fn swipe_backend_parses() {
        let cfg: Config = toml::from_str(
            r#"
            [gestures.swipe.vertical]
            backend = "notification"
        "#,
        )
        .unwrap();
        assert_eq!(
            cfg.gestures.swipe.vertical.backend,
            SwipeBackend::Notification
        );
    }

    #[test]
    fn device_hex_literals() {
        let cfg = toml::from_str::<Config>(
            r#"
            [device]
            vid = 0x1234
            pid = 0x5678
        "#,
        )
        .unwrap();
        assert_eq!(cfg.device.vid, Some(0x1234));
        assert_eq!(cfg.device.pid, Some(0x5678));
    }

    #[test]
    fn net_defaults_and_overrides() {
        let cfg = toml::from_str::<Config>("").unwrap();
        assert_eq!(cfg.net.port, 4242);
        assert!(cfg.net.listen_ip.is_none());

        let cfg = toml::from_str::<Config>(
            r#"
            [net]
            port = 5000
            listen_ip = "127.0.0.1"
        "#,
        )
        .unwrap();
        assert_eq!(cfg.net.listen_ip.as_deref(), Some("127.0.0.1"));
        assert_eq!(cfg.net.port, 5000);
        assert!(cfg.net.token.is_none());
    }

    #[test]
    fn parse_str_tracks_explicit_leaf_fields() {
        let cfg = Config::parse_str(
            r#"
            [scroll]
            natural = false
            enable = false
            horizontal = false
            [gestures]
            tap_to_click = "off"
            [macos]
            sync_system_settings = false
            haptic_feedback = "off"
            "#,
        )
        .unwrap();
        assert!(cfg.has_explicit("scroll.natural"));
        assert!(cfg.has_explicit("scroll.enable"));
        assert!(cfg.has_explicit("scroll.horizontal"));
        assert!(cfg.has_explicit("gestures.tap_to_click"));
        assert!(cfg.has_explicit("macos.sync_system_settings"));
        assert!(cfg.has_explicit("macos.haptic_feedback"));
        assert!(!cfg.has_explicit("scroll.sensitivity"));
    }
}
