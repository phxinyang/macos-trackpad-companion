# macOS 触控板原生体验执行规划书

## 0. 执行状态

- 规划版本：2026-08-28
- 目标：把项目从“能发送类似 macOS 的事件”推进到“边界诚实、协议稳定、可回归、可验证的 native-like 体验”。
- 当前基线：`2da296f`，工作树在规划开始时干净，`main` 比 `origin/main` 超前 23 个提交。
- 执行方式：先完成当前 Linux/Android 环境可验证的代码与契约修复；依赖 macOS、WindowServer、Dock 或真实触控板的项目单独保留为真机验收门槛。
- 回滚：每个阶段保持独立提交边界；回滚时只需反向该阶段的文件改动，不触碰用户配置和运行数据。

## 1. 产品边界与成功标准

### 1.1 明确目标

本项目的短期目标是：

1. 光标、点击、滚动在应用层表现稳定，并正确处理 phase、momentum、断链和多客户端切换。
2. pinch、rotate、DockSwipe 在明确的 macOS 版本和应用矩阵中达到可重复的 native-like 行为。
3. Android、浏览器和 HID 输入共享同一套可重放的 gesture engine，而不是各自维护不同语义。
4. 所有“原生”声明都区分为：事件字段正确、应用兼容、系统手势兼容、物理手感对齐、真机已验证。

### 1.2 明确不承诺

- 用户态 `CGEventPost` 不承诺等同于 Apple 的原始 MultitouchSupport/PTP 输入流。
- 未经目标 macOS 版本真机验证，不把 Preview、Photos、Maps、Mission Control 或 Spaces 写成“完全对齐”。
- 不把经验加速曲线写成 Apple 官方参数。
- 不把私有 ABI 当作稳定公共 API；每个私有路径都必须有版本探测、失败降级和回归记录。

### 1.3 完成定义

一个阶段只有同时满足以下条件才算完成：

- 代码、配置、README、parity matrix 的契约一致。
- 至少有一个非空、非跳过的自动化测试覆盖新增行为。
- 相关边界失败时有明确的取消、降级或拒绝行为。
- 对 macOS 专属行为，附带真实设备/应用/系统版本的验收记录；没有设备时标记为“待真机”，不伪造完成。

## 2. 阶段计划

### 阶段 A：输入时序与网络安全（本轮执行）

目标：避免乱序、重启和 modifier 组合把 gesture engine 推入错误状态。

文件：`src/net.rs`、`src/scan_clock.rs`、`src/output.rs`、`src/gesture.rs`。

- [x] A1. UDP 迟到帧不再进入 `ScanTimeClock`；保留有意的 lift 重传。
- [x] A2. 同一 UDP endpoint 的 sender restart 触发 clock/session reset。
- [x] A3. Cmd/Ctrl zoom stream 在一个 touch session 内保持一致，结束时不得启动 scroll inertia。
- [x] A4. 修复 Pan→pinch/rotate 的 `frame_rot` 取值顺序，并收紧动态转场以过滤尾指噪声。
- [x] A5. 为 A1-A4 增加单元测试或可观察的状态测试。

验收：`cargo test --workspace`、`cargo check --all-targets`；新增测试必须覆盖“乱序帧 + scan time”和“modifier + lift”。

### 阶段 B：配置与文档契约（本轮执行）

目标：让运行时行为和公开配置真实一致，删除无法使用的完成声明。

文件：`src/config.rs`、`src/boot.rs`、`README.md`、`docs/native-parity-matrix.md`、`docs/engine-backlog.md`。

- [x] B1. 增加 `gestures.press_and_hold_drag.enable`，默认关闭以保持当前行为并符合普通 macOS 默认体验。
- [x] B2. `boot::gesture_options` 读取该设置，不再硬编码 `false`。
- [x] B3. 修正 swipe 配置路径、`release_delay_ms` 和 press-and-hold 的文档矛盾。
- [x] B4. 移除不存在的 `--no-private-gestures` CLI 声明。
- [x] B5. 将“已完全对齐”改成带验证等级的表述。

验收：配置解析测试、README/schema 搜索一致性检查、全量 Rust 测试。

### 阶段 C：PTP/HID 兼容性（需实现 + 真实 descriptor fixtures）

