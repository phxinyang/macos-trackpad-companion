//! Native-shaped terminal settings editor for the virtual trackpad companion.
//!
//! The navigation mirrors Apple's Trackpad settings groups while keeping
//! companion-only controls explicitly separate. The editor writes only the
//! companion TOML file; it never pretends to create a macOS Trackpad pane.

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use macos_trackpad_companion::config;
#[cfg(target_os = "macos")]
use macos_trackpad_companion::macos_preferences;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use std::collections::BTreeSet;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

#[derive(clap::Parser, Debug)]
#[command(
    name = "companion-tui",
    version,
    about = "Configure the virtual macOS trackpad companion"
)]
struct Args {
    /// TOML path. Defaults to the same path used by companion-net.
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Locale {
    English,
    Chinese,
}

impl Locale {
    fn toggle(&mut self) {
        *self = match self {
            Self::English => Self::Chinese,
            Self::Chinese => Self::English,
        };
    }
    fn language_label(self) -> &'static str {
        match self {
            Self::English => "中文",
            Self::Chinese => "English",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Category {
    PointAndClick,
    ScrollAndZoom,
    MoreGestures,
    Companion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Sidebar,
    Settings,
}

const CATEGORIES: &[Category] = &[
    Category::PointAndClick,
    Category::ScrollAndZoom,
    Category::MoreGestures,
    Category::Companion,
];

impl Category {
    fn title(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::PointAndClick, Locale::English) => "Point & Click",
            (Self::PointAndClick, Locale::Chinese) => "点按与点击",
            (Self::ScrollAndZoom, Locale::English) => "Scroll & Zoom",
            (Self::ScrollAndZoom, Locale::Chinese) => "滚动与缩放",
            (Self::MoreGestures, Locale::English) => "More Gestures",
            (Self::MoreGestures, Locale::Chinese) => "更多手势",
            (Self::Companion, Locale::English) => "Companion",
            (Self::Companion, Locale::Chinese) => "Companion 扩展",
        }
    }

