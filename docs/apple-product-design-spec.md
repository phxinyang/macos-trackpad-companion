# Apple-style product design specification

Status: implemented, 2026-08-29. Live WebSocket and macOS application-matrix verification remain host-dependent.

This document is the source of truth for the Android APK and the browser touchpad surface. It borrows Apple's hierarchy, semantic color, system typography, material layering, and interaction discipline without claiming to be a native Apple control.

## Product frame

The product is a utility, not a marketing page. The first screen must answer three questions immediately:

1. Is the Mac reachable?
2. Where is the touch surface?
3. Where do I change the few settings that affect muscle memory?

The touch surface is the primary content layer and now uses a bounded liquid-glass treatment with a low-contrast sampled backdrop. The input view remains a clear child above the material, so the material never intercepts pointer/touch events. Connection status, settings, and diagnostics stay in the functional chrome and use the same material vocabulary without stacking glass on glass.

## Evidence used

- Apple Materials: <https://developer.apple.com/design/human-interface-guidelines/materials>. Liquid Glass is a functional layer for controls/navigation, regular glass protects legibility, and content-layer surfaces should remain standard materials.
- Apple Human Interface Guidelines: <https://developer.apple.com/design/human-interface-guidelines>. The system font, semantic colors, accessible controls, and platform-specific behavior are the baseline.
- Apple Accessibility: <https://developer.apple.com/design/human-interface-guidelines/accessibility>. Dynamic text, contrast, focus, and non-color status cues are required.
- Apple WWDC25 Liquid Glass: <https://developer.apple.com/videos/play/wwdc2025/219/>. Material adapts to its environment and must preserve a clear content/controls hierarchy.
- Material 3 component guidance: <https://m3.material.io/>. The Android implementation uses the same semantic surface vocabulary and 8dp rhythm, but keeps the existing View stack to avoid adding a new UI runtime.
- Mousedroid, an open-source remote input product: <https://github.com/darusc/Mousedroid>. Its connection modes, explicit gesture guide, and separate input modes are useful product patterns for a remote input utility.
- Open Apple HIG skill family used during implementation: <https://github.com/0xKoru/apple-hig-codex-skills> and <https://github.com/s1gmamale1/apple-design-skills>.
- Android liquid material: <https://github.com/QWEA0/Liquid-Glass-Android> (MIT; JitPack `com.github.QWEA0:liquidglass:v2.0.2`). The APK uses its View-based `LiquidGlassView`, API 33+ single-pass AGSL/SDF lens with backdrop capture, refraction, physical dispersion, and edge highlight; older Android versions keep the opaque theme fallback. The touch plane uses the `CLEAR` material so the backdrop remains recognizable instead of becoming a gray slab; no idle animation is scheduled.
- Web liquid lens reference: <https://github.com/PallavAg/liquid-glass-web-react> (MIT). The static client uses its geometry-derived displacement-map approach for the SVG `feDisplacementMap` enhancement and keeps `backdrop-filter` as the cross-browser readable baseline.

## Visual thesis

Quiet graphite utility surfaces with a white touch canvas, one Apple blue action accent, and a restrained glass toolbar that floats above the workspace. The signature is the contrast between a calm, high-contrast trackpad plane and translucent controls that reveal depth without competing with the gesture target.

## Theme matrix

Both clients persist the same theme key (`light-glass` by default) locally. The seven Liquid Glass variants use the QWEA0 lens on Android and the SVG displacement path on Chromium Web; editor/utility themes deliberately use opaque surfaces for predictable contrast.

| Key | Surface | Material | Use |
|---|---|---|---|
| `light-glass` | blue/white/peach sampled backdrop | Clear lens, balanced refraction | Default Apple-style appearance |
| `dark-glass` | graphite/blue sampled backdrop | Clear lens, deeper refraction and adaptive tint | Low-light rooms |
| `ocean-glass` | blue/cyan sampled backdrop | Stronger refraction and cool chromatic edge | Calm, high-clarity glass |
| `graphite-glass` | graphite/teal/rose sampled backdrop | Low-brightness lens with adaptive tint | Dark glass without pure black |
| `sunset-glass` | warm coral/amber sampled backdrop | Soft warm refraction and edge light | Warm rooms and evening use |
| `aurora-glass` | cyan/green/violet sampled backdrop | Cool adaptive tint with stronger edge light | Colorful ambient scenes |
| `custom-glass` | user-selected wallpaper | Highest refraction range with adaptive tint | Personal wallpapers and experiments |
| `tokyo-night`, `nord`, `dracula`, `solarized-dark`, `catppuccin-mocha`, `monokai` | editor-inspired solid surfaces | No blur/refraction | Stable developer themes |
| `classic-light`, `classic-dark` | solid system surfaces | No blur/refraction | Battery and clarity |
| `high-contrast` | black/white boundaries | No transparency or motion reliance | Accessibility fallback |