目标：要么诚实收窄产品范围，要么真正支持 descriptor-defined PTP。

文件：`src/descriptor.rs`、`src/report.rs`、`src/hid.rs`、`docs/wire-protocol.md`。

- [x] C1. 保留字段 bit offset/width，并让 decoder 按 descriptor 位域读取 contact、scan time、count 和 button。
- [x] C2. 按 descriptor 发现 Input Mode Feature Report ID，不把固定值当成 universal ID；缺少该 feature 时 fail-closed。
- [ ] C3. 增加 Microsoft parallel、single-finger hybrid、two-finger hybrid fixtures。
- [ ] C4. 明确 Contact Count=0 但仍携带 contact 数据时的聚合规则。
- [x] C5. 保留 6-byte reference profile，同时说明 decoder 已支持 descriptor-defined bit-packed fields；hybrid/parallel 仍不宣称支持。

验收：每个 fixture 都有 decode 正例、截断、bit-packed 和 hybrid 负例；真实 HID 设备至少记录 VID/PID、descriptor、macOS 版本和报告样本。

### 阶段 D：事件输出和私有 ABI（需 macOS 真机）

目标：验证应用收到的事件，而不是只验证 recorder 中出现了调用。

- [ ] D1. pinch/rotate payload 与 CalfTrail/TouchSimulator 逐字段比对，决定是否嵌入 child digitizer events。
- [ ] D2. DockSwipe 按 macOS major version 验证 `SLEventSetIOHIDEvent` ownership、timestamp、phase、progress、velocity。
- [ ] D3. Smart Zoom 只保留一条经过验证的事件路径。
- [ ] D4. 建立 Preview、Photos、Safari、Maps、Figma、Mission Control、Spaces 验收矩阵。
- [ ] D5. 记录私有路径失效时的 fail-closed 行为。

验收：每个应用至少完成 tap/hold/lift、pinch、rotate、cancel、反向和连续两次 gesture；注明系统版本、架构和权限。

### 阶段 E：输入采集与长期原生路线（研究/决策）

- [ ] E1. 评估 raw `MultitouchSupport`：外接设备枚举、权限、run loop、palm rejection、睡眠唤醒和 Universal Control。
- [ ] E2. 评估虚拟 HID digitizer：驱动签名、系统接管、分发和回滚成本。
- [ ] E3. 只有在 E1/E2 完成成本评估后，决定是否追求真正的 Apple trackpad input stream。

### 阶段 F：macOS 系统设置同步（本轮执行）

目标：让用户在 macOS 系统设置中已经选择的触控板行为成为 companion 的默认策略，同时保留 TOML 对单个字段的明确覆盖。同步失败、缺少 key 或遇到未知枚举时继续使用项目默认值，绝不能阻止启动。

文件：`src/config.rs`、`src/macos_preferences.rs`、`src/boot.rs`、`src/gesture.rs`、`src/output.rs`、`src/output_portable.rs`、`src/main.rs`、`src/bin/companion_net.rs`、`README.md`、`docs/native-parity-matrix.md`、`docs/engine-backlog.md`。

#### Building

- 启动时通过 Core Foundation `CFPreferencesCopyValue` 读取 `com.apple.AppleMultitouchTrackpad`，以 `com.apple.driver.AppleBluetoothMultitouch.trackpad` 作为缺失 key 的回退域，并读取 `.GlobalPreferences` 的 `com.apple.swipescrolldirection`。
- 保存一个可审计的 `RawTrackpadPreferences` 快照：记录已知 key 的原始整数值、来源域、冲突和无法映射字段；日志只输出 key/value，不输出用户数据。
- 将高确定性设置映射为 `NormalizedTrackpadPolicy`，再合并到 `Config`。合并优先级固定为：`显式 TOML 字段 > macOS 设置 > Rust 默认值`。
- 第一阶段实际驱动：轻点点击、双指右键、自然滚动、滚动开关、滚动惯性、横向滚动、捏合、旋转、智能缩放、三指查词、三指拖移、拖移锁定、单指轻点拖移、右边缘通知中心、四指水平/垂直轻扫，以及 `HIDScrollZoomModifierMask`。
- 非布尔或只影响 Apple 驱动/硬件的字段仍被探测并报告为 `unsupported`，包括压力阈值、palm/resting、Force Touch、五指捏合和 USB 鼠标联动；不把它们伪装成已应用。`ActuateDetents` 作为唯一高确定性触觉开关映射到 `macos.haptic_feedback`。
- `HIDScrollZoomModifierMask` 解析为 Quartz modifier mask；值为未知位时保留已知位并记录告警，值缺失时继续使用 Cmd/Ctrl 兼容默认。
- macOS 输出实现使用 `NSHapticFeedbackManager.defaultPerformer()`：点击使用 Generic，拖拽接合使用 Alignment，系统手势确认使用 LevelChange。反馈是设备感知的确认提示，不构造 Force Touch 压力或私有触点事件。