    fn description(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::PointAndClick, Locale::English) => {
                "Pointer tracking, clicking, lookup, and tactile feedback"
            }
            (Self::PointAndClick, Locale::Chinese) => "指针跟踪、点击、查词与触觉反馈",
            (Self::ScrollAndZoom, Locale::English) => {
                "Native-style scrolling, zooming, rotation, and momentum"
            }
            (Self::ScrollAndZoom, Locale::Chinese) => "原生风格滚动、缩放、旋转与惯性",
            (Self::MoreGestures, Locale::English) => {
                "Page, Space, Mission Control, and Notification Center gestures"
            }
            (Self::MoreGestures, Locale::Chinese) => "页面、Space、调度中心与通知中心手势",
            (Self::Companion, Locale::English) => {
                "Virtual-input controls that do not exist in macOS Trackpad settings"
            }
            (Self::Companion, Locale::Chinese) => "虚拟输入专属控制，不冒充 macOS 原生选项",
        }
    }

    fn settings(self) -> &'static [SettingId] {
        match self {
            Self::PointAndClick => POINT_AND_CLICK,
            Self::ScrollAndZoom => SCROLL_AND_ZOOM,
            Self::MoreGestures => MORE_GESTURES,
            Self::Companion => COMPANION,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingId {
    CursorSensitivity,
    ClickPressure,
    QuietClick,
    TapToClick,
    SecondaryClick,
    DictionaryLookup,
    HapticFeedback,
    NaturalScroll,
    Pinch,
    PinchGain,
    SmartZoom,
    Rotate,
    RotateGain,
    ScrollSensitivity,
    ScrollEnabled,
    HorizontalScroll,
    MomentumScroll,
    ModifierZoomMask,
    HorizontalSwipe,
    VerticalSwipe,
    RightEdgeSwipe,
    ThreeFingerDrag,
    PersistentDragLock,
    ReleaseDelay,
    OneFingerTapDrag,
    PressAndHoldDrag,
    CursorAccelExponent,
    CursorAccelRef,
    HorizontalBackend,
    VerticalBackend,
    ShiftScrollHorizontal,
    DynamicTransformCompat,
    ParameterProfile,
    SurfaceWidth,
    SyncSystemSettings,
}

const POINT_AND_CLICK: &[SettingId] = &[
    SettingId::CursorSensitivity,
    SettingId::ClickPressure,
    SettingId::QuietClick,
    SettingId::TapToClick,
    SettingId::SecondaryClick,
    SettingId::DictionaryLookup,
    SettingId::HapticFeedback,
];
const SCROLL_AND_ZOOM: &[SettingId] = &[
    SettingId::NaturalScroll,
    SettingId::ScrollEnabled,
    SettingId::ScrollSensitivity,
    SettingId::MomentumScroll,
    SettingId::HorizontalScroll,
    SettingId::Pinch,
    SettingId::PinchGain,
    SettingId::SmartZoom,
    SettingId::Rotate,
    SettingId::RotateGain,
    SettingId::ModifierZoomMask,
];
const MORE_GESTURES: &[SettingId] = &[
    SettingId::HorizontalSwipe,
    SettingId::VerticalSwipe,
    SettingId::RightEdgeSwipe,
];
const COMPANION: &[SettingId] = &[
    SettingId::ThreeFingerDrag,
    SettingId::PersistentDragLock,
    SettingId::ReleaseDelay,
    SettingId::OneFingerTapDrag,
    SettingId::PressAndHoldDrag,
    SettingId::CursorAccelExponent,
    SettingId::CursorAccelRef,
    SettingId::HorizontalBackend,
    SettingId::VerticalBackend,
    SettingId::ShiftScrollHorizontal,
    SettingId::DynamicTransformCompat,
    SettingId::ParameterProfile,
    SettingId::SurfaceWidth,
    SettingId::SyncSystemSettings,
];

impl SettingId {
    fn label(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::ClickPressure, Locale::English) => "Click",
            (Self::ClickPressure, Locale::Chinese) => "点按力度",
            (Self::QuietClick, Locale::English) => "Quiet Click",
            (Self::QuietClick, Locale::Chinese) => "静音点按",
            (Self::TapToClick, Locale::English) => "Tap to click",
            (Self::TapToClick, Locale::Chinese) => "轻点来点按",
            (Self::SecondaryClick, Locale::English) => "Secondary click",
            (Self::SecondaryClick, Locale::Chinese) => "辅助点按",
            (Self::DictionaryLookup, Locale::English) => "Look up & data detectors",
            (Self::DictionaryLookup, Locale::Chinese) => "查询与数据检测器",
            (Self::CursorSensitivity, Locale::English) => "Tracking speed",
            (Self::CursorSensitivity, Locale::Chinese) => "跟踪速度",
            (Self::HapticFeedback, Locale::English) => "Force Click and haptic feedback",
            (Self::HapticFeedback, Locale::Chinese) => "用力点按与触觉反馈",
            (Self::NaturalScroll, Locale::English) => "Natural scrolling",
            (Self::NaturalScroll, Locale::Chinese) => "自然滚动",
            (Self::Pinch, Locale::English) => "Zoom in or out",
            (Self::Pinch, Locale::Chinese) => "放大或缩小",
            (Self::PinchGain, Locale::English) => "Zoom response",
            (Self::PinchGain, Locale::Chinese) => "缩放响应",
            (Self::SmartZoom, Locale::English) => "Smart zoom",
            (Self::SmartZoom, Locale::Chinese) => "智能缩放",
            (Self::Rotate, Locale::English) => "Rotate",
            (Self::Rotate, Locale::Chinese) => "旋转",
            (Self::RotateGain, Locale::English) => "Rotation response",
            (Self::RotateGain, Locale::Chinese) => "旋转响应",
            (Self::ScrollSensitivity, Locale::English) => "Scroll sensitivity",
            (Self::ScrollSensitivity, Locale::Chinese) => "滚动灵敏度",
            (Self::ScrollEnabled, Locale::English) => "Trackpad scrolling",
            (Self::ScrollEnabled, Locale::Chinese) => "触控板滚动",
            (Self::HorizontalScroll, Locale::English) => "Horizontal scrolling",
            (Self::HorizontalScroll, Locale::Chinese) => "水平滚动",
            (Self::MomentumScroll, Locale::English) => "Momentum scrolling",
            (Self::MomentumScroll, Locale::Chinese) => "惯性滚动",
            (Self::ModifierZoomMask, Locale::English) => "Zoom modifier",
            (Self::ModifierZoomMask, Locale::Chinese) => "缩放修饰键",
            (Self::HorizontalSwipe, Locale::English) => "Swipe between pages",
            (Self::HorizontalSwipe, Locale::Chinese) => "在页面之间轻扫",
            (Self::VerticalSwipe, Locale::English) => "Mission Control",
            (Self::VerticalSwipe, Locale::Chinese) => "调度中心",
            (Self::RightEdgeSwipe, Locale::English) => "Notification Center",
            (Self::RightEdgeSwipe, Locale::Chinese) => "通知中心",
            (Self::ThreeFingerDrag, Locale::English) => "Three-finger drag",
            (Self::ThreeFingerDrag, Locale::Chinese) => "三指拖移",
            (Self::PersistentDragLock, Locale::English) => "Persistent drag lock",
            (Self::PersistentDragLock, Locale::Chinese) => "持久拖移锁定",
            (Self::ReleaseDelay, Locale::English) => "Drag-lock delay",
            (Self::ReleaseDelay, Locale::Chinese) => "拖移锁定延迟",
            (Self::OneFingerTapDrag, Locale::English) => "One-finger tap-drag",
            (Self::OneFingerTapDrag, Locale::Chinese) => "单指轻点拖移",
            (Self::PressAndHoldDrag, Locale::English) => "Press-and-hold drag",
            (Self::PressAndHoldDrag, Locale::Chinese) => "按住拖移",
            (Self::CursorAccelExponent, Locale::English) => "Acceleration curve",
            (Self::CursorAccelExponent, Locale::Chinese) => "加速度曲线",
            (Self::CursorAccelRef, Locale::English) => "Acceleration reference",
            (Self::CursorAccelRef, Locale::Chinese) => "加速度参考速度",
            (Self::HorizontalBackend, Locale::English) => "Horizontal swipe backend",
            (Self::HorizontalBackend, Locale::Chinese) => "水平轻扫后端",
            (Self::VerticalBackend, Locale::English) => "Vertical swipe backend",
            (Self::VerticalBackend, Locale::Chinese) => "垂直轻扫后端",
            (Self::ShiftScrollHorizontal, Locale::English) => "Shift scroll compatibility",
            (Self::ShiftScrollHorizontal, Locale::Chinese) => "Shift 滚动兼容",
            (Self::DynamicTransformCompat, Locale::English) => "Scroll-to-transform compatibility",
            (Self::DynamicTransformCompat, Locale::Chinese) => "滚动转变换兼容",
            (Self::ParameterProfile, Locale::English) => "Gesture parameter profile",
            (Self::ParameterProfile, Locale::Chinese) => "手势参数配置",
            (Self::SurfaceWidth, Locale::English) => "Virtual surface width",
            (Self::SurfaceWidth, Locale::Chinese) => "虚拟触控面宽度",
            (Self::SyncSystemSettings, Locale::English) => "Sync macOS settings",
            (Self::SyncSystemSettings, Locale::Chinese) => "同步 macOS 设置",
        }
    }

    fn description(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::ClickPressure, Locale::English) => {
                "Hardware-only setting; unavailable for virtual input."
            }
            (Self::ClickPressure, Locale::Chinese) => "仅适用于实体触控板；虚拟输入不可用。",
            (Self::QuietClick, Locale::English) => {
                "Hardware-only setting; unavailable for virtual input."
            }
            (Self::QuietClick, Locale::Chinese) => "仅适用于实体触控板；虚拟输入不可用。",
            (Self::TapToClick, Locale::English) => "Tap with one finger to click.",
            (Self::TapToClick, Locale::Chinese) => "用一个手指轻点触控板来点按。",
            (Self::SecondaryClick, Locale::English) => "Click or tap with two fingers.",
            (Self::SecondaryClick, Locale::Chinese) => "用两个手指点按来打开辅助菜单。",
            (Self::DictionaryLookup, Locale::English) => {
                "Use a gesture to look up words and detect addresses or dates."
            }
            (Self::DictionaryLookup, Locale::Chinese) => "使用手势查询单词，并检测地址或日期。",
            (Self::CursorSensitivity, Locale::English) => {
                "Set how fast the pointer moves across the screen."
            }
            (Self::CursorSensitivity, Locale::Chinese) => "设置指针在屏幕上的移动速度。",
            (Self::HapticFeedback, Locale::English) => {
                "Enable tactile confirmation for supported virtual clicks."
            }
            (Self::HapticFeedback, Locale::Chinese) => "为支持的虚拟点按启用触觉确认。",
            (Self::NaturalScroll, Locale::English) => {
                "Move the contents of a window in the same direction as your fingers."
            }
            (Self::NaturalScroll, Locale::Chinese) => "让窗口内容与手指移动方向一致。",
            (Self::Pinch, Locale::English) => "Pinch with two fingers to zoom.",
            (Self::Pinch, Locale::Chinese) => "用两个手指捏合来缩放。",
            (Self::PinchGain, Locale::English) => {
                "Companion-only multiplier for magnification deltas; macOS has no native slider."
            }
            (Self::PinchGain, Locale::Chinese) => {
                "仅限 Companion 的缩放增量倍率；macOS 没有原生滑块。"
            }
            (Self::SmartZoom, Locale::English) => "Double-tap with two fingers to zoom.",
            (Self::SmartZoom, Locale::Chinese) => "用两个手指轻点两下以智能缩放。",
            (Self::Rotate, Locale::English) => "Rotate items with two fingers.",
            (Self::Rotate, Locale::Chinese) => "用两个手指旋转屏幕上的项目。",
            (Self::RotateGain, Locale::English) => {
                "Companion-only multiplier for rotation deltas; macOS has no native slider."
            }
            (Self::RotateGain, Locale::Chinese) => {
                "仅限 Companion 的旋转增量倍率；macOS 没有原生滑块。"
            }
            (Self::ScrollSensitivity, Locale::English) => "Tune the virtual scroll response.",
            (Self::ScrollSensitivity, Locale::Chinese) => "调整虚拟滚动的响应速度。",
            (Self::ScrollEnabled, Locale::English) => "Emit two-finger scroll events.",
            (Self::ScrollEnabled, Locale::Chinese) => "发送双指滚动事件。",
            (Self::HorizontalScroll, Locale::English) => "Preserve horizontal scroll deltas.",
            (Self::HorizontalScroll, Locale::Chinese) => "保留水平滚动分量。",
            (Self::MomentumScroll, Locale::English) => "Continue scrolling after fingers lift.",
            (Self::MomentumScroll, Locale::Chinese) => "抬指后继续滚动一小段惯性。",
            (Self::ModifierZoomMask, Locale::English) => {
                "Choose the keyboard modifier used for scroll-to-zoom."
            }
            (Self::ModifierZoomMask, Locale::Chinese) => "选择滚动缩放使用的键盘修饰键。",
            (Self::HorizontalSwipe, Locale::English) => "Swipe between document pages.",
            (Self::HorizontalSwipe, Locale::Chinese) => "在文档页面之间左右轻扫。",
            (Self::VerticalSwipe, Locale::English) => "Swipe up to open Mission Control.",
            (Self::VerticalSwipe, Locale::Chinese) => "向上轻扫以打开调度中心。",
            (Self::RightEdgeSwipe, Locale::English) => {
                "Swipe from the right edge for notifications."
            }
            (Self::RightEdgeSwipe, Locale::Chinese) => "从右边缘向左轻扫以显示通知中心。",
            (Self::ThreeFingerDrag, Locale::English) => {
                "Hold a virtual click while three fingers move."
            }
            (Self::ThreeFingerDrag, Locale::Chinese) => {
                "三指移动时保持虚拟点按，用于拖移窗口或项目。"
            }
            (Self::PersistentDragLock, Locale::English) => {
                "Keep the drag across 3F → 4F → 3F; moving 1F/2F returns to normal input."
            }
            (Self::PersistentDragLock, Locale::Chinese) => {
                "保持 3F → 4F → 3F 拖移；单指或双指移动会立即恢复普通输入。"
            }
            (Self::ReleaseDelay, Locale::English) => {
                "Keep the drag held briefly while changing grip."
            }
            (Self::ReleaseDelay, Locale::Chinese) => "换手时短暂保持拖移，避免按键粘连。",
            (Self::OneFingerTapDrag, Locale::English) => {
                "Double-tap and hold to drag with one finger."
            }
            (Self::OneFingerTapDrag, Locale::Chinese) => "单指双击后保持按住并拖移。",
            (Self::PressAndHoldDrag, Locale::English) => {
                "Accessibility-style stationary press drag."
            }
            (Self::PressAndHoldDrag, Locale::Chinese) => "无移动时按住即可开始拖移的辅助模式。",
            (Self::CursorAccelExponent, Locale::English) => {
                "Power curve applied to pointer velocity."
            }
            (Self::CursorAccelExponent, Locale::Chinese) => "应用于指针速度的幂函数曲线。",
            (Self::CursorAccelRef, Locale::English) => {
                "Velocity at which acceleration reaches its reference."
            }
            (Self::CursorAccelRef, Locale::Chinese) => "加速度达到参考值时的移动速度。",
            (Self::HorizontalBackend, Locale::English) => {
                "Output used for horizontal Space/page swipes."
            }
            (Self::HorizontalBackend, Locale::Chinese) => "水平 Space/页面轻扫使用的输出方式。",
            (Self::VerticalBackend, Locale::English) => {
                "Output used for Mission Control or notifications."
            }
            (Self::VerticalBackend, Locale::Chinese) => "调度中心或通知手势使用的输出方式。",
            (Self::ShiftScrollHorizontal, Locale::English) => {
                "Optional compatibility remap; native mode keeps the original axis."
            }
            (Self::ShiftScrollHorizontal, Locale::Chinese) => {
                "可选兼容转换；原生模式保留手指的原始滚动轴。"
            }
            (Self::DynamicTransformCompat, Locale::English) => {
                "Allow an established scroll to become pinch or rotate; off matches native scroll lock."
            }
            (Self::DynamicTransformCompat, Locale::Chinese) => {
                "允许已开始的滚动转为缩放或旋转；关闭时遵循原生滚动锁定。"
            }
            (Self::ParameterProfile, Locale::English) => {
                "Use native thresholds or the experimental ChromiumOS gesture profile."
            }
            (Self::ParameterProfile, Locale::Chinese) => {
                "选择原生阈值，或试用 ChromiumOS 的实验参数配置。"
            }
            (Self::SurfaceWidth, Locale::English) => {
                "Width used to locate the right-edge gesture zone."
            }
            (Self::SurfaceWidth, Locale::Chinese) => "用于计算右边缘手势区域的虚拟触控面宽度。",
            (Self::SyncSystemSettings, Locale::English) => {
                "Use available macOS preferences as startup defaults."
            }
            (Self::SyncSystemSettings, Locale::Chinese) => {
                "启动时读取可用的 macOS 偏好作为默认值。"
            }
        }
    }

    fn path(self) -> &'static [&'static str] {
        match self {
            Self::ClickPressure | Self::QuietClick => &[],
            Self::TapToClick => &["gestures", "tap_to_click"],
            Self::SecondaryClick => &["gestures", "secondary_click"],
            Self::DictionaryLookup => &["gestures", "dictionary_lookup"],
            Self::CursorSensitivity => &["cursor", "sensitivity"],
            Self::HapticFeedback => &["macos", "haptic_feedback"],
            Self::NaturalScroll => &["scroll", "natural"],
            Self::Pinch => &["gestures", "pinch", "enable"],
            Self::PinchGain => &["gestures", "pinch", "gain"],
            Self::SmartZoom => &["gestures", "smart_zoom"],
            Self::Rotate => &["gestures", "rotate", "enable"],
            Self::RotateGain => &["gestures", "rotate", "gain"],
            Self::ScrollSensitivity => &["scroll", "sensitivity"],
            Self::ScrollEnabled => &["scroll", "enable"],
            Self::HorizontalScroll => &["scroll", "horizontal"],
            Self::MomentumScroll => &["scroll", "momentum"],
            Self::ModifierZoomMask => &["scroll", "modifier_zoom_mask"],
            Self::HorizontalSwipe => &["gestures", "swipe", "horizontal", "enable"],
            Self::VerticalSwipe => &["gestures", "swipe", "vertical", "enable"],
            Self::RightEdgeSwipe => &["gestures", "right_edge_swipe"],
            Self::ThreeFingerDrag => &["gestures", "three_finger_drag", "enable"],
            Self::PersistentDragLock => &["gestures", "three_finger_drag", "persistent_drag_lock"],
            Self::ReleaseDelay => &["gestures", "three_finger_drag", "release_delay_ms"],
            Self::OneFingerTapDrag => &["gestures", "one_finger_tap_drag", "enable"],
            Self::PressAndHoldDrag => &["gestures", "press_and_hold_drag", "enable"],
            Self::CursorAccelExponent => &["cursor", "accel_exponent"],
            Self::CursorAccelRef => &["cursor", "accel_ref"],
            Self::HorizontalBackend => &["gestures", "swipe", "horizontal", "backend"],
            Self::VerticalBackend => &["gestures", "swipe", "vertical", "backend"],
            Self::ShiftScrollHorizontal => &["scroll", "shift_scroll_horizontal"],
            Self::DynamicTransformCompat => &["gestures", "dynamic_transform_compat"],
            Self::ParameterProfile => &["gestures", "parameter_profile"],
            Self::SurfaceWidth => &["gestures", "surface_width_mm"],
            Self::SyncSystemSettings => &["macos", "sync_system_settings"],
        }
    }
}

