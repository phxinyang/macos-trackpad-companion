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
- Android liquid material: <https://github.com/QWEA0/Liquid-Glass-Android> (MIT; JitPack `com.github.QWEA0:liquidglass:v2.0.2`). The APK uses its View-based `LiquidGlassView`, API 33+ single-pass AGSL/SDF lens with live backdrop capture, refraction, physical dispersion, sensor highlight, and Regular/Clear material variants; older Android versions keep the opaque theme fallback. The default light theme uses the Regular lens with adaptive tint disabled so the bright sampled scene stays neutral and the rim remains visible; dark glass enables adaptive tint for legibility.
- Web liquid lens reference: <https://github.com/PallavAg/liquid-glass-web-react> (MIT). The static client uses its geometry-derived displacement-map approach for the SVG `feDisplacementMap` enhancement and keeps `backdrop-filter` as the cross-browser readable baseline.

## Visual thesis

Quiet graphite utility surfaces with a white touch canvas, one Apple blue action accent, and a restrained glass toolbar that floats above the workspace. The signature is the contrast between a calm, high-contrast trackpad plane and translucent controls that reveal depth without competing with the gesture target.

## Theme matrix

Both clients persist the same theme key (`light-glass` by default) locally.

| Key | Surface | Material | Use |
|---|---|---|---|
| `light-glass` | blue/white/peach sampled backdrop | LiquidGlassView Regular + blur + refraction + dispersion | Default Apple-style appearance |
| `dark-glass` | graphite/blue sampled backdrop | LiquidGlassView + blur + refraction | Low-light rooms |
| `classic-light` | solid system light surfaces | No blur/refraction | Battery and clarity |
| `classic-dark` | solid system dark surfaces | No blur/refraction | OLED-friendly utility mode |
| `high-contrast` | black/white boundaries | No transparency or motion reliance | Accessibility fallback |

Liquid Glass is intentionally limited to the touch plane and functional chrome. The touch plane's sampled backdrop contains quiet gradient bands and moving sheen so the real lens has visual structure to refract; it is not a decorative full-screen blur.

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
- Radii: 8 for fields, 12 for grouped surfaces, 16 for dialogs/sheets, 999 for compact status pills.
- Android touch targets: minimum 48dp for controls. Web touch targets: minimum 44px.
- System font: Android `sans-serif` with `sans-serif-medium` for labels; Web `-apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", Arial, sans-serif`.
- Type hierarchy: app title 20sp/20px, section label 12sp metadata only, body 14-16sp, control label 13-14sp.

## Android information architecture

```text
MainActivity
├── Top app bar: connection state + product name + compact actions
├── Touch surface: TouchPadView + configurable DeepPressBarView
└── Bottom action rail: sensitivity, haptic toggle, gesture tests, deep-press settings
    └── Connection sheet: host / port / token / connect
    └── Deep-press sheet: enabled / hold / strength / position / size
    └── Gesture test sheet: grouped test actions
```

The main screen never puts IP, port, token, and six unrelated buttons in one row. Connection is a focused sheet, preserving the touch surface as the primary task.

## Web information architecture

- A compact top toolbar shows status, a connection hint, diagnostics, haptics, and fullscreen.
- A high-contrast touch plane occupies the full viewport and keeps pointer events uninterrupted.
- A bottom glass action dock exposes sensitivity and settings without hiding the touch plane.
- Fullscreen keeps a single floating settings button. Escape and the browser fullscreen API remain available.
- `prefers-reduced-motion`, `prefers-reduced-transparency`, `prefers-contrast`, and safe-area insets have explicit fallbacks.

## Interaction rules

- Every button has an active press response under 160ms, visible focus, and a 44/48px target.
- Connection state is represented by dot, label, and text status, never by color alone.
- A touch frame is never blocked by a visual overlay. The deep-press bar is the only interactive overlay and remains configurable.
- Sensitivity changes are immediate and persisted locally. Haptic toggle gives one confirmation pulse only when enabled.
- Diagnostic gestures stay in a grouped sheet and keep their existing sender actions.
- Glass appears on the touch plane and chrome only. Reduced transparency uses opaque surfaces. Reduced motion removes decorative transforms.

## Implementation constraints

- Android: keep the existing `TouchPadView`, `DeepPressBarView`, `Haptics`, `UdpSender`, and gesture test runner contracts. Refactor only the presentation shell and dialog styling in `MainActivity`.
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