#### Not building

- 不写回 macOS 偏好，不修改用户系统设置，不依赖 `defaults` 命令作为运行时 API。
- 不宣称四指 DockSwipe、Launchpad、Mission Control、Force Touch 等私有或系统级行为已经因为读取设置而获得原生输入流。
- 不在 Linux/Android 上引入 Core Foundation；portable 构建继续使用现有默认策略和测试 fake。
- 不承诺在没有 Taptic Engine 的 Mac、外接普通鼠标或远端手机上产生物理震动；这些设备由系统 performer 安静降级。
- 不做设置变更的实时热更新；本阶段只在进程启动时读取一次，实时刷新留给后续真机验证。

#### 参数映射与优先级

| macOS key | companion 行为 | 映射规则 |
|---|---|---|
| `Clicking` | `GestureOptions.tap_to_click` | 仅 `0/1` 有效；关闭时不产生手指 tap 左键 |
| `TrackpadRightClick` | `GestureOptions.secondary_click` | 仅 `0/1` 有效；关闭时两指 tap 不产生右键 |
| `TrackpadScroll` | `GestureOptions.scroll_enabled` | 关闭时 2F 平移不发滚动事件 |
| `TrackpadHorizScroll` | `output::Config.horizontal_scroll` | 关闭时丢弃横向滚动分量，保留纵向滚动 |
| `TrackpadMomentumScroll` | `output::Config.momentum_scroll` | 关闭时 lift 不启动 inertia |
| `TrackpadPinch` / `TrackpadRotate` | `output::Config.pinch/rotate` | 作为 On/Off policy，仍支持 TOML app filter |
| `TrackpadTwoFingerDoubleTapGesture` | `GestureOptions.smart_zoom` | 值 `1` 开启双指智能缩放 |
| `TrackpadThreeFingerTapGesture` | `GestureOptions.dictionary_lookup` | 值 `2`（及兼容的非零启用值）开启查词 |
| `TrackpadThreeFingerDrag` | `GestureOptions.three_finger_drag` | 值 `1` 为三指拖移，`0` 为三指轻扫 |
| `Dragging` | `GestureOptions.one_finger_tap_drag` | 辅助功能拖移开关；只影响单指双击拖移 |
| `DragLock` | `GestureOptions.release_delay_ms` | 开启使用 500ms，关闭使用 0ms；明确 TOML `release_delay_ms` 优先 |
| `TrackpadTwoFingerFromRightEdgeSwipeGesture` | `GestureOptions.right_edge_swipe` | 非零启用右缘通知中心动作 |
| `TrackpadFourFingerHoriz/VertSwipeGesture` | horizontal/vertical swipe policy | `0` 关闭对应轴，`2` 开启；未知值只记录并保留配置 |
| `HIDScrollZoomModifierMask` | `output::Config.modifier_zoom_mask` | 使用系统 mask 锁定整个 scroll session 的 zoom 路由 |
| `ActuateDetents` | `macos.haptic_feedback` | `1` 开启语义触觉，`0` 关闭；`auto` 模式由系统值填充 |
| `com.apple.swipescrolldirection` | `output::Config.natural_scroll` | 全局值优先于触控板域中的历史别名 |

#### Verification