struct App {
    cfg: config::Config,
    path: PathBuf,
    category: usize,
    selected: usize,
    focus: Focus,
    locale: Locale,
    changed: BTreeSet<String>,
    status: String,
    system: String,
    quit_pending: bool,
}

impl App {
    fn new(cfg: config::Config, path: PathBuf) -> Self {
        Self {
            cfg,
            path,
            category: 0,
            selected: 0,
            focus: Focus::Settings,
            locale: Locale::Chinese,
            changed: BTreeSet::new(),
            status: "未修改".into(),
            system: system_summary(),
            quit_pending: false,
        }
    }
    fn current_category(&self) -> Category {
        CATEGORIES[self.category]
    }
    fn current_settings(&self) -> &'static [SettingId] {
        self.current_category().settings()
    }
    fn selected_id(&self) -> SettingId {
        self.current_settings()[self.selected]
    }
    fn select_category(&mut self, index: usize) {
        self.category = index % CATEGORIES.len();
        self.selected = 0;
        self.quit_pending = false;
    }

    fn move_setting(&mut self, delta: isize) {
        let len = self.current_settings().len();
        self.selected = ((self.selected as isize + delta).rem_euclid(len as isize)) as usize;
        self.quit_pending = false;
    }
    fn mark_changed(&mut self) {
        self.changed.insert(self.selected_id().path().join("."));
        self.status = match self.locale {
            Locale::English => format!("{} unsaved change(s)", self.changed.len()),
            Locale::Chinese => format!("待保存：{} 项修改", self.changed.len()),
        };
        self.quit_pending = false;
    }

    fn value(&self, id: SettingId) -> String {
        use config::HapticSetting;
        match id {
            SettingId::ClickPressure | SettingId::QuietClick => match self.locale {
                Locale::English => "Unavailable".into(),
                Locale::Chinese => "不可用".into(),
            },
            SettingId::TapToClick => enable_text(&self.cfg.gestures.tap_to_click, self.locale),
            SettingId::SecondaryClick => {
                enable_text(&self.cfg.gestures.secondary_click, self.locale)
            }
            SettingId::DictionaryLookup => {
                enable_text(&self.cfg.gestures.dictionary_lookup, self.locale)
            }
            SettingId::CursorSensitivity => format!("{:.1} px/mm", self.cfg.cursor.sensitivity),
            SettingId::HapticFeedback => match (self.cfg.macos.haptic_feedback, self.locale) {
                (HapticSetting::Auto, Locale::English) => "Automatic".into(),
                (HapticSetting::On, Locale::English) => "On".into(),
                (HapticSetting::Off, Locale::English) => "Off".into(),
                (HapticSetting::Auto, Locale::Chinese) => "自动".into(),
                (HapticSetting::On, Locale::Chinese) => "打开".into(),
                (HapticSetting::Off, Locale::Chinese) => "关闭".into(),
            },
            SettingId::NaturalScroll => bool_text(self.cfg.scroll.natural, self.locale),
            SettingId::Pinch => enable_text(&self.cfg.gestures.pinch.enable, self.locale),
            SettingId::PinchGain => format!("{:.2}x", self.cfg.gestures.pinch.gain),
            SettingId::SmartZoom => enable_text(&self.cfg.gestures.smart_zoom, self.locale),
            SettingId::Rotate => enable_text(&self.cfg.gestures.rotate.enable, self.locale),
            SettingId::RotateGain => format!("{:.2}x", self.cfg.gestures.rotate.gain),
            SettingId::ScrollSensitivity => format!("{:.1} px/mm", self.cfg.scroll.sensitivity),
            SettingId::ScrollEnabled => bool_text(self.cfg.scroll.enable, self.locale),
            SettingId::HorizontalScroll => bool_text(self.cfg.scroll.horizontal, self.locale),
            SettingId::MomentumScroll => bool_text(self.cfg.scroll.momentum, self.locale),
            SettingId::ModifierZoomMask => self
                .cfg
                .scroll
                .modifier_zoom_mask
                .map(|mask| modifier_mask_text(mask, self.locale))
                .unwrap_or_else(|| match self.locale {
                    Locale::English => "Default (Cmd/Ctrl)".into(),
                    Locale::Chinese => "默认（Command/Control）".into(),
                }),
            SettingId::HorizontalSwipe => {
                enable_text(&self.cfg.gestures.swipe.horizontal.enable, self.locale)
            }
            SettingId::VerticalSwipe => {
                enable_text(&self.cfg.gestures.swipe.vertical.enable, self.locale)
            }
            SettingId::RightEdgeSwipe => {
                enable_text(&self.cfg.gestures.right_edge_swipe, self.locale)
            }
            SettingId::ThreeFingerDrag => {
                enable_text(&self.cfg.gestures.three_finger_drag.enable, self.locale)
            }
            SettingId::PersistentDragLock => bool_text(
                self.cfg.gestures.three_finger_drag.persistent_drag_lock,
                self.locale,
            ),
            SettingId::ReleaseDelay => format!(
                "{} ms",
                self.cfg.gestures.three_finger_drag.release_delay_ms
            ),
            SettingId::OneFingerTapDrag => {
                enable_text(&self.cfg.gestures.one_finger_tap_drag.enable, self.locale)
            }
            SettingId::PressAndHoldDrag => {
                enable_text(&self.cfg.gestures.press_and_hold_drag.enable, self.locale)
            }
            SettingId::CursorAccelExponent => format!("{:.2}", self.cfg.cursor.accel_exponent),
            SettingId::CursorAccelRef => format!("{:.1} mm/s", self.cfg.cursor.accel_ref),
            SettingId::HorizontalBackend => {
                backend_text(self.cfg.gestures.swipe.horizontal.backend, self.locale)
            }
            SettingId::VerticalBackend => {
                backend_text(self.cfg.gestures.swipe.vertical.backend, self.locale)
            }
            SettingId::ShiftScrollHorizontal => {
                bool_text(self.cfg.scroll.shift_scroll_horizontal, self.locale)
            }
            SettingId::DynamicTransformCompat => {
                bool_text(self.cfg.gestures.dynamic_transform_compat, self.locale)
            }
            SettingId::ParameterProfile => match (self.cfg.gestures.parameter_profile, self.locale)
            {
                (config::GestureParameterProfile::Native, Locale::English) => "Native".into(),
                (config::GestureParameterProfile::Native, Locale::Chinese) => "原生".into(),
                (config::GestureParameterProfile::ChromiumOs, Locale::English) => {
                    "ChromiumOS (experimental)".into()
                }
                (config::GestureParameterProfile::ChromiumOs, Locale::Chinese) => {
                    "ChromiumOS（实验）".into()
                }
            },
            SettingId::SurfaceWidth => format!("{:.1} mm", self.cfg.gestures.surface_width_mm),
            SettingId::SyncSystemSettings => {
                bool_text(self.cfg.macos.sync_system_settings, self.locale)
            }
        }
    }

    fn adjust(&mut self, increase: bool) {
        use config::HapticSetting;
        let id = self.selected_id();
        if matches!(id, SettingId::ClickPressure | SettingId::QuietClick) {
            self.status = match self.locale {
                Locale::English => "Hardware-only setting; nothing to change".into(),
                Locale::Chinese => "硬件专属设置，虚拟输入无需修改".into(),
            };
            return;
        }
        let signed = if increase { 1.0 } else { -1.0 };
        match id {
            SettingId::ClickPressure | SettingId::QuietClick => {
                unreachable!("read-only rows return before adjust")
            }
            SettingId::TapToClick => toggle_enable(&mut self.cfg.gestures.tap_to_click),
            SettingId::SecondaryClick => toggle_enable(&mut self.cfg.gestures.secondary_click),
            SettingId::DictionaryLookup => toggle_enable(&mut self.cfg.gestures.dictionary_lookup),
            SettingId::CursorSensitivity => {
                self.cfg.cursor.sensitivity =
                    (self.cfg.cursor.sensitivity + signed).clamp(5.0, 80.0)
            }
            SettingId::HapticFeedback => {
                self.cfg.macos.haptic_feedback = match (self.cfg.macos.haptic_feedback, increase) {
                    (HapticSetting::Auto, true) => HapticSetting::On,
                    (HapticSetting::On, true) => HapticSetting::Off,
                    (HapticSetting::Off, true) => HapticSetting::Auto,
                    (HapticSetting::Auto, false) => HapticSetting::Off,
                    (HapticSetting::Off, false) => HapticSetting::On,
                    (HapticSetting::On, false) => HapticSetting::Auto,
                }
            }
            SettingId::NaturalScroll => self.cfg.scroll.natural = !self.cfg.scroll.natural,
            SettingId::Pinch => toggle_enable(&mut self.cfg.gestures.pinch.enable),
            SettingId::PinchGain => {
                self.cfg.gestures.pinch.gain =
                    (self.cfg.gestures.pinch.gain + signed * 0.1).clamp(0.25, 4.0)
            }
            SettingId::SmartZoom => toggle_enable(&mut self.cfg.gestures.smart_zoom),
            SettingId::Rotate => toggle_enable(&mut self.cfg.gestures.rotate.enable),
            SettingId::RotateGain => {
                self.cfg.gestures.rotate.gain =
                    (self.cfg.gestures.rotate.gain + signed * 0.1).clamp(0.25, 4.0)
            }
            SettingId::ScrollSensitivity => {
                self.cfg.scroll.sensitivity =
                    (self.cfg.scroll.sensitivity + signed).clamp(5.0, 80.0)
            }
            SettingId::ScrollEnabled => self.cfg.scroll.enable = !self.cfg.scroll.enable,
            SettingId::HorizontalScroll => self.cfg.scroll.horizontal = !self.cfg.scroll.horizontal,
            SettingId::MomentumScroll => self.cfg.scroll.momentum = !self.cfg.scroll.momentum,
            SettingId::ModifierZoomMask => {
                cycle_modifier_mask(&mut self.cfg.scroll.modifier_zoom_mask, increase)
            }
            SettingId::HorizontalSwipe => {
                toggle_enable(&mut self.cfg.gestures.swipe.horizontal.enable)
            }
            SettingId::VerticalSwipe => toggle_enable(&mut self.cfg.gestures.swipe.vertical.enable),
            SettingId::RightEdgeSwipe => toggle_enable(&mut self.cfg.gestures.right_edge_swipe),
            SettingId::ThreeFingerDrag => {
                toggle_enable(&mut self.cfg.gestures.three_finger_drag.enable)
            }
            SettingId::PersistentDragLock => {
                self.cfg.gestures.three_finger_drag.persistent_drag_lock =
                    !self.cfg.gestures.three_finger_drag.persistent_drag_lock
            }
            SettingId::ReleaseDelay => {
                let delta = if increase { 50 } else { -50 };
                let value = self.cfg.gestures.three_finger_drag.release_delay_ms as i64;
                self.cfg.gestures.three_finger_drag.release_delay_ms =
                    (value + delta).clamp(0, 2000) as u64;
            }
            SettingId::OneFingerTapDrag => {
                toggle_enable(&mut self.cfg.gestures.one_finger_tap_drag.enable)
            }
            SettingId::PressAndHoldDrag => {
                toggle_enable(&mut self.cfg.gestures.press_and_hold_drag.enable)
            }
            SettingId::CursorAccelExponent => {
                self.cfg.cursor.accel_exponent =
                    (self.cfg.cursor.accel_exponent + signed * 0.05).clamp(1.0, 2.0)
            }
            SettingId::CursorAccelRef => {
                self.cfg.cursor.accel_ref =
                    (self.cfg.cursor.accel_ref + signed * 5.0).clamp(20.0, 200.0)
            }
            SettingId::HorizontalBackend => {
                cycle_backend(&mut self.cfg.gestures.swipe.horizontal.backend, increase)
            }
            SettingId::VerticalBackend => {
                cycle_backend(&mut self.cfg.gestures.swipe.vertical.backend, increase)
            }
            SettingId::ShiftScrollHorizontal => {
                self.cfg.scroll.shift_scroll_horizontal = !self.cfg.scroll.shift_scroll_horizontal
            }
            SettingId::DynamicTransformCompat => {
                self.cfg.gestures.dynamic_transform_compat =
                    !self.cfg.gestures.dynamic_transform_compat
            }
            SettingId::ParameterProfile => {
                let _ = increase;
                self.cfg.gestures.parameter_profile = match self.cfg.gestures.parameter_profile {
                    config::GestureParameterProfile::Native => {
                        config::GestureParameterProfile::ChromiumOs
                    }
                    config::GestureParameterProfile::ChromiumOs => {
                        config::GestureParameterProfile::Native
                    }
                };
            }
            SettingId::SurfaceWidth => {
                self.cfg.gestures.surface_width_mm =
                    (self.cfg.gestures.surface_width_mm + signed).clamp(20.0, 200.0);
            }
            SettingId::SyncSystemSettings => {
                self.cfg.macos.sync_system_settings = !self.cfg.macos.sync_system_settings
            }
        }
        self.mark_changed();
    }

    fn save(&mut self) -> Result<()> {
        if self.changed.is_empty() {
            self.status = match self.locale {
                Locale::English => "No changes to save".into(),
                Locale::Chinese => "没有需要保存的修改".into(),
            };
            return Ok(());
        }
        let mut root = if self.path.exists() {
            let source = std::fs::read_to_string(&self.path)
                .with_context(|| format!("read config {}", self.path.display()))?;
            toml::from_str::<toml::Value>(&source).context("parse config for TUI save")?
        } else {
            toml::Value::Table(toml::map::Map::new())
        };
        for id in all_settings()
            .iter()
            .copied()
            .filter(|id| self.changed.contains(&id.path().join(".")))
        {
            if matches!(id, SettingId::ModifierZoomMask)
                && self.cfg.scroll.modifier_zoom_mask.is_none()
            {
                remove_value(&mut root, id.path());
            } else {
                set_value(&mut root, id.path(), self.toml_value(id));
            }
        }
        let rendered = toml::to_string_pretty(&root).context("render config")?;
        let tmp = self.path.with_extension("toml.tmp");
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create config directory {}", parent.display()))?;
        }
        std::fs::write(&tmp, rendered).with_context(|| format!("write {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)
            .with_context(|| format!("replace config {}", self.path.display()))?;
        self.changed.clear();
        self.status = match self.locale {
            Locale::English => format!("Saved {}", self.path.display()),
            Locale::Chinese => format!("已保存 {}", self.path.display()),
        };
        self.quit_pending = false;
        Ok(())
    }

    fn toml_value(&self, id: SettingId) -> toml::Value {
        use config::HapticSetting;
        match id {
            SettingId::ClickPressure | SettingId::QuietClick => {
                toml::Value::String("unsupported".into())
            }
            SettingId::TapToClick => enable_value(&self.cfg.gestures.tap_to_click),
            SettingId::SecondaryClick => enable_value(&self.cfg.gestures.secondary_click),
            SettingId::DictionaryLookup => enable_value(&self.cfg.gestures.dictionary_lookup),
            SettingId::CursorSensitivity => toml::Value::Float(self.cfg.cursor.sensitivity),
            SettingId::HapticFeedback => toml::Value::String(
                match self.cfg.macos.haptic_feedback {
                    HapticSetting::Auto => "auto",
                    HapticSetting::On => "on",
                    HapticSetting::Off => "off",
                }
                .into(),
            ),
            SettingId::NaturalScroll => toml::Value::Boolean(self.cfg.scroll.natural),
            SettingId::Pinch => enable_value(&self.cfg.gestures.pinch.enable),
            SettingId::PinchGain => toml::Value::Float(self.cfg.gestures.pinch.gain),
            SettingId::SmartZoom => enable_value(&self.cfg.gestures.smart_zoom),
            SettingId::Rotate => enable_value(&self.cfg.gestures.rotate.enable),
            SettingId::RotateGain => toml::Value::Float(self.cfg.gestures.rotate.gain),
            SettingId::ScrollSensitivity => toml::Value::Float(self.cfg.scroll.sensitivity),
            SettingId::ScrollEnabled => toml::Value::Boolean(self.cfg.scroll.enable),
            SettingId::HorizontalScroll => toml::Value::Boolean(self.cfg.scroll.horizontal),
            SettingId::MomentumScroll => toml::Value::Boolean(self.cfg.scroll.momentum),
            SettingId::ModifierZoomMask => {
                toml::Value::Integer(self.cfg.scroll.modifier_zoom_mask.unwrap_or_default() as i64)
            }
            SettingId::HorizontalSwipe => enable_value(&self.cfg.gestures.swipe.horizontal.enable),
            SettingId::VerticalSwipe => enable_value(&self.cfg.gestures.swipe.vertical.enable),
            SettingId::RightEdgeSwipe => enable_value(&self.cfg.gestures.right_edge_swipe),
            SettingId::ThreeFingerDrag => enable_value(&self.cfg.gestures.three_finger_drag.enable),
            SettingId::PersistentDragLock => {
                toml::Value::Boolean(self.cfg.gestures.three_finger_drag.persistent_drag_lock)
            }
            SettingId::ReleaseDelay => {
                toml::Value::Integer(self.cfg.gestures.three_finger_drag.release_delay_ms as i64)
            }
            SettingId::OneFingerTapDrag => {
                enable_value(&self.cfg.gestures.one_finger_tap_drag.enable)
            }
            SettingId::PressAndHoldDrag => {
                enable_value(&self.cfg.gestures.press_and_hold_drag.enable)
            }
            SettingId::CursorAccelExponent => toml::Value::Float(self.cfg.cursor.accel_exponent),
            SettingId::CursorAccelRef => toml::Value::Float(self.cfg.cursor.accel_ref),
            SettingId::HorizontalBackend => toml::Value::String(backend_text(
                self.cfg.gestures.swipe.horizontal.backend,
                Locale::English,
            )),
            SettingId::VerticalBackend => toml::Value::String(backend_text(
                self.cfg.gestures.swipe.vertical.backend,
                Locale::English,
            )),
            SettingId::ShiftScrollHorizontal => {
                toml::Value::Boolean(self.cfg.scroll.shift_scroll_horizontal)
            }
            SettingId::DynamicTransformCompat => {
                toml::Value::Boolean(self.cfg.gestures.dynamic_transform_compat)
            }
            SettingId::ParameterProfile => toml::Value::String(
                match self.cfg.gestures.parameter_profile {
                    config::GestureParameterProfile::Native => "native",
                    config::GestureParameterProfile::ChromiumOs => "chromium_os",
                }
                .into(),
            ),
            SettingId::SurfaceWidth => toml::Value::Float(self.cfg.gestures.surface_width_mm),
            SettingId::SyncSystemSettings => {
                toml::Value::Boolean(self.cfg.macos.sync_system_settings)
            }
        }
    }
}

