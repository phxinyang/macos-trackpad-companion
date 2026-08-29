# Open-Source Reverse-Engineering Sources

Updated 2026-08-29. This file records the public projects used as technical
references for the private macOS gesture compatibility layer. We reimplemented
the relevant behavior in Rust; no source file from these projects is vendored
in this repository.

## Gesture event payloads

## 2026 deep-dive: captures, thresholds, and modern-system limits

This section separates values observed in a real event dump from values chosen
by an open-source recognizer. A number appearing in a reverse-engineering
project is not automatically an Apple preference or an Apple threshold.

### CalfTrail's recorded CGEvent and IOHID bytes

- Sources:
  - <https://raw.githubusercontent.com/calftrail/Touch/master/TouchExtractor/CGEvent%20data%20decode%20notes.txt>
  - <https://raw.githubusercontent.com/calftrail/Touch/master/TouchExtractor/Gesture%20field%20decode%20notes.txt>
  - <https://raw.githubusercontent.com/calftrail/Touch/master/TouchExtractor/TouchExtractorAppDelegate.m>
- This is the strongest public evidence found for an actual trackpad capture,
  rather than a guessed setter. The dump identifies the CGEvent wrapper as
  big-endian field records and the attached IOHID queue as little-endian.
- The decoded gesture fields are:
  - field `110` (`0x6e`): gesture subtype;
  - field `113` (`0x71`): magnification value;
  - field `114` (`0x72`): rotation value;
  - field `115` (`0x73`): swipe direction;
  - field `132` (`0x84`): phase.
- The notes show non-zero captured deltas, including magnification around
  `0.009995 -> 0.007996` and rotation around `0.080432` per observed frame.
  They do not provide a calibrated device-wide curve, a system preference, or
  a complete time-series dataset from which one could infer an Apple gain.
- `TouchExtractorAppDelegate.m` also shows the replay method: it preserves
  event timing, recreates type-29 events, copies fields, moves the event to the
  current pointer location, and posts through the HID tap. This is useful
  evidence for a replay experiment, not proof that every client consumes the
  replay identically.

### CalfTrail's recognizer numbers are heuristics

`TouchSynthesis.m` classifies a two-contact stream with a 10% relative-distance
threshold for magnification and a 5-degree threshold for rotation. It limits a
single magnification delta to `+/-0.025`, accumulates what was sent, and emits
rotation as a per-frame degree delta. These numbers belong to CalfTrail's
`MagicConverter`; the source does not claim that they came from Apple's driver.
The project also supports switching between gesture families while a touch
session is active, whereas current AppKit documentation says scroll and swipe
lock once begun and only magnify/rotate may switch. We therefore retain the
numbers as a useful comparison profile, not as defaults.

### Hammerspoon confirms the synthesis boundary

- Sources: <https://github.com/Hammerspoon/hammerspoon/issues/1434> and
  <https://github.com/Hammerspoon/hammerspoon/pull/2512>.
- The project exposes `beginMagnify`, `endMagnify`, `beginRotate`, and
  `endRotate` helpers using the same type-29/subtype-5/8 and fields 113/114.
- The maintainers explicitly record that some generated gestures initially
  worked only inside Hammerspoon, and that changing from a session tap to a HID
  tap did not change their test result. This is a direct warning against
  treating successful posting or one application's callback as universal
  native-trackpad compatibility.

### Mac Mouse Fix adds timing observations, not a sensitivity curve

- Source: <https://raw.githubusercontent.com/noah-nuebling/mac-mouse-fix/master/Helper/Core/Touch/TouchSimulator.m>.
- Its rotation and magnification helpers use the same private fields. The
  comments around its real-device comparison report roughly 8 ms cadence for
  real pinch events and note that real end events can carry a non-zero delta.
- The modern macOS 27 work in the same repository concerns DockSwipe's attached
  `IOHIDEvent` path. It does not publish a rotation or magnification gain, nor
  does it establish that type-29 transform fields changed on that release.