- 单测覆盖：缺少 key、主域/回退域冲突、未知枚举、modifier mask、TOML 覆盖系统值，以及每个关闭开关的事件抑制。
- macOS 真机启动日志记录系统版本、来源域、应用字段和 unsupported 列表；`scripts/diagnose-mac.sh` 输出与读取层使用同一组 key。
- Linux 上运行 `cargo test --workspace`、`cargo check --all-targets`；macOS 上额外验证 Preview、Photos、Safari、Maps、Mission Control、Spaces 的 tap/scroll/inertia/pinch/rotate/swipe。

最脆弱假设：`CFPreferences` 返回的触控板域仍代表当前活跃设备。若新版本 macOS 改用未覆盖的 domain 或枚举，读取层会保留默认/显式配置并在日志标记 `unsupported`，不会把错误值传播到手势引擎。

## 3. 调研与审查结论

检索记录：以 Ketch 的 `keenable`/Exa 联合搜索发现来源，再用 Ketch scrape 抓取 Apple、Microsoft、GitHub 原文。Exa 有一次限流，Grok 有 provider fallback；这些失败结果未作为事实依据，关键结论只来自已抓取的官方文档、源码和 PR 内容。

### 3.1 Apple 官方手势语义

Apple 的 [Handling Trackpad Events](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/EventOverview/HandlingTouchEvents/HandlingTouchEvents.html) 明确：magnify 和 rotate 是相对事件，每次事件只携带相对上一次同类事件的增量；scroll/swipe 一旦开始会锁定到该手势直到结束；scroll wheel 还分别拥有 `phase` 与 `momentumPhase`。因此本项目可以声称“AppKit 字段/相对增量兼容”，但不能把自定义加速曲线、私有 CGEvent payload 或滚动中转场称为 Apple 原生输入流。

### 3.2 Microsoft PTP/HID 规范

Microsoft 的 [Windows Precision Touchpad Collection](https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-windows-precision-touchpad-collection) 和 [Buttons, Report Level Usages](https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-buttons-report-level-usages) 规定了 Contact ID、X/Y、Tip、Confidence、Scan Time、Contact Count，以及 parallel/hybrid 报告的聚合规则。尤其是 hybrid 模式：同一帧后续报告的 Contact Count 可为 0，但 Scan Time 必须相同，主报告的 Contact Count 表示整帧触点总数。该规范不保证固定 byte layout，也不保证 Input Mode Feature Report 永远使用 `0x08`。

### 3.3 逆向实现与开源项目