fn all_settings() -> Vec<SettingId> {
    CATEGORIES
        .iter()
        .flat_map(|category| category.settings().iter().copied())
        .collect()
}
fn enable_text(value: &config::GestureEnable, locale: Locale) -> String {
    match value {
        config::GestureEnable::On => match locale {
            Locale::English => "On",
            Locale::Chinese => "打开",
        }
        .into(),
        config::GestureEnable::Off => match locale {
            Locale::English => "Off",
            Locale::Chinese => "关闭",
        }
        .into(),
        config::GestureEnable::Only(_) => match locale {
            Locale::English => "Only (…) ",
            Locale::Chinese => "仅 (…)",
        }
        .trim()
        .into(),
        config::GestureEnable::Except(_) => match locale {
            Locale::English => "Except (…) ",
            Locale::Chinese => "排除 (…)",
        }
        .trim()
        .into(),
    }
}
fn enable_value(value: &config::GestureEnable) -> toml::Value {
    match value {
        config::GestureEnable::On => toml::Value::String("on".into()),
        config::GestureEnable::Off => toml::Value::String("off".into()),
        config::GestureEnable::Only(apps) => toml::Value::Table(toml::map::Map::from_iter([(
            "only".into(),
            toml::Value::Array(apps.iter().cloned().map(toml::Value::String).collect()),
        )])),
        config::GestureEnable::Except(apps) => toml::Value::Table(toml::map::Map::from_iter([(
            "except".into(),
            toml::Value::Array(apps.iter().cloned().map(toml::Value::String).collect()),
        )])),
    }
}
fn toggle_enable(value: &mut config::GestureEnable) {
    *value = match value {
        config::GestureEnable::On => config::GestureEnable::Off,
        config::GestureEnable::Off => config::GestureEnable::On,
        config::GestureEnable::Only(_) => config::GestureEnable::Off,
        config::GestureEnable::Except(_) => config::GestureEnable::On,
    };
}
fn bool_text(value: bool, locale: Locale) -> String {
    match (value, locale) {
        (true, Locale::English) => "On".into(),
        (false, Locale::English) => "Off".into(),
        (true, Locale::Chinese) => "打开".into(),
        (false, Locale::Chinese) => "关闭".into(),
    }
}
fn backend_text(value: config::SwipeBackend, locale: Locale) -> String {
    match (value, locale) {
        (config::SwipeBackend::Synthetic, Locale::English) => "Synthetic".into(),
        (config::SwipeBackend::Notification, Locale::English) => "Notification".into(),
        (config::SwipeBackend::Off, Locale::English) => "Off".into(),
        (config::SwipeBackend::Synthetic, Locale::Chinese) => "合成事件".into(),
        (config::SwipeBackend::Notification, Locale::Chinese) => "通知事件".into(),
        (config::SwipeBackend::Off, Locale::Chinese) => "关闭".into(),
    }
}
fn modifier_mask_text(mask: u64, locale: Locale) -> String {
    match (mask, locale) {
        (0x0004_0000, Locale::English) => "Control".into(),
        (0x0008_0000, Locale::English) => "Option".into(),
        (0x0010_0000, Locale::English) => "Command".into(),
        (0x0004_0000, Locale::Chinese) => "Control".into(),
        (0x0008_0000, Locale::Chinese) => "Option".into(),
        (0x0010_0000, Locale::Chinese) => "Command".into(),
        (other, _) => format!("0x{other:x}"),
    }
}
fn cycle_modifier_mask(value: &mut Option<u64>, increase: bool) {
    const OPTIONS: [Option<u64>; 4] = [
        None,
        Some(0x0004_0000),
        Some(0x0008_0000),
        Some(0x0010_0000),
    ];
    let index = OPTIONS
        .iter()
        .position(|candidate| candidate == value)
        .unwrap_or(0);
    let next = if increase {
        (index + 1) % OPTIONS.len()
    } else {
        (index + OPTIONS.len() - 1) % OPTIONS.len()
    };
    *value = OPTIONS[next];
}
fn cycle_backend(value: &mut config::SwipeBackend, increase: bool) {
    use config::SwipeBackend::{Notification, Off, Synthetic};
    *value = match (*value, increase) {
        (Synthetic, true) => Notification,
        (Notification, true) => Off,
        (Off, true) => Synthetic,
        (Synthetic, false) => Off,
        (Off, false) => Notification,
        (Notification, false) => Synthetic,
    };
}
fn set_value(root: &mut toml::Value, path: &[&str], value: toml::Value) {
    if path.is_empty() {
        *root = value;
        return;
    }
    if !root.is_table() {
        *root = toml::Value::Table(toml::map::Map::new());
    }
    let table = root.as_table_mut().expect("table initialized");
    if path.len() == 1 {
        table.insert(path[0].into(), value);
        return;
    }
    let child = table
        .entry(path[0])
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    set_value(child, &path[1..], value);
}
fn remove_value(root: &mut toml::Value, path: &[&str]) {
    let Some(table) = root.as_table_mut() else {
        return;
    };
    if path.len() == 1 {
        table.remove(path[0]);
        return;
    }
    if let Some(child) = table.get_mut(path[0]) {
        remove_value(child, &path[1..]);
    }
}
fn system_summary() -> String {
    #[cfg(target_os = "macos")]
    {
        let raw = macos_preferences::read_raw();
        let clicking = raw
            .value("Clicking")
            .map(|value| format!("{value:?}"))
            .unwrap_or_else(|| "missing".into());
        format!(
            "macOS prefs: trackpad={} global={} Clicking={}; virtual input ignores Clicking",
            raw.trackpad_domain_available(),
            raw.global_domain_available(),
            clicking
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        "macOS prefs: unavailable on this platform".into()
    }
}

fn draw(frame: &mut ratatui::Frame<'_>, app: &App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(frame.area());
    let (title, subtitle) = match app.locale {
        Locale::English => (
            "Trackpad",
            "Virtual input settings · macOS-style organization",
        ),
        Locale::Chinese => ("触控板", "虚拟输入设置 · 按 macOS 原生结构组织"),
    };
    let header = Paragraph::new(vec![
        Line::from(Span::styled(
            format!("  {title}"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("  {subtitle}")),
        Line::from(format!("  {}", app.path.display())),
        Line::from(format!("  {}", app.system)),
    ])
    .block(Block::default().borders(Borders::BOTTOM))
    .wrap(Wrap { trim: true });
    frame.render_widget(header, areas[0]);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(25), Constraint::Min(35)])
        .split(areas[1]);
    let nav_items: Vec<ListItem<'_>> = CATEGORIES
        .iter()
        .map(|category| ListItem::new(format!("  {}", category.title(app.locale))))
        .collect();
    let nav = List::new(nav_items)
        .block(
            Block::default()
                .borders(Borders::RIGHT)
                .border_style(if app.focus == Focus::Sidebar {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                })
                .title(match app.locale {
                    Locale::English => "Settings",
                    Locale::Chinese => "设置",
                }),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );
    let mut nav_state = ListState::default();
    nav_state.select(Some(app.category));
    frame.render_stateful_widget(nav, columns[0], &mut nav_state);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(4)])
        .split(columns[1]);
    let settings = app.current_settings();
    let items: Vec<ListItem<'_>> = settings
        .iter()
        .map(|id| {
            let marker = if app.changed.contains(&id.path().join(".")) {
                "●"
            } else {
                " "
            };
            let row = Line::from(vec![
                Span::styled(
                    format!("{marker} "),
                    Style::default().fg(if marker == "●" {
                        Color::Yellow
                    } else {
                        Color::DarkGray
                    }),
                ),
                Span::styled(
                    format!("{:<33}", id.label(app.locale)),
                    Style::default().fg(Color::White),
                ),
                Span::styled(app.value(*id), Style::default().fg(Color::Cyan)),
            ]);
            ListItem::new(Text::from(vec![row]))
        })
        .collect();
    let list_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", app.current_category().title(app.locale)))
        .border_style(if app.focus == Focus::Settings {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        });
    let list = List::new(items)
        .block(list_block)
        .highlight_style(Style::default().bg(Color::Rgb(38, 54, 78)).fg(Color::White));
    let mut setting_state = ListState::default();
    setting_state.select(Some(app.selected));
    frame.render_stateful_widget(list, right[0], &mut setting_state);
    let detail = app.selected_id();
    let desc =
        Paragraph::new(vec![
            Line::from(Span::styled(
                detail.description(app.locale),
                Style::default().fg(Color::Gray),
            )),
            Line::from(format!(
                "  {}",
                app.current_category().description(app.locale)
            )),
        ])
        .block(Block::default().borders(Borders::TOP).border_style(
            if app.focus == Focus::Settings {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            },
        ))
        .wrap(Wrap { trim: true });
    frame.render_widget(desc, right[1]);
    let footer = Paragraph::new(Line::from(vec![Span::styled(&app.status, Style::default().fg(Color::Green)), Span::raw(match app.locale { Locale::English => "   ↑↓/jk select  ←→/Enter adjust  Tab focus  [] section  l language  s save  r reload  q quit", Locale::Chinese => "   ↑↓/jk 选择  ←→/Enter 调整  Tab 焦点  [] 分组  l 语言  s 保存  r 重载  q 退出" })])).block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, areas[2]);
}