### Modern macOS 26 evidence: capture the raw source when finger count matters

- Source: <https://github.com/acsandmann/aerospace-swipe/pull/29>.
- On macOS 26.3, the author measured that a session-level gesture event exposed
  at most one `NSTouch` during a four-finger swipe. The fix moved recognition to
  `MTRegisterContactFrameCallback` over every device from
  `MTDeviceCreateList`, with Input Monitoring permission and a live run loop.
- This does not invalidate type-29 transform synthesis, but it does mean a
  recorder based only on `NSEvent.allTouches()` cannot be used to infer the
  original finger geometry or Apple's classification thresholds on current
  systems.

### Other open implementations and what they actually prove

- **TrackpadKit** (<https://github.com/pszypowicz/TrackpadKit>) ships a raw
  `NSTouch` recorder, deterministic JSONL fixtures, and a tunable recognizer.
  Its published defaults include a 60 ms settle interval, 10 device-point
  swipe commit, 8 device-point pinch commit, and a 1.25 dominance margin. The
  fixtures are valuable reproducible input, but this package recognizes swipe
  and pinch; it does not publish an Apple rotation curve.
- **Re9iNee/mac-gesture-events**
  (<https://github.com/Re9iNee/mac-gesture-events>) demonstrates WebKit's
  `gesturestart`/`gesturechange` values (`scale` and `rotation`). It is a
  browser gesture demo, not a macOS CGEvent or MultitouchSupport capture, so it
  is not evidence for WindowServer payload units.
- **slop-desk input audit**
  (<https://github.com/aislopware/slop-desk/blob/dc64b6fa/docs/05-input-window-control.md>)
  reports probe results in which synthetic phased scroll cannot trigger
  browser history gestures and no public constructor exists for universal
  magnify/rotate events. Its product chooses key-equivalent translation for
  pinch and drops rotate. This is an application-compatibility observation,
  not an assertion that private type-29 events never work.
- **dockswipe**
  (<https://github.com/oomol-lab/dockswipe>) independently documents the
  type-29/type-30 private event split and version fragility. It is useful for
  the Dock path, but it contains no transform sensitivity parameter. Its
  macOS 26 recipe also makes the typed-field boundary explicit: type/phase/
  axis/progress/velocity fields use `CGEventSetDoubleValueField`, while field
  135 carries the Float32 progress bit pattern through the integer slot and
  field 136 is the integer inversion flag. It resends the terminal `Ended`
  event after 200 ms (and optionally 500 ms) because WindowServer can drop a
  terminal frame under load.

### Community reports: sensitivity is perceived behavior, not a readable key

- Apple Support's current Trackpad settings page lists "Zoom in or out" and
  "Rotate" as on/off gesture choices and lists a tracking-speed control, but no
  rotation or pinch sensitivity control: <https://support.apple.com/en-sg/guide/mac-help/mchlp1226/mac>.
- An Apple Support Community report says a user found the trackpad too
  sensitive to palm contact and that an older accidental-touch setting appeared
  to be gone in Sonoma: <https://discussions.apple.com/thread/255383759>.
- An ArcSite thread reports pinch zoom feeling overly sensitive, while a
  Parallels thread reports two-finger scrolling being much faster in a VM than
  on native macOS. These are useful UX signals but contain no hidden Apple
  preference or numeric transfer function:
  <https://community.arcsite.com/c/support/trackpad-sensitivity-pinch-zoom>,
  <https://forum.parallels.com/threads/mac-trackpad-gesture-sensitivity.103780/>.
- Third-party preference catalogs expose click thresholds such as
  `FirstClickThreshold = 0/1/2`, and lists commonly include
  `TrackpadPinch`/`TrackpadRotate` as booleans. No catalog or captured plist
  found in this pass exposed a rotation or magnification gain:
  <https://macos-defaults.com/trackpad/firstclickthreshold.html>,
  <https://forums.macrumors.com/threads/complete-list-of-known-defaults-write-commands.2361091/>.

### Consequence for this project

1. The only publicly reproducible Apple-facing transform contract is a
   per-event relative magnification and per-event signed rotation delta with a
   complete began/changed/ended lifecycle. The private fields identify the
   payload; they do not define a user sensitivity setting.
2. Keep `gestures.pinch.gain` and `gestures.rotate.gain` as explicit
   Companion-only compatibility controls, defaulting to `1.0`. Do not map
   `TrackpadPinch` or `TrackpadRotate` to a fabricated numeric gain.
3. Keep the current 1:1 geometric rotation baseline until a target Mac capture
   and application A/B matrix exists. Do not copy CalfTrail's `5 degrees`, `10%`, or
   `+/-0.025` values into the default path without measuring cadence, device
   size, and consumer behavior on the target OS.
4. The next high-value experiment is a macOS recorder that captures, for the
   same physical gesture, raw `MultitouchSupport` frames, type-29 CGEvent
   fields, AppKit `magnifyWithEvent:`/`rotateWithEvent:` callbacks, and the
   target app result. Without all four, a "native sensitivity" claim remains
   unverified.

## Cross-platform touchpad references

These projects are not Apple-driver sources, but they expose useful design
choices for gesture classification, contact quality, and parameter surfaces.
They are included to identify portable invariants and to prevent accidentally
calling a Linux or Windows knob an Apple-native setting.

### Classification and parameter findings

#### libinput: an explicit pinch/rotate contract

- Sources:
  - <https://wayland.freedesktop.org/libinput/doc/1.30.1/gestures.html>
  - <https://raw.githubusercontent.com/jiixyj/libinput/master/src/evdev-mt-touchpad-gestures.c>
  - <https://raw.githubusercontent.com/jiixyj/libinput/master/src/evdev-mt-touchpad.c>
- libinput starts a gesture only once motion is unambiguous, keeps the finger
  count fixed for the gesture lifetime, and ends the gesture when the first
  member lifts. A count change therefore ends/cancels the old stream and may
  start a new stream. This is a stronger, testable version of the lock rule we
  need for native mode.
- A pinch update exposes three independent values: center `dx/dy`, angle delta
  relative to the previous event, and scale relative to the initial spread.
  Pinch and rotate are intentionally concurrent. The docs explicitly say the
  scale is absolute-from-initial while position and angle are previous-frame
  deltas.
- The reference implementation defines a 100 ms gesture-switch timeout and a
  150 ms two-finger-scroll timeout. Before lock, a per-contact direction needs
  roughly 1 mm of normalized travel (scaled by finger count); same-direction
  motion selects scroll/swipe, opposing motion selects pinch, and a stationary
  two-finger pair after the timeout is treated as slow scroll. These are
  libinput heuristics, not Apple thresholds, but they are useful comparison
  points for our 2F lock and landing grace.
- libinput's palm/thumb design is stateful rather than a frame boolean. It uses
  firmware confidence, pressure or contact size, edge zones, keyboard/trackpoint
  activity, and sticky classification. This supports our decision to keep
  confidence in the PTP wire format and to avoid reclassifying a contact on
  every frame.

#### ChromiumOS Gestures: the most complete public recognizer parameter set

- Sources:
  - <https://chromium.googlesource.com/chromiumos/platform/gestures/+/HEAD/src/immediate_interpreter.cc>
  - <https://chromium.googlesource.com/chromiumos/platform/gestures/+/HEAD/include/immediate_interpreter.h>
  - <https://chromium.googlesource.com/chromiumos/platform/gestures/+/HEAD/src/palm_classifying_filter_interpreter.cc>
  - <https://chromium.googlesource.com/chromiumos/platform/gestures/+/HEAD/include/palm_classifying_filter_interpreter.h>
- The constructor publishes concrete defaults for the recognizer. The source
  comments identify the following units: distances are millimetres, timeouts
  are seconds, and pinch scale thresholds are squared ratios. Pressure and
  contact-width values are hardware-normalized values and must not be copied
  between devices without calibration.
- Classification and lock defaults:
  - `Change Timeout = 0.20 s`, `Evaluation Timeout = 0.15 s`,
    `Pinch Evaluation Timeout = 0.10 s`;
  - `Two Finger Scroll Distance Thresh = 1.5 mm`,
    `Two Finger Move Distance Thresh = 7.0 mm`;
  - `Three/Four Finger Swipe Distance Thresh = 1.5 mm`, with a
    `0.2` distance-ratio requirement;
  - `Minimum Movement Direction Detection = 0.003` (normalized movement);
  - `Tap Min Separation = 10.0` (the same device-space coordinate family used
    by the touchpad metrics).
- Pinch-specific defaults:
  - `Pinch Noise Level Squared = 2.0 mm^2`,
    `Pinch Guess Minimum Movement = 2.0 mm`,
    `Pinch Thumb Minimum Movement = 1.41 mm`, and
    `Pinch Certain Minimum Movement = 8.0 mm`;
  - `Inward Pinch Minimum Angle = 0.3` and `Pinch Zoom Maximum Angle = -0.4`
    are cosine-of-angle tests, not degrees;
  - `Pinch Guess Consistent Movement Ratio = 0.4`,
    `Pinch Zoom Minimum Events = 3`, and `Pinch Initial Scale Time Inverse =
    3.33 s^-1`;
  - `Minimum Pinch Scale Resolution Squared = 1.005`, while stationary and
    hysteresis updates use `1.05` after `Stationary Pinch Time = 0.10 s`.
    These are update-resolution gates, not user-facing zoom gains.
- Palm and fat-finger defaults in `PalmClassifyingFilterInterpreter` are:
  `Palm Pressure = 200.0`, `Palm Width = 21.2`, `Multiple Palm Width = 75.0`,
  `Fat Finger Pressure Ratio = 1.4`, `Fat Finger Width Ratio = 1.3`,
  `Fat Finger Min Move Distance = 15.0`, `Tap Exclusion Border Width = 8.0`,
  `Palm Edge Zone Width = 14.0`, `Palm Edge Zone Min Point Speed = 100.0`,
  `Palm Eval Timeout = 0.10 s`, `Palm Stationary Time = 2.0 s`,
  `Palm Stationary Distance = 4.0 mm`, and `Palm Split Maximum Distance =
  4.0`. A contact can be reconsidered as a pointing finger only after the
  movement and lifetime tests pass; this is intentionally sticky state, not a
  per-frame palm bit.
- The same source exposes quality-filter defaults useful for a compatibility
  profile: a second-order Butterworth IIR (`b0=0.0674552738890719`,
  `b1=0.134910547778144`, `b2=0.0674552738890719`,
  `a1=-1.1429805025399`, `a2=0.412801598096189`), IIR distance threshold
  `10`, stationary wiggle energy/hysteresis `0.012/0.006`, lookahead quick
  move threshold `3.0`, and liftoff speed factor `5.0`.
- These values are ChromiumOS implementation defaults, not Apple values. They
  are useful for an opt-in `chromium-profile` or for test fixtures, but the
  native macOS default must continue to use the Companion's calibrated
  geometry and target-device sampling.

#### Synaptics Xorg: explicit pressure, hysteresis, and coasting knobs

- Sources:
  - <https://man.archlinux.org/man/synaptics.4.en>
  - <https://wiki.archlinux.org/title/Touchpad_Synaptics>
  - <https://gist.github.com/tuurep/b21126b93f04a498fb587a27ec9c9e00>
- The driver documents a concrete parameter surface even though it is not a
  macOS implementation: `FingerLow`/`FingerHigh` define release/touch
  pressure, `MaxTapTime` and `MaxDoubleTapTime` are millisecond windows,
  `MaxTapMove` is the tap travel limit, `Vert/HorizScrollDelta` define scroll
  distance and sign, and `MinSpeed`/`MaxSpeed`/`AccelFactor` define the pointer
  speed curve.
- Palm rejection has two independent dimensions: `PalmMinWidth` and
  `PalmMinZ` (minimum contact width and pressure). A common observed Xorg
  configuration is `10` and `200`, but these are device/profile values, not a
  Synaptics-wide physical standard. The Arch manual also documents the
  default coasting trigger of `20` scroll events/s and friction of `50`
  scroll events/s^2, plus `HorizHysteresis`/`VertHysteresis` defaulting to
  `0.5%` of the pad diagonal when the device does not advertise fuzz.
- `LockedDragTimeout` is explicitly in milliseconds; `SoftButtonAreas` and
  `Area*Edge` are coordinate/percentage regions. These concepts map cleanly
  to Companion policy knobs, but the raw pressure, width, and coordinate
  ranges must stay device-relative.

#### Fusuma: user-facing sensitivity is a multiplier, not a physical unit

- Source: <https://github.com/iberianpig/fusuma/blob/master/README.md>
- `threshold` defaults to `1.0`; setting `0.5` shortens the required
  swipe/pinch/hold length by half. `interval` also defaults to `1.0`; setting
  `0.5` halves the delay before another gesture can be recognized. Direction-
  specific values override the gesture-level value, which overrides the root
  value, which overrides the default.
- Fusuma's `pinch` and `rotate` sections expose begin/update/end events, but
  the project does not define an Apple-compatible rotation or magnification
  gain. Its values are therefore appropriate as UI semantics for a
  Companion profile, not as a source for macOS native sensitivity.

#### Windows Precision Touchpad: richer contact metadata and real settings

- Sources:
  - <https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-windows-precision-touchpad-collection>
  - <https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-tuning-guidelines>
  - <https://github.com/imbushuo/mac-precision-touchpad>
- The PTP input report requires Contact ID, X/Y, Tip, and Confidence. Width,
  Height, and Pressure are optional but recommended. Microsoft says the device
  should forward all contacts and mark low-confidence contacts instead of
  hiding them in firmware. This is directly relevant to remote palm handling:
  our current protocol carries `confidence`, but not width/height/pressure.
- PTP devices advertise a contact-count maximum (normally 3-5). If a frame
  exceeds that maximum, Windows discards the entire frame; new contacts should
  also be suppressed for the lifetime of the overflowing contact. This is a
  useful transport-side rule for future Android/virtual-pad implementations.
- Windows exposes actual user/OEM tuning surfaces that macOS does not: `CursorSpeed`
  (1-20), `ClickForceSensitivity` (0-100 on newer Windows 11), and haptic
  `FeedbackIntensity` (0-100). The official guide still does not expose a
  rotation or pinch gain. The existence of these Windows values must not be
  used to infer equivalent Apple plist keys.
- `imbushuo/mac-precision-touchpad` decodes Apple MacBook/Magic Trackpad HID
  reports into the Windows PTP model over USB, SPI, and Bluetooth. Its roadmap
  calls out input-sensitivity configuration and gesture refinement, but it does
  not publish Apple's internal transform curve. It is most useful here as a
  device/report compatibility reference, not as a macOS behavior oracle.

#### Vendor/driver palm evidence

- The VoodooRMI discussion at
  <https://github.com/VoodooSMBus/VoodooRMI/issues/92> describes using contact
  size/pressure to guess which member of a four-finger cluster is the thumb.
  It is an implementation report, not a stable specification, but it reinforces
  that finger-count-only classification is fragile on clickpads.
- `OpenMultitouchSupport` (<https://github.com/Kyome22/OpenMultitouchSupport>)
  exposes global raw frames on macOS 15+ with id, position, capacitance,
  pressure, contact axes, finger angle, density, state, and timestamp. The
  library is unsandboxed and limited to the default device. It is a practical
  blueprint for the recorder proposed above, while its private struct layout
  remains version-sensitive.

#### Cross-platform mapping decisions

1. Keep the transform core platform-neutral: per-frame signed angle delta,
   absolute scale from a gesture baseline, and optional center translation.
   The macOS output adapter converts these into AppKit-compatible private
   fields; Linux/Windows values are not copied as gains.
2. Extend the remote contact model only when a sender can provide trustworthy
   data. `confidence` is already available. Width/height/pressure should be
   optional protocol fields or a versioned extension, not fabricated from
   screen coordinates.
3. Treat timing numbers as profiles. The current 2-frame observation and
   3-degree rotation lock are Companion choices; a future `libinput-profile`
   could expose 100/150 ms settling for comparison without changing native
   defaults.
4. Add a recorder fixture that preserves contact confidence/size/pressure when
   available. This enables palm/thumbnail analysis without making those fields
   prerequisites for browser clients.

### Concrete parameter quick reference

The table below is intentionally explicit about provenance. `Apple public` means
the value is documented by Apple as a user preference or event contract;
`OSS default` means a recognizer's compiled-in heuristic; `sample profile` means
an observed device/configuration value. Only the first category can be called a
macOS-native setting.

| Platform / source | Parameter | Value | Unit / semantics | Provenance | Companion use |
|---|---|---:|---|---|---|
| macOS Apple settings | `TrackpadPinch`, `TrackpadRotate` | `0/1` | enable flags only | Apple public | map to enable/disable; no gain mapping |
| macOS Apple events | magnification / rotation | relative per-frame delta | AppKit-compatible event semantics | Apple public contract | keep geometric delta and full phase lifecycle |
| CalfTrail `TouchSynthesis` | magnify classify threshold | `10%` | relative distance change | OSS heuristic | comparison profile only |
| CalfTrail `TouchSynthesis` | rotate classify threshold | `5` | degrees | OSS heuristic | comparison profile only |
| CalfTrail `TouchSynthesis` | magnify frame clamp | `+/-0.025` | per-frame magnification | OSS heuristic | safety test fixture, not default gain |
| libinput | gesture switch timeout | `100` | ms | OSS default | optional `libinput` timing profile |
| libinput | 2F scroll timeout | `150` | ms | OSS default | optional settling profile |
| libinput | direction intent travel | `~1` | mm x (`finger_count - 1`) | OSS heuristic | classifier comparison, device-normalized |
| ChromiumOS | `Pinch Evaluation Timeout` | `0.10` | s | OSS default | opt-in classifier profile |
| ChromiumOS | `Pinch Guess Minimum Movement` | `2.0` | mm | OSS default | opt-in classifier profile |
| ChromiumOS | `Pinch Certain Minimum Movement` | `8.0` | mm | OSS default | opt-in lock profile |
| ChromiumOS | `Pinch Zoom Minimum Events` | `3` | input frames | OSS default | avoid one-frame zoom spikes |
| ChromiumOS | pinch update resolution | `1.005` | squared scale ratio | OSS default | transform noise gate |
| ChromiumOS | stationary pinch resolution / time | `1.05` / `0.10` | squared ratio / s | OSS default | stationary hysteresis profile |
| ChromiumOS | pinch angle tests | `0.3` / `-0.4` | cosine thresholds | OSS default | do not convert to degrees blindly |
| ChromiumOS | 2F scroll distance | `1.5` | mm | OSS default | scroll-vs-transform comparison |
| ChromiumOS | palm pressure / width | `200.0` / `21.2` | normalized device units | OSS default | only with calibrated pressure/area |
| ChromiumOS | palm edge zone / eval timeout | `14.0` / `0.10` | device units / s | OSS default | optional palm profile |
| ChromiumOS | palm stationary time / distance | `2.0` / `4.0` | s / mm | OSS default | sticky palm reconsideration |
| ChromiumOS | IIR Butterworth coefficients | `b0=.0674553`, `b1=.1349105`, `b2=.0674553`, `a1=-1.1429805`, `a2=.4128016` | 2nd-order low-pass | OSS default | experimental filter fixture only |
| Synaptics Xorg | `PalmMinWidth` / `PalmMinZ` | `10` / `200` | width / pressure units | sample profile | do not copy without sender calibration |
| Synaptics Xorg | hysteresis | `0.5%` | pad diagonal (fallback) | OSS documented default | useful normalized motion dead zone |
| Synaptics Xorg | coasting speed / friction | `20` / `50` | scroll/s / scroll/s^2 | OSS documented default | compare inertia shape, not macOS value |
| Fusuma | `threshold` / `interval` | `1.0` / `1.0` | length and repeat multipliers | OSS user-facing default | expose as profile multipliers if needed |
| Windows PTP | `CursorSpeed` | `1..20` (default `10`) | scalar | Microsoft public | map to cursor sensitivity UI |
| Windows PTP | `AAPThreshold` | `0..4` (default `2`) | accidental-activation suppression | Microsoft public | map to keyboard palm suppression |
| Windows PTP | `ClickForceSensitivity` | `0..100` (default `50`) | percent | Microsoft public | map only to haptic-capable sender |
| Windows PTP | `FeedbackIntensity` | `0..100` (default `50`) | percent | Microsoft public | map to optional haptic strength |

No source in this pass exposes an Apple rotation gain, pinch gain, or a public
Apple threshold in degrees/mm. `gestures.pinch.gain` and `gestures.rotate.gain`
therefore remain Companion-only controls until a recorder captures the same
physical gesture at the target macOS version and verifies the consuming app.

## Existing Apple-source profiles

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
  must be revalidated on each macOS major release. For a macOS 26 carried
  mouse drag, the companion uses the SymbolicHotKey path described by PR
  #1875; standalone swipes can still use the legacy continuous stream.
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

## Web Liquid Glass performance references

### naughtyduk/liquidGL

- Source: <https://github.com/naughtyduk/liquidGL>
- Findings: a shared WebGL canvas, bounded snapshot `resolution`, one snapshot
  bounding-box read per frame, and dirty tracking for dynamic elements. Its
  README warns that `shadow`, `specular`, and tilt add render work and that
  large snapshot regions increase texture memory.
- Use in companion: keep the displacement map static when geometry is static,
  update only interaction state, and bound the visual backing store instead of
  reducing the optical layers.
- License: repository README presents the package as open source; review the
  repository license before copying implementation code.

### PallavAg/liquid-glass-web-react

- Source: <https://github.com/PallavAg/liquid-glass-web-react>
- Findings: MIT-licensed SVG `feDisplacementMap` engine; geometry map is
  regenerated only when lens shape changes, while movement updates filter
  subregions in place. The map is a small PNG and neutral outside the lens.
- Use in companion: generate the 128px map once, reuse one data URL, and keep
  pointer movement in CSS custom properties.
- License: MIT.

### ybouane/liquidglass

- Source: <https://github.com/ybouane/liquidglass>
- Findings: WebGL renderer crops the scene to each glass panel, tracks dirty
  elements, and skips the shader pipeline for glass panels whose sampled
  regions did not change; `data-dynamic` is explicitly an opt-in expensive
  path. Each instance owns a WebGL context.
- Use in companion: avoid a permanent animation loop, keep one glass region,
  and mark only the touch surface as active during interaction.
- License: repository metadata did not expose an SPDX license on 2026-08-29.

### CSS backdrop-filter field reports

- Sources: <https://github.com/shadcn-ui/ui/issues/327> and
  <https://www.joshwcomeau.com/css/backdrop-filter/>
- Findings: large/overlapping `backdrop-filter` surfaces can dominate GPU
  painting; constraining the filter element, using masks for the sampled
  region, and keeping hidden sheets out of paint reduces work. The shadcn
  issue is a field report rather than a browser specification.
- Use in companion: `contain: paint` on the pad, interaction-only
  `will-change`, and `content-visibility: hidden` for closed sheets; maintain
  readable blur fallbacks for browsers with unreliable SVG backdrop filters.

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