- [calftrail/Touch](https://github.com/calftrail/Touch) 的 `MultitouchSupport.h` 给出 raw `MTTouch` 字段：finger ID、state、normalized position、velocity、angle、major/minor axis、density；这是采集真实 Apple 触点的参考，不是 PTP parser 的替代品。
- [Mac Mouse Fix FixDockSwipes.m](https://raw.githubusercontent.com/noah-nuebling/mac-mouse-fix/master/Tests/FixDockSwipes.m) 通过 `CGEventCopyIOHIDEvent` 检查 DockSwipe 的 HIDEvent 字段，并在结束阶段携带 velocity child；源码明确写出 `SLEventSetIOHIDEvent` 比 `CGEventSetIOHIDEvent` 可靠。
- [Mac Mouse Fix CGEventHIDEventBridge.m](https://raw.githubusercontent.com/noah-nuebling/mac-mouse-fix/master/Shared/IOKit/CGEventHIDEventBridge.m) 保留了旧版 `0x18/0xd0` opaque offset 写入作为历史实验，不能视为稳定 ABI。
- 社区对 ownership 仍有分歧：[PR #1920](https://github.com/noah-nuebling/mac-mouse-fix/pull/1920) 建议额外 `CFRetain`，而 [PR #1936](https://github.com/noah-nuebling/mac-mouse-fix/pull/1936) 报告在 macOS 27 arm64/Rosetta round-trip 后不应额外 retain；本项目必须以自己的 retain-count/释放回归为准，不能照抄任一结论。
- [OpenMultitouchSupport](https://github.com/KrishKrosh/OpenMultitouchSupport) 说明 raw `MultitouchSupport.framework` 需要关闭 App Sandbox，并提供设备枚举、选择和异步 touch stream；这支持阶段 E 的“私有框架 + 权限/线程/设备生命周期”评估。
- [Karabiner-DriverKit-VirtualHIDDevice](https://github.com/pqrs-org/Karabiner-DriverKit-VirtualHIDDevice) 证明 DriverKit 虚拟键盘/鼠标可以被 macOS 当作硬件识别，但其控制端要求 root 权限；它不是现成的 digitizer 方案。
- [HIDTouch](https://github.com/koshi545/HIDTouch) 与 [Topti](https://github.com/Yukaii/topti) 都采用“IOHIDManager 采集 + CGEvent fallback”，并记录 `com.apple.developer.hid.virtual.device` 受限 entitlement 会阻塞真正的虚拟 HID 设备。这是阶段 E 必须核算签名、entitlement、分发和回滚成本的直接社区证据。

### 3.4 Git 历史审查

`origin/main..HEAD` 的 23 个提交集中在 2026-08-27 至 2026-08-28，约 48 个文件、1.1 万行新增；最大热点为 `src/gesture.rs`、`src/output.rs`、`src/net.rs`。最近的 `2da296f` 只运行 portable `cargo test`，没有 Preview/Photos/Dock 真机证据。历史提交信息多次使用“complete native”“native parity”等措辞，但代码实际走的是用户态 CGEvent/私有字段合成；本计划已把这些措辞收窄为分级验证结论。

### 3.5 当前代码的高风险或不合理点

1. **已处理（待 hybrid fixture 扩展）：** `src/descriptor.rs` 保留每个字段的 bit offset/width，`src/report.rs` 按 descriptor 位域解码 contact、scan time、count 和 button；6-byte reference profile 继续作为已验证基线。
2. **已处理（待真实设备 fixture 扩展）：** `src/hid.rs` 现在使用 descriptor 发现的 Digitizer/Input Mode (`0x52`) report ID，并把 vendor `0x10` 作为设备特例；descriptor 缺少标准 feature 时拒绝硬编码写入。
3. `src/output.rs:702-718` 构造 parent digitizer 且 `child_event_mask=0`，没有真实 child contacts；它可能被部分 AppKit 应用接受，但不能等同 CalfTrail 的 raw child touch stream，需阶段 D 逐字段真机比对。
4. `src/gesture.rs:2521-2610` 当前实现已改为两个已准入 transform stream 各自保持完整 phase 生命周期，并对相对增量限速；Pan→pinch/rotate 动态转场仍是兼容策略，必须在 Preview/Photos/Figma 矩阵中验证。
5. `src/output.rs:2104-2117` 的 DockSwipe 依赖私有 SkyLight ABI；虽然 macOS 27+ 缺失 attach 时会 fail-closed，但 ownership、timestamp、phase、velocity 仍无本机证据。
6. `src/output.rs:2622-2643` Smart Zoom 同时投递两种事件形状、两个 tap，疑似重复触发；阶段 D3 必须用单一 recorder/应用结果决定保留哪条路径。
7. `src/config.rs:72-80` 默认网络监听地址为空，`src/net.rs:190-198` 解析为 `0.0.0.0`；无 token 时局域网任意主机可注入事件。README 已保留警告，生产部署应显式绑定回环/LAN 和 token。
8. `src/gesture.rs:2640-2664` 的 Launchpad/Show Desktop 是离散 Dock notification/hotkey，不是连续原生四指输入流；矩阵已改成“离散命令 + 待真机”。
9. `src/output.rs:44-110` 与 `src/gesture.rs:2576-2582` 的加速曲线参数来自经验调参，不是 Apple 官方参数；文档不得写成“原生曲线”。
10. `android/app/src/main/java/com/mtc/touchpad/TouchPadView.kt:209-220` 将 `ACTION_CANCEL` 也纳入短按震动判定；取消路径应优先发送 lift/清理，不应产生点击反馈，属于 Android 后续修复项。

### 3.6 旋转/缩放专项复盘（2026-08-28）

本轮用 AnySearch（Node CLI，匿名额度）独立检索，再用 Ketch 的 Exa/Keenable 联合结果抓取原文。安装入口为 [anysearch-ai/anysearch-skill](https://github.com/anysearch-ai/anysearch-skill)，本机 Python 入口因缺少 `requests` 未采用，Node 入口可用。

#### 已确认的事件合同

- Apple 的 [Handling Trackpad Events](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/EventOverview/HandlingTouchEvents/HandlingTouchEvents.html) 说明 `magnifyWithEvent:` 和 `rotateWithEvent:` 都是相对增量；每个事件携带相对上一个同类事件的变化量。手势序列必须处理 `Began`、`Changed`、`Ended` 和 `Cancelled`，并且 magnify/rotate 在一次多点接触中可能发生序列切换。
- Apple 对 `NSEvent.magnification` 的定义是“加到当前缩放值上以得到新缩放值的变化量”；`NSEvent.rotation` 是应加到当前旋转值的角度变化，逆时针为正。这支持几何引擎输出差分值，但不支持把经验增益称作系统参数。
- [Hammerspoon PR #2512](https://github.com/Hammerspoon/hammerspoon/pull/2512) 和当前 [hs.eventtap.event 文档](https://www.hammerspoon.org/docs/hs.eventtap.event) 展示了同一套字段：gesture subtype 5/8、phase 1/2/4/8、rotation field 114、magnification field 113；其跨应用验证记录为 macOS Mojave 的 Safari/Preview。PR 中曾把投递点从 session tap 改到 HID tap，作者虽记录“对当时测试无影响”，但没有为现代系统提供普适结论。

#### 对手感最相关的开源证据

- [CalfTrail TouchSynthesis.c](https://raw.githubusercontent.com/calftrail/Touch/master/TouchSynthesis/TouchEvents.c) 用 `rDistance - totalSent` 生成 magnification 增量，并把单次 magnification 限制在 `±0.025`；rotation 直接发送本帧角度增量（度）。它还把 parent digitizer、child touch、`childEventMask` 和 vendor device ID 一起序列化。
- [Mac Mouse Fix TouchSimulator.m](https://raw.githubusercontent.com/noah-nuebling/mac-mouse-fix/master/Helper/Core/Touch/TouchSimulator.m) 使用 field 113/114 和 HID phase；其 scroll-to-zoom 路径在 `Scroll.m` 明确注释“首个 delta 似乎会被忽略”。这意味着首帧处理和 phase 组合必须单独验证，不能只看累计角度/比例。
- [SaneSideButtons 的 TouchEvents.c](https://sourcegraph.com/github.com/thealpa/SaneSideButtons/-/blob/External/TouchEvents.c#L190) 仍沿用 CalfTrail 的完整序列化结构，说明 child touch 并非一次性实验代码；但 [Hammerspoon 的 `newGesture`](https://github.com/Hammerspoon/hammerspoon/pull/2512) 也证明空 touch 数组在部分 AppKit 应用可工作。因此“无 child 一定失败”或“无 child 一定等价”都不能在没有目标 macOS 真机的情况下下结论。

#### 社区信号与本项目映射

- BetterTouchTool 社区的 [pinch 问题串](https://community.folivora.ai/t/anyone-having-issues-with-pinch-to-zoom/2591) 明确记录：macOS 忙时 CGEventTap 可能跳过事件，导致 pinch/rotate 整体失效；通过重新切换设置或重启 Dock 恢复。这是时序/状态机风险，不是单纯灵敏度问题。
- Apple 社区的 [Monterey pinch 不可靠](https://discussions.apple.com/thread/253369850) 记录了约一半成功率，并指出手指形状、接触区域会改变识别结果。我们的远端触点同样需要处理落指帧、接触不对称和丢帧，不能把每次失败都归因于倍率。
- [danqing/Pinch](https://github.com/danqing/Pinch) 将 `killall Dock` 作为常见恢复手段，说明系统手势服务本身可能卡在状态中；本项目必须把 `Cancelled`、sender restart 和输出 tap 选择纳入验收。
- [aerospace-swipe Issue #28](https://github.com/acsandmann/aerospace-swipe/issues/28) 在 macOS 26.3 的实测显示，session-level gesture event 的 `NSEvent.allTouches` 不能稳定提供完整的多指 identity；同一手势中可能只有一个可用 touch。该项目转而依赖 raw `MTRegisterContactFrameCallback` 才能拿到完整多指帧，并明确要求 Input Monitoring 权限和持续 run loop。这进一步说明我们的输出事件 recorder 不能被当作“真实触点已被系统接管”的证明。
- [TrackpadKit](https://github.com/pszypowicz/TrackpadKit) 将 settle interval、motion lock、dominance margin、velocity window 和 palm filter 分成可单测的阶段；这是比在一个函数里同时决定锁定、转场、滤波和倍率更容易校准的参考架构。

#### 当前代码的具体风险排序

1. **已处理（待真机确认）：phase/stream 合同。** `src/gesture.rs` 现在在锁定时为每个已准入 stream 发送完整的 `Began -> Changed* -> Ended/Cancelled` 生命周期，不再使用 dominant-only 的中间静默。应用层兼容性仍需 Preview/Photos/Figma 真机验收。
2. **已处理（待真机校准）：缩放单帧跳变。** `scale_delta` 现在同时受时间速率和 `±0.08` 单帧硬上限约束；异常暂停、丢帧和非有限输入会被限制或丢弃。上限是防护参数，不代表 Apple 的官方阈值。
3. **已处理（保守默认）：旋转增益。** 旋转改为独立 transform 时间基准，并默认输出几何 1:1 的有符号相对角度；原先未经验证的 2.0x 经验曲线已移除，后续只能通过真机 A/B 作为显式实验参数恢复。
4. **高：Pan→Pinch 动态转场不是 Apple 的 scroll lock。** Apple 文档只允许 magnify/rotate 在一个多点接触中重新解释；scroll 一旦开始应锁定到结束。当前为补救误分类而保留的 Pan→pinch 转场应标成兼容性策略，而不是 native parity。
5. **中：输出 tap 未完成 A/B。** 当前 `src/output.rs` 的 pinch/rotate 发往 `kCGSessionEventTap`，而 Mac Mouse Fix、Hammerspoon 的主路径都使用 `kCGHIDEventTap`。Hammerspoon 旧 PR 说当时“无影响”，所以应在目标系统录制两条路径后再定，不应把当前选择写成原生事实。
6. **中：payload 只含 parent digitizer。** 当前 `child_event_mask=0`、vendor payload 的 device ID 为 0，没有真实 child contacts；这可能足够触发简单 `NSResponder` 路径，但不足以证明对 `NSMagnificationGestureRecognizer`/`NSRotationGestureRecognizer` 的跨应用兼容。
7. **中：应用层事件接受仍无证据。** 现在的 phase 合同和 1:1 几何增量在 portable recorder 上可回归，但 `kCGSessionEventTap`、parent-only payload、首帧策略和动态转场仍需目标 macOS 应用矩阵验证。

#### 下一步决策（先测合同，再调参数）

- [x] R1. 增加纯逻辑的 pinch/rotate delta filter：使用时间速率 + 单帧上限，覆盖正常、超限和非有限输入。
- [x] R2. 选定 phase 合同：两个已准入 stream 都完整发送 `Changed`，结束时分别发送 `Ended` 或 `Cancelled`。
- [x] R3. 暂停“native rotation curve”命名，默认回到 1:1 几何角度；经验加速不再进入默认路径。
- [ ] R4. 在 macOS recorder 上对 HID tap/session tap、空 child/真实 child、首帧 0 delta/首帧有效 delta 做最小 A/B 矩阵；记录 Preview、Photos、Safari、Maps、Figma 的实际回调和结束状态。
- [ ] R5. 在 R1-R4 有结果前，不继续微调 1.25x/1.85x/2.0x 等倍率，也不把测试 recorder 的“调用成功”升级成“原生体验完成”。
- [ ] R6. 把 Pan→pinch 转场作为独立“兼容模式”开关评估；native 模式默认保持 scroll lock，避免用一个非原生补救策略掩盖识别器问题。

## 4. 本轮实际执行清单

- [x] 更新规划书并冻结基线。
- [x] 实施阶段 A 的高确定性代码修复。
- [x] 实施阶段 B 的配置和文档修复。
- [x] 运行 Rust、Android、Clippy 和 diff 检查。
- [x] 将命令输出、未完成原因和真机验收入口写回本文件。
- [x] 使用 AnySearch + Ketch 完成旋转/缩放专项资料核对，形成 R1-R6 执行项。
- [x] 在 macOS 真机前不继续以经验倍率修改旋转/缩放输出。
- [x] F1. 新增 Core Foundation 系统设置读取、主/回退 domain 冲突记录和 normalized policy。
- [x] F2. 固定 TOML 显式字段优先级，启动时合并到 HID 与 network 两条入口。
- [x] F3. 增加滚动/点击/智能缩放/查词/三指拖移/四指 swipe 的高确定性开关映射。
- [x] F4. 采用 `NSHapticFeedbackManager.defaultPerformer()` 提供设备感知触觉确认，并修复 Android `ACTION_CANCEL` 误震动。
- [x] R1-R3. 完成旋转/缩放相对增量过滤、phase 生命周期统一和 1:1 旋转默认值；新增纯逻辑回归测试。
- [x] C2. 从 descriptor 发现 Input Mode feature report ID，新增非 `0x08` report ID 回归 fixture，并处理无编号 report 的 payload 规则。
- [x] C1. 按 descriptor 位域解码非字节对齐 contact/trailing fields，新增 bit-packed report 回归 fixture。
- [ ] F5. 在目标 macOS 版本用真实 MacBook/Magic Trackpad 验证 performer 是否可用、触发时机和系统“触控反馈”开关；未完成前不宣称硬件 click parity。

### 4.1 本轮验证记录

- `~/.cargo/bin/cargo test --workspace`：通过，125 个主工程测试 + 9 个协议测试（包含 R1 transform filter、C1 bit-packed decode 与 C2 descriptor report-ID 回归）。
- `~/.cargo/bin/cargo check --all-targets`：通过。
- `~/.cargo/bin/cargo check --target aarch64-apple-darwin --all-targets`：通过（交叉检查 macOS binary/module wiring；仅有既有 dead-code 警告）。
- `android/./gradlew test`（工作目录 `android/`）：通过，Gradle `BUILD SUCCESSFUL`。
- `android/./gradlew assembleDebug`：通过；`adb install -r android/app/build/outputs/apk/debug/app-debug.apk` 返回 `Success`，已在设备 `192.168.3.131:34743` 启动 `com.mtc.touchpad/.MainActivity`。
- `git diff --check`：通过。
- `git diff --check origin/main...HEAD`：失败，仅命中既有 `android/gradlew.bat` CRLF 和 `diagnostics/mac-settings.txt` EOF 空行；本轮未修改这两个无关文件。
- `~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings`：失败，仓库现有 11 个 lint（6 个 `collapsible_if`/冗余匹配，1 个近似常量，4 个旧文档 quote continuation）；本轮未为清理这些无关质量债务扩大修改面。
- `~/.cargo/bin/cargo fmt --all -- --check`：失败，仓库基线已有多个未格式化文件（协议 crate、descriptor、overlay、gesture_tap 等）；本轮新增代码可编译且 `git diff --check` 通过，未执行全仓格式化以免改写无关历史代码。

### 4.2 真机验收入口

阶段 C/D 不能在 Linux 上宣称完成。需要 macOS 主机、目标 major 版本、Accessibility 权限，以及至少 Preview、Photos、Safari、Maps、Figma、Mission Control、Spaces。每个用例记录系统版本、架构、权限、事件 recorder 输出和应用结果；覆盖 tap/hold/lift、pinch、rotate、cancel、反向、连续两次 gesture、DockSwipe 中途反向和 modifier zoom 抬指。

## 5. 风险与降级

- UDP 乱序：丢弃迟到 motion frame；lift replay 仍允许一次安全重传。
- sender restart：取消当前 touch，重置 scan clock，再接受新 session。
- modifier 变化：以 `Phase::Began` 的 modifier 快照决定整个 scroll session 的输出类型。
- 私有 ABI 不可用：不发送未经验证的 legacy payload，保留公共 cursor/click/scroll。
- 真机不可用：不修改为“猜测通过”，阶段 D/E 保留待验收状态。

## 6. 执行回写

本节在每次执行后更新。状态只允许使用：`已完成`、`部分完成`、`待真机`、`已知限制`。

- 阶段 A：已完成
- 阶段 B：已完成
- 阶段 C：部分完成（C1/C2/C5 已完成；C3/C4 的 parallel/hybrid fixture 与聚合规则待执行）
- 阶段 D：待真机
- 阶段 E：部分完成（已收集 raw MultitouchSupport 资料；虚拟 HID 成本评估与架构决策待后续）
