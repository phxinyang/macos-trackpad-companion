# Open-Source Reverse-Engineering Sources

Updated 2026-08-29. This file records the public projects used as technical
references for the private macOS gesture compatibility layer. We reimplemented
the relevant behavior in Rust; no source file from these projects is vendored
in this repository.

## Gesture event payloads

### CalfTrail Touch

- Source: <https://github.com/calftrail/Touch>
- Core files: `TouchSynthesis/TouchSynthesis.m`,
  `TouchSynthesis/TouchEvents.c`, `TouchSynthesis/TouchEvents.h`.
- Findings:
  - rotate subtype `0x05`, magnify subtype `0x08`, swipe subtype `0x10`;
  - phase values are IOHID `Began=1`, `Changed=2`, `Ended=4`, `Cancelled=8`;
  - magnification is written to serialized field `0x71` (CGEvent field 113);
  - rotation is written to serialized field `0x72` (CGEvent field 114);
  - the event contains a parent digitizer collection, child digitizer events,
    and a vendor token with the device id;
  - the sample recognizer uses a 10% magnification and 5 degree rotation
    classification threshold, and limits a magnification frame to `+/-0.025`.
- Use in companion: field ids, relative delta semantics, phase lifecycle, and
  the serialized IOHID queue shape. The current implementation intentionally
  emits a parent-only payload and must not be described as a raw Apple stream.
- License: repository metadata did not expose an SPDX license on 2026-08-29.
  Treat the implementation as reference material until its license is checked
  before any source-level reuse.

### Hammerspoon

- Source: <https://github.com/Hammerspoon/hammerspoon/pull/2512>
- Findings: `newGesture()` exposes begin/end magnify, rotate, swipe, and smart
  magnify; rotation is degrees, positive counter-clockwise and negative
  clockwise. The pull request records that some synthetic gestures initially
  worked only inside Hammerspoon and that HID/session tap changes were not a
  universal compatibility proof.
- Use in companion: public API naming and sign convention only.
- License: MIT.

### Mac Mouse Fix

- Sources:
  - <https://raw.githubusercontent.com/noah-nuebling/mac-mouse-fix/master/Helper/Core/Touch/TouchSimulator.m>
  - <https://raw.githubusercontent.com/noah-nuebling/mac-mouse-fix/master/Tests/FixDockSwipes.m>
- Findings:
  - magnify uses gesture type 29, subtype 8, field 113, and phase field 132;
  - rotate uses gesture type 29, subtype 5, field 114, and phase field 132;
  - Smart Zoom uses subtype 22;
  - newer DockSwipe handling attaches a private `IOHIDEvent` through
    `SLEventSetIOHIDEvent`; end events may include a velocity child event;
  - real DockSwipe captures can run near 8 ms cadence and may carry a non-zero
    end delta.
- Use in companion: Smart Zoom subtype, DockSwipe version split, progress and
  velocity handling. Ownership and opaque field offsets remain versioned and
  must be revalidated on each macOS major release.
- License: repository reports `Other`/MMF License metadata. Do not copy source
  without reviewing that license.

## Raw MultitouchSupport access

### OpenMultitouchSupport

- Source: <https://github.com/Kyome22/OpenMultitouchSupport>
- Findings: private framework access on macOS 15+, unsandboxed operation, and
  raw id, position, capacitance, pressure, axis, angle, density, timestamp, and
  hover/touch lifecycle states.
- Use in companion: target for a future macOS recorder and capability report;
  these fields are not currently present in the network protocol.
- License: MIT.

### mhuusko5/M5MultitouchSupport

- Source: <https://github.com/mhuusko5/M5MultitouchSupport>
- Findings: Objective-C listener wrapper for global trackpad and Magic Mouse
  frames, including normalized position, velocity, axis, angle, and contact
  size.
- Use in companion: alternate recorder/reference for device enumeration and
  callback threading.
- License: MIT.

### mactic

- Source: <https://github.com/MatMercer/mactic/blob/main/docs/implementation.md>
- Findings: `dlopen/dlsym` avoids arm64e PAC failures seen with direct private
  function declarations; modern framework code is resolved from the dyld shared
  cache; a 96-byte `MTTouch` layout and device-id offset 64 were measured on a
  specific M3/Sequoia machine; `MTActuatorActuate` provides haptic waveforms.
- Use in companion: recorder/haptic feasibility study only. The struct layout
  and offset are empirical, not a stable ABI.
- License: repository metadata did not expose an SPDX license on 2026-08-29.

## Gesture conflict and product behavior

### Trident

- Source: <https://github.com/cyanyux/trident>
- Findings: private raw touch recognition, scoped click-suppression event tap,
  post-gesture tails, cursor freeze during Cmd-Tab HUD, palm rejection, and
  four-finger-tail quarantine. It requires changing macOS's native three-finger
  Spaces gesture to four fingers to avoid a conflict.
- Use in companion: informs stray-click suppression and explicit conflict
  messaging. It does not prove that a remote phone can participate in the same
  WindowServer gesture stream.
- License: MIT.

### LinearSwipe

- Source: <https://github.com/ChilledEther/LinearSwipe>
- Findings: gesture type 29 tap, `NSEvent.allTouches()`, same-direction velocity
  gating, a short App Switcher reveal delay, and an explicit requirement to
  disable or reassign three-finger Spaces. Its README records a known case where
  a three-finger swipe is consumed as content scrolling.
- Use in companion: directional unanimity and delayed confirmation are useful
  classifier ideas, not native thresholds.
- License: repository metadata did not expose an SPDX license on 2026-08-29.

## Implementation policy

1. Use public Apple semantics where available: relative magnification and
   rotation deltas plus complete phase lifecycles.
2. Keep private payloads behind versioned macOS code paths and fail closed when
   an attachment or setter is unavailable.
3. Keep `pinch.gain` and `rotate.gain` as companion-only controls with default
   `1.0`; Apple does not expose equivalent sensitivity sliders.
4. Do not copy private ABI offsets, Objective-C ownership assumptions, or
   GPL/MMF/unknown-license source into the repository without a separate
   license decision.
5. Every new payload change requires a macOS recorder result from the target
   system and an application matrix covering Preview, Photos, Safari, Maps, and
   Figma.