Liquid Glass is intentionally limited to the touch plane and functional chrome. The sampled backdrop uses quiet static gradient bands; specular response is driven by touch position rather than an idle loop, so the lens is readable without becoming a screensaver.

Touch feedback is a separate, non-interactive visual layer above the lens: active contacts get a restrained colored core, two optical rings, a velocity ribbon and a directional specular streak. The glass frame also brightens its local edge field while a contact is held, then settles on release. On lift, the marker eases out over 280ms so a fast swipe reads as fluid material motion without leaving persistent particles or stealing contrast from the gesture target.

## Tokens

### Shared semantic roles

| Role | Android | Web dark | Use |
| --- | --- | --- | --- |
| Canvas | `#F4F5F7` | `#0B0D12` | Touch surface background |
| Canvas raised | `#FFFFFF` | `#151923` | Touch surface inset / empty state |
| Chrome | `#FFFFFF` at 92% | `rgba(24,27,36,.76)` | Toolbar and settings sheets |
| Primary label | `#111318` | `#F5F7FB` | Headings and active labels |
| Secondary label | `#68707D` | `#AEB6C5` | Descriptions and metadata |
| Separator | `#D8DCE3` | `rgba(255,255,255,.12)` | Grouped rows |
| Accent | `#007AFF` | `#0A84FF` | Connect, selected state, progress |
| Success | `#1E9E5A` | `#30D158` | Connected |
| Warning | `#B86A00` | `#FF9F0A` | Connecting / degraded |
| Danger | `#C9342F` | `#FF453A` | Disconnected / errors |

### Geometry and type

- Spacing: 4dp/4px half steps, 8dp/8px base rhythm, 16 and 24 for section separation.
- Radii: 8 for controls and fields, 12 for grouped surfaces, 16 for dialogs/sheets, 999 only for compact status pills.
- Press profile: controls acknowledge input with a 95ms down state, 0.8% scale plus a half-pixel visual settle, then a 175ms release. Android mirrors the same geometry and adds an asymmetric fast-out/decay pair with 96% alpha instead of the platform's default symmetric easing.
- Deep press bar: use the same 10px/10dp control radius at every configured height, clip progress to that shape, and expose a clear accessible action label because the bar is a custom-drawn control.
- Android touch targets: minimum 48dp for controls. Web touch targets: minimum 44px.
- System font: Android `sans-serif` with `sans-serif-medium` for labels; Web `-apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", Arial, sans-serif`.
- Type hierarchy: app title 20sp/20px, section label 12sp metadata only, body 14-16sp, control label 13-14sp.

## Android information architecture

```text
MainActivity
├── Unified top chrome: connection state + product name + expandable actions
├── Touch surface: TouchPadView + configurable DeepPressBarView
└── Control center: sensitivity, haptic toggle, gesture tests, deep-press settings
    └── Connection sheet: host / port / token / connect
    └── Deep-press sheet: enabled / hold / strength / position / size
    └── Gesture test sheet: grouped test actions
```

The main screen never puts IP, port, token, and six unrelated buttons in one row. Connection is a focused sheet, preserving the touch surface as the primary task.

## Web information architecture

- A unified top toolbar owns only status, the control-center entry, and fullscreen. After connection it collapses to a compact status capsule; sensitivity, haptics, diagnostics, theme, command, and deep-press controls live in a grouped control center and never require horizontal discovery.
- A high-contrast touch plane occupies the full viewport and keeps pointer events uninterrupted. Its contact visualizer is velocity-aware on Android and Web, with short trails and spring-like lift fade.
- There is no persistent bottom dock. The touch plane uses the freed bottom space; mobile widths keep the top toolbar fixed-width with no horizontal scrolling, while the control center is the only vertically scrollable surface.
- Fullscreen keeps a single floating settings button. Escape and the browser fullscreen API remain available; the touch surface keeps its current bounds and material instead of expanding edge-to-edge.
- `prefers-reduced-motion`, `prefers-reduced-transparency`, `prefers-contrast`, and safe-area insets have explicit fallbacks.

## Interaction rules