fn run_tui(mut app: App) -> Result<()> {
    enable_raw_mode().context("enable terminal raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;
    let result = event_loop(&mut terminal, &mut app);
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();
    result
}
fn event_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal
            .draw(|frame| draw(frame, app))
            .context("draw TUI")?;
        if !event::poll(Duration::from_millis(250)).context("poll terminal")? {
            continue;
        }
        let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event::read().context("read terminal event")?
        else {
            continue;
        };
        match (code, modifiers) {
            (KeyCode::Char('q'), KeyModifiers::CONTROL) => return Ok(()),
            (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => {
                if app.changed.is_empty() || app.quit_pending {
                    return Ok(());
                }
                app.quit_pending = true;
                app.status = match app.locale {
                    Locale::English => {
                        "Unsaved changes: press s to save or q again to discard".into()
                    }
                    Locale::Chinese => "有未保存修改：按 s 保存，再按 q 放弃".into(),
                };
            }
            (KeyCode::Tab, _) => {
                app.focus = match app.focus {
                    Focus::Sidebar => Focus::Settings,
                    Focus::Settings => Focus::Sidebar,
                }
            }
            (KeyCode::BackTab, _) => {
                app.focus = match app.focus {
                    Focus::Sidebar => Focus::Settings,
                    Focus::Settings => Focus::Sidebar,
                }
            }
            (KeyCode::Char('['), _) => {
                app.select_category((app.category + CATEGORIES.len() - 1) % CATEGORIES.len())
            }
            (KeyCode::Char(']'), _) => app.select_category((app.category + 1) % CATEGORIES.len()),
            (KeyCode::Char('l'), _) => {
                app.locale.toggle();
                app.status = app.locale.language_label().to_string();
            }
            (KeyCode::Left, _) => match app.focus {
                Focus::Sidebar => {
                    app.select_category((app.category + CATEGORIES.len() - 1) % CATEGORIES.len())
                }
                Focus::Settings => app.focus = Focus::Sidebar,
            },
            (KeyCode::Right, _) => match app.focus {
                Focus::Sidebar => app.focus = Focus::Settings,
                Focus::Settings => app.adjust(true),
            },
            (KeyCode::Enter, _) | (KeyCode::Char(' '), _) => {
                if app.focus == Focus::Sidebar {
                    app.focus = Focus::Settings;
                } else {
                    app.adjust(true);
                }
            }
            (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
                if app.focus == Focus::Sidebar {
                    app.select_category((app.category + CATEGORIES.len() - 1) % CATEGORIES.len());
                } else {
                    app.move_setting(-1);
                }
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
                if app.focus == Focus::Sidebar {
                    app.select_category((app.category + 1) % CATEGORIES.len());
                } else {
                    app.move_setting(1);
                }
            }
            (KeyCode::Char('s'), _) => {
                app.quit_pending = false;
                app.save()?;
            }
            (KeyCode::Char('r'), _) => {
                let (cfg, path) = config::load(Some(&app.path))?;
                app.cfg = cfg;
                app.path = path;
                app.changed.clear();
                app.quit_pending = false;
                app.status = match app.locale {
                    Locale::English => "Reloaded from disk".into(),
                    Locale::Chinese => "已从磁盘重载".into(),
                };
            }
            _ => {}
        }
    }
}
fn main() -> Result<()> {
    let args = <Args as clap::Parser>::parse();
    let (cfg, path) = config::load(args.config.as_deref())?;
    run_tui(App::new(cfg, path))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn categories_follow_native_order_and_have_companion_extension() {
        assert_eq!(CATEGORIES[0], Category::PointAndClick);
        assert_eq!(CATEGORIES[1], Category::ScrollAndZoom);
        assert_eq!(CATEGORIES[2], Category::MoreGestures);
        assert!(
            Category::Companion
                .settings()
                .contains(&SettingId::ThreeFingerDrag)
        );
        assert!(
            Category::Companion
                .settings()
                .contains(&SettingId::PersistentDragLock)
        );
    }
    #[test]
    fn language_switch_changes_labels() {
        assert_eq!(SettingId::TapToClick.label(Locale::English), "Tap to click");
        assert_eq!(SettingId::TapToClick.label(Locale::Chinese), "轻点来点按");
    }

    #[test]
    fn hardware_only_rows_are_read_only() {
        let mut app = App::new(config::Config::default(), PathBuf::from("config.toml"));
        app.selected = 1;
        app.adjust(true);
        assert!(app.changed.is_empty());
        assert!(app.status.contains("硬件") || app.status.contains("Hardware"));
    }

    #[test]
    fn parameter_profile_cycles_and_serializes() {
        let mut app = App::new(config::Config::default(), PathBuf::from("config.toml"));
        app.category = CATEGORIES
            .iter()
            .position(|category| *category == Category::Companion)
            .unwrap();
        app.selected = COMPANION
            .iter()
            .position(|id| *id == SettingId::ParameterProfile)
            .unwrap();

        assert_eq!(app.value(SettingId::ParameterProfile), "原生");
        app.adjust(true);
        assert_eq!(
            app.cfg.gestures.parameter_profile,
            config::GestureParameterProfile::ChromiumOs
        );
        assert_eq!(app.value(SettingId::ParameterProfile), "ChromiumOS（实验）");
        assert_eq!(
            app.toml_value(SettingId::ParameterProfile)
                .as_str()
                .unwrap(),
            "chromium_os"
        );
        app.adjust(false);
        assert_eq!(
            app.cfg.gestures.parameter_profile,
            config::GestureParameterProfile::Native
        );
    }

    #[test]
    fn persistent_drag_lock_toggles_and_serializes() {
        let mut app = App::new(config::Config::default(), PathBuf::from("config.toml"));
        app.category = CATEGORIES
            .iter()
            .position(|category| *category == Category::Companion)
            .unwrap();
        app.selected = COMPANION
            .iter()
            .position(|id| *id == SettingId::PersistentDragLock)
            .unwrap();

        assert_eq!(app.value(SettingId::PersistentDragLock), "打开");
        app.adjust(true);
        assert!(!app.cfg.gestures.three_finger_drag.persistent_drag_lock);
        assert_eq!(
            app.toml_value(SettingId::PersistentDragLock).as_bool(),
            Some(false)
        );
    }

    #[test]
    fn set_value_creates_nested_tables() {
        let mut root = toml::Value::Table(toml::map::Map::new());
        set_value(
            &mut root,
            &["scroll", "modifier_zoom_mask"],
            toml::Value::Integer(262_144),
        );
        assert_eq!(
            root.get("scroll")
                .and_then(|value| value.get("modifier_zoom_mask"))
                .and_then(toml::Value::as_integer),
            Some(262_144)
        );
    }
    #[test]
    fn remove_value_clears_nested_leaf() {
        let mut root: toml::Value =
            toml::from_str("[scroll]\nsensitivity = 20.0\nmodifier_zoom_mask = 262144\n").unwrap();
        remove_value(&mut root, &["scroll", "modifier_zoom_mask"]);
        assert!(
            root.get("scroll")
                .and_then(|value| value.get("modifier_zoom_mask"))
                .is_none()
        );
        assert_eq!(
            root.get("scroll")
                .and_then(|value| value.get("sensitivity"))
                .and_then(toml::Value::as_float),
            Some(20.0)
        );
    }
}