- Every button has an active press response under 160ms, visible focus, and a 44/48px target.
- Connection state is represented by dot, label, and text status, never by color alone.
- A touch frame is never blocked by a visual overlay. The deep-press bar is the only interactive overlay and remains configurable.
- Sensitivity changes are immediate and persisted locally. Haptic toggle gives one confirmation pulse only when enabled.
- Diagnostic gestures stay in a grouped sheet and keep their existing sender actions.
- Glass appears on the touch plane and chrome only. Reduced transparency uses opaque surfaces. Material animations are opt-in and event-driven: a touch can trigger one finite response, but no surface runs a permanent loop. Fullscreen hides chrome while preserving the touch plane's current bounds and hit area; `fullSensor` allows portrait and landscape layouts.

## Implementation constraints

- Android: keep the existing `TouchPadView`, `DeepPressBarView`, `Haptics`, `UdpSender`, and gesture test runner contracts. Presentation uses a compact connection capsule, a fixed top chrome with a single control-center entry, a dominant touch plane, and a vertically scrollable control-center dialog; alternate material rendering stays in a pointer-transparent `MaterialSurfaceView`.
- Android editor and utility themes must derive their touch-plane background from `ThemePalette` instead of a baked white ceramic gradient; all settings/test sheets use the active palette for labels, controls, and strokes.
- Web: keep the existing binary wire protocol and pointer capture logic. Replace only the shell markup, CSS tokens, visualizer palette, and controls.
- No new network service, no new runtime, and no unverified Apple/private API dependency.

## Verification gates

- Android: `./gradlew test assembleDebug`, install over ADB, launch, inspect portrait and landscape screenshots, verify touch surface, connection sheet, fullscreen, deep-press settings, and gesture tests.
- Web: run the existing server, capture 375px and 1280px screenshots, verify status transitions, pointer capture, fullscreen, haptic toggle, keyboard focus, and reduced-transparency fallback.
- Static: `git diff --check`, Rust workspace tests, and a grep for accidental `transition: all` / missing button labels.

## Phase status

- [x] Research and install Apple HIG / materials / interaction skills.
- [x] Audit current Android and Web presentation shells.
- [x] Lock shared semantic tokens and interaction rules.
- [x] Rebuild Android presentation shell.
- [x] Rebuild Web presentation shell.
- [x] Run device/browser visual verification and update the execution plan.
- [x] Prepare GitHub-facing README and release checklist.

## Verification record

- Android `./gradlew test assembleDebug`: passed.
- APK installed over ADB to `192.168.3.131:34743`; main screen, connection sheet, and deep-press settings were screenshot-checked.
- Web JavaScript syntax check: passed.
- Web screenshots checked at 375x812 and 1280x800 for `/touchpad.html`; `/tester.html` checked at the same sizes.
- The static Python server reports an expected WebSocket 404 because it does not implement `/ws`; a running `companion-net` instance is required for live connection-state verification.
- `git diff --check`: passed.

## macOS settings surfaces

The settings experience now has two clients over one configuration contract:

- `companion-tui` is the terminal fallback for Mac mini deployments where
  System Settings omits the Trackpad pane. Its sidebar follows Apple's three
  groups and its `Companion` section makes virtual-only behavior explicit.
- `macos/TrackpadCompanionSettings` is a macOS 13+ SwiftUI package using
  `NavigationSplitView` and grouped `Form` rows. It uses semantic system colors,
  system typography, standard macOS controls, keyboard-accessible navigation,
  dark-mode adaptation, and a Chinese/English language picker.

Both clients call the Rust `companion-config` helper for reads and atomic TOML
writes. The GUI does not parse TOML or write `defaults` directly. Linux CI can
validate the helper and schema; SwiftUI compilation and VoiceOver/window
restoration checks remain a macOS-host gate.

## Wallpaper and material pipeline

The wallpaper path is deliberately separated from the material path. A selected
image is drawn once as a full-window canvas layer, then receives only user
controlled exposure (`0-100%`), saturation (`60-140%`) and brightness
(`70-130%`). Theme gradients are used only when no image is selected; this
prevents a preset or local image from being double-tinted by the active theme.

The touch surface has an independent opacity control (`55-100%`). Liquid Glass
uses it as the material alpha while solid/editor themes use it as a restrained
surface blend, keeping text and gesture feedback stable instead of fading the
entire application. This follows Apple's material model: transparency is a
surface property, and reduced-transparency environments switch to opaque
surfaces rather than applying an indiscriminate global dimmer.

Android mirrors this pipeline in `MainActivity` with a color-matrix pass for the
wallpaper and a low-strength readability scrim. API 31+ `GpuGlassView` samples
that same scene for refraction; older devices use the same source through the
QWEA0 fallback. The browser uses a pointer-transparent pseudo-element so the
background never intercepts touch input.
