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
- [x] C3. 增加按 Microsoft 规则构造的 parallel、single-finger hybrid、two-finger hybrid 合成 fixtures，覆盖 descriptor slot 数和 HID 聚合器行为；真实设备样本仍待采集。
- [x] C4. 明确并实现 Contact Count=0 但仍携带 contact 数据时的聚合规则：同一 Scan Time 的 hybrid 分片在 HID 层聚合，Scan Time 变化则丢弃不完整分片。
- [x] C5. 保留 6-byte reference profile，同时说明 decoder 已支持 descriptor-defined bit-packed fields；parallel/hybrid 需以专用 fixtures 继续验收。

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
- 第一阶段实际驱动：轻点点击、双指右键、自然滚动、滚动开关、滚动惯性、横向滚动、捏合、旋转、智能缩放、三指查词、三指拖移、单指轻点拖移、右边缘通知中心、四指水平/垂直轻扫，以及 `HIDScrollZoomModifierMask`。`DragLock` 只做诊断，不覆盖三指换把参数。
- 非布尔或只影响 Apple 驱动/硬件的字段仍被探测并报告为 `unsupported`，包括压力阈值、palm/resting、Force Touch、五指捏合和 USB 鼠标联动；不把它们伪装成已应用。`ActuateDetents` 作为唯一高确定性触觉开关映射到 `macos.haptic_feedback`。
- `HIDScrollZoomModifierMask` 解析为 Quartz modifier mask；值为未知位时保留已知位并记录告警，值缺失时继续使用 Cmd/Ctrl 兼容默认。
- `.GlobalPreferences` 的 `com.apple.trackpad.scaling` / `com.apple.scrollwheel.scaling` 只做有界兼容性归一化：分别填充 `cursor.sensitivity` / `scroll.sensitivity`；`trackpad.scaling = -1` 仅映射为线性光标曲线。Apple 没有公开稳定的 px/mm 传递公式，不能把这些内部数值宣称成物理校准。
- `Clicking = 0` 只对本地 HID/实体触控板入口按原生语义关闭 tap-to-click；`companion-net` 是虚拟输入入口，Mac mini 上残留的物理触控板偏好不会关闭手机轻点。两条入口都仍允许 TOML `gestures.tap_to_click = "off"` 显式关闭。`DragLock` 不再写入三指 `release_delay_ms`，避免错误地取消三指换把悬停。
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
| `Clicking` | `GestureOptions.tap_to_click`（本地 HID） | 仅 `0/1` 有效；本地实体触控板入口关闭时不产生手指 tap 左键；`companion-net` 忽略该物理开关 |
| `TrackpadRightClick` | `GestureOptions.secondary_click` | 仅 `0/1` 有效；关闭时两指 tap 不产生右键 |
| `TrackpadScroll` | `GestureOptions.scroll_enabled` | 关闭时 2F 平移不发滚动事件 |
| `TrackpadHorizScroll` | `output::Config.horizontal_scroll` | 关闭时丢弃横向滚动分量，保留纵向滚动 |
| `TrackpadMomentumScroll` | `output::Config.momentum_scroll` | 关闭时 lift 不启动 inertia |
| `TrackpadPinch` / `TrackpadRotate` | `output::Config.pinch/rotate` | 作为 On/Off policy，仍支持 TOML app filter |
| `TrackpadTwoFingerDoubleTapGesture` | `GestureOptions.smart_zoom` | 值 `1` 开启双指智能缩放 |
| `TrackpadThreeFingerTapGesture` | `GestureOptions.dictionary_lookup` | 值 `2`（及兼容的非零启用值）开启查词 |
| `TrackpadThreeFingerDrag` | `GestureOptions.three_finger_drag` | 值 `1` 为三指拖移，`0` 为三指轻扫 |
| `Dragging` | `GestureOptions.one_finger_tap_drag` | 辅助功能拖移开关；只影响单指双击拖移 |
| `DragLock` | 诊断信息 | 不覆盖三指 `release_delay_ms`；该键与三指换把不是同一语义 |
| `TrackpadTwoFingerFromRightEdgeSwipeGesture` | `GestureOptions.right_edge_swipe` | 非零启用右缘通知中心动作 |
| `TrackpadFourFingerHoriz/VertSwipeGesture` | horizontal/vertical swipe policy | `0` 关闭对应轴，`2` 开启；未知值只记录并保留配置 |
| `HIDScrollZoomModifierMask` | `output::Config.modifier_zoom_mask` | 使用系统 mask 锁定整个 scroll session 的 zoom 路由 |
| `ActuateDetents` | `macos.haptic_feedback` | `1` 开启语义触觉，`0` 关闭；`auto` 模式由系统值填充 |
| `com.apple.swipescrolldirection` | `output::Config.natural_scroll` | 全局值优先于触控板域中的历史别名 |
| `.GlobalPreferences/com.apple.trackpad.scaling` | `[cursor] sensitivity` / `accel_exponent` | 有界兼容性归一化；`-1` 只表示线性曲线 |
| `.GlobalPreferences/com.apple.scrollwheel.scaling` | `[scroll] sensitivity` | 以 `0.6875` 为常见基线的有界兼容性标量 |

#### Verification

- 单测覆盖：缺少 key、主域/回退域冲突、未知枚举、modifier mask、TOML 覆盖系统值，以及每个关闭开关的事件抑制。
- 单测覆盖：缺少 Trackpad/global domain、scaling 正常值与越界/NaN、显式 TOML 对速度参数的覆盖，以及 `DragLock` 不改变三指 `release_delay_ms`。
- macOS 真机启动日志记录系统版本、来源域、应用字段和 unsupported 列表；`scripts/diagnose-mac.sh` 输出与读取层使用同一组 key。
- Linux 上运行 `cargo test --workspace`、`cargo check --all-targets`；macOS 上额外验证 Preview、Photos、Safari、Maps、Mission Control、Spaces 的 tap/scroll/inertia/pinch/rotate/swipe。

最脆弱假设：`CFPreferences` 返回的触控板域仍代表当前活跃设备。若新版本 macOS 改用未覆盖的 domain 或枚举，读取层会保留默认/显式配置并在日志标记 `unsupported`，不会把错误值传播到手势引擎。

### 阶段 H：Mac mini 设置入口与原生面板可用性（本轮执行）

目标：确认没有实体触控板时能否让 macOS 自带 Trackpad 面板出现；如果系统不允许，提供不依赖该面板的完整 companion 配置入口。

#### 调研结论

- [x] H1. Apple 官方文档明确要求“内置触控板或已连接的无线触控板”才能查看/修改 Trackpad 设置；Mac mini 没有设备时缺少该面板是系统设计，不是单个 plist key 漏写。
- [x] H2. `open "x-apple.systempreferences:com.apple.Trackpad-Settings.extension"`（Ventura 及更新）和 `open /System/Library/PreferencePanes/Trackpad.prefPane`（旧式入口）只是深链/打开已存在的 pane；它们不会绕过硬件检测。
- [x] H3. `defaults write`、`CFPreferences`、`activateSettings -u`、重启 `cfprefsd` 或同时写主/蓝牙 domain 可以更新持久化值，但没有公开接口把 Mac mini 注册成 Apple trackpad。写入残留 `AppleMultitouchTrackpad` domain 不能使 System Settings 生成可用 UI。
- [x] H4. Apple 的 `SystemPreferences` 配置描述中的 `EnabledPreferencePanes` / `DisabledPreferencePanes` 只控制 pane 可见性，不能创建输入设备能力；不采用 MDM/profile 伪造方案。
- [x] H5. 开源 defaults/TUI 项目也都停留在读写历史 defaults。真正让原生驱动接管需要连接 Magic Trackpad，或进入 DriverKit/虚拟 HID digitizer 的签名、entitlement、权限和分发评估，超出本项目的无驱动用户态边界。

#### 已执行方案

- [x] H6. 新增 `companion-tui`，在 Mac mini 上直接编辑 companion 的点击、右键、智能缩放、捏合、旋转、滚动、惯性、缩放修饰键、三指拖移、四指 swipe、光标/滚动灵敏度、系统同步和触觉策略。
- [x] H7. TUI 只原子写入 `config.toml`，不修改 macOS defaults，不需要 `sudo`；Mac 系统偏好可用性以只读摘要呈现，无法模拟的硬件项继续写入 companion 启动诊断。
- [x] H8. `companion-net` 使用虚拟输入合并路径，忽略无实体触控板时残留的 `Clicking=0`；仍尊重 TOML 对 `gestures.tap_to_click = "off"` 的显式关闭。

#### 验收与限制

- `cargo run --release --bin companion-tui` 或发布包中的 `./companion-tui` 是 Mac mini 的推荐入口；保存后重启 `companion-net` 以加载配置。
- 若用户确实需要 Apple 原生 Trackpad pane，唯一受支持的验证路径是连接 Magic Trackpad（蓝牙/USB）后重新打开 System Settings；不把深链成功或 plist 写入误报为“强行开启”。
- H 阶段已完成“可行性核查 + 无面板配置入口”；原生 pane 的硬件检测和真正 Apple multitouch stream 仍属于 macOS/驱动边界，标记为已知限制，不再投入无证据的 defaults 猜测。

### 阶段 J：键盘修饰键与触控板组合（本轮执行）

目标：让 Shift、Command、Control、Option 在点击、拖拽、滚动、捏合、旋转和系统手势中遵循 macOS 的分层语义；只对有证据的“修饰键改变手势类型”做转换。

#### 规则与实现

- [x] J1. 统一使用 Quartz 的四个 modifier flag（Shift `0x00020000`、Control `0x00040000`、Option `0x00080000`、Command `0x00100000`），鼠标/滚动/私有 gesture 事件均以当前键盘状态覆盖这四个位，避免 stale flags。
- [x] J2. Control-click 保持为带 Control flag 的左键事件，让 AppKit 按 Apple 语义解释为 secondary click；Command/Option/Shift-click 和拖拽不做应用层猜测，交给前台 App。
- [x] J3. Accessibility Zoom 的滚动修饰键支持 Control、Option、Command；`HIDScrollZoomModifierMask` 可从系统偏好或 TOML 选择，且在 `Phase::Began` 锁定到整个 scroll session。
- [x] J4. Shift + 双指滚动默认保持原始轴向（严格原生）；新增 `[scroll].shift_scroll_horizontal` 兼容开关，只有显式 `true` 才把纯纵向输入转成横向。
- [x] J5. `companion-tui` 增加 Zoom mask 的 Control/Option/Command 循环和 Shift 兼容开关；补充纯逻辑 modifier/轴向回归测试。
- [x] J7. 合成的 SymbolicHotKey 与 Control+Arrow 保留快捷键自身所需修饰位；面向 App 的指针/手势事件才合并用户实时 Shift/Command/Control/Option。内部 Cmd+Ctrl+D 查词脉冲仍使用固定组合，避免被用户键盘状态污染。

#### 不做的推断

- 不把 Command/Option/Shift 点击硬编码成“打开标签/复制/多选”等动作；这些是 Finder、Safari、编辑器等应用语义，Quartz flags 已足够。
- 不把 Shift 说成 Apple Trackpad 的内建横向滚动手势；Apple 官方 Accessibility Zoom 只列 Control、Option、Command，Shift 兼容路径必须由用户主动打开。
- 不在没有目标 App/系统录制证据时改变 pinch/rotate 的几何增量或 DockSwipe 的私有 payload；修饰键只随事件传递。

#### 验收

- 自动化：modifier mask 选择、session latch、Option Zoom、Shift 默认原生轴向与显式兼容转换。
- macOS 真机：Safari/Preview/Photos/Numbers/Finder 分别验证 Control-click、Command/Option/Shift-click/drag、Control/Option/Command + scroll（Accessibility Zoom 开关开/关）、Shift scroll 两种模式；记录键盘布局和系统版本。

### 阶段 K：三指拖拽与四指切 Space 联合（本轮执行）

目标：三指拖拽已经按住窗口时，完整抬起三指进入锁定态，再用四指切换 Space，抬起后重新落三指继续拖动；该体验以“可靠完成跨桌面拖拽”为目标，不声称能复刻 Apple 私有 multitouch identity。

#### 调研结论

- Apple 官方分别记录三指拖拽和四指左右切换全屏 App/桌面，但没有公开“两个手势并行”的事件合同；原生 Trackpad 面板只提供独立开关。
- Apple 社区有用户报告用触控板把文件拖到另一 Space 失败，说明系统版本、Finder 路径和拖拽目标会影响结果；该类案例不能作为普遍支持证据。
- Mac Mouse Fix Issue #1735 把“按住窗口、执行切 Space、保持抓取、目标 Space 释放”列为明确需求。其 PR #1875 的早期补丁曾把 macOS 26/27 都切到 SymbolicHotKey；后续运行时版本修订确认 macOS 26 保留连续 DockSwipe，而 macOS 27+ 才需要 SymbolicHotKey，并尝试通过 SkyLight `SLEventSetIOHIDEvent` 附加 HIDEvent。Symbolic 路径使用约 220px/150px 阈值、350ms 冷却和快速释放检测保证不会连续跳过多个 Space。
- BetterTouchTool 社区指出四指拖动并非原生能力，鼠标按钮切 Space 过快还可能被系统解释为双击最小化；因此本项目保留“按键生命周期”和“切换节流”两个独立状态，不把四指输入直接伪装成鼠标双击。

#### 实现与验收状态

- [x] K1. 将联合拖拽合同收敛为显式 `3F -> 0F -> 4F -> 0F -> 3F` staged 状态机；第四指只有在完整三指抬起后才接管，避免加指时静默抢走 live drag。
- [x] K1a. 修复真实网络帧的 `3F -> 0F -> 1F/2F -> 4F` re-grip：release-delay 窗口内的 1F/2F 只做重锚定，不提前发送 `leftMouseUp`；原始 deadline 到期由 heartbeat 安全释放并清空旧触点状态。
- [x] K1c. 增加 `DragLocked` 持久会话：4F Space 阶段结束后返回锁定态，重新落 3F 复用同一左键拖拽；1F/2F 短触摸显式解锁，断链/取消仍强制释放。
- [x] K2. macOS 26 及更早版本继续使用已存在的动画 DockSwipe payload。
- [x] K3. macOS 26 的独立四指 swipe 保留连续 DockSwipe；带着三指拖拽切 Space 时自动走 CGS SymbolicHotKey（规避 WindowServer 对第三方 DockSwipe sender 的静默丢弃），macOS 27+ 的独立 `synthetic` swipe 优先通过 SkyLight `SLEventSetIOHIDEvent`，运行时不可用时降级到同一 SymbolicHotKey 状态机。Symbolic 路径横向 Space 阈值 10mm、纵向 Mission Control/App Exposé 阈值 7mm，单次触发后冷却 350ms，不在冷却期累积位移。
- [x] K4. 增加快速抬指路径（速度阈值 180mm/s，且最后运动距抬指不超过 80ms），避免停住后释放仍误切 Space。
- [x] K5. 取消/断链/进程退出不会发送错误的 DockSwipe 取消包，也不会留下按键粘连。
- [ ] K6. macOS 真机矩阵：MacBook/Magic Trackpad + Finder、Preview、Safari、Numbers；分别验证 3F→4F 加指、左右多 Space、切换中反向、目标 Space 抬指、Mission Control/App Exposé，以及 macOS 25/26/27 的事件日志和窗口结果。

#### K6 调研结论（2026-08-29）

- **需求合同已被 Mac Mouse Fix Issue #1735 明确写出：**先按住窗口进入拖拽，执行
  Space 切换，窗口在过渡期间保持 grabbed，进入目标 Space 后再释放；这不是“同时让
  macOS 识别三指和四指原生触点”，而是拖拽按钮生命周期与系统 Space 输出的组合。
  参考：<https://github.com/noah-nuebling/mac-mouse-fix/issues/1735>。
- **系统路径存在版本分歧。**Mac Mouse Fix PR #1875 的早期提交把 macOS 26+ 都降级为
  SymbolicHotKey；后续运行时版本修订和当前 `TouchSimulator.m` 将连续 legacy 字段路径
  保留到 macOS 26，并在 macOS 27+ 构造 `HIDEvent`。因此不能只依据 PR 的早期评论判断
  26.5.x；目标版本必须实测。参考：
  <https://github.com/noah-nuebling/mac-mouse-fix/pull/1875>、
  <https://raw.githubusercontent.com/noah-nuebling/mac-mouse-fix/master/Helper/Core/Touch/TouchSimulator.m>。
- **macOS 27 的连续路径依赖私有 setter。**PR #1920 记录了旧的
  `CGEventSetIOHIDEvent` opaque offset（`0x18/0xd0`）在 27 上失效，建议运行时解析
  `SLEventSetIOHIDEvent`；找不到符号时只能降级。该 PR 还提示 ownership 在 arm64、
  Rosetta 和不同修订间存在争议，必须用本项目自己的 recorder/释放回归定案，不能把
  “调用成功”当作 Dock 已消费。参考：<https://github.com/noah-nuebling/mac-mouse-fix/pull/1920>。
- **独立 OSS 实现复核了事件形状而非原生保证。**`oomol-lab/dockswipe` 使用 type 30
  DockControl、subtype 23、axis/progress/phase 字段，按约 8ms 的多帧流驱动连续动画，
  并明确标注 macOS 27+ 需要 IOHIDEvent 路径；这支持当前字段布局，但不证明窗口拖拽
  在所有系统版本都跨 Space 保持抓取。参考：<https://github.com/oomol-lab/dockswipe>。
- **三指与系统 Space 手势默认存在冲突。**Trident 的首次运行引导要求把 macOS 的
  三指 Space 手势改成四指，否则自定义三指手势会与系统动作同时触发；这支持本项目
  采用“3F drag + 4F swipe”的显式状态机，而不是依赖系统设置自动并行识别。参考：
  <https://github.com/cyanyux/trident>。

当前代码与上述合同的对应关系：`src/gesture.rs` 只有从 `DragLocked` 进入
`FourFingerLive` 时才保留 `drag_button_held`；四指 `Ended` 后返回 `DragLocked`，再落
三指恢复移动，显式 1F/2F 短触摸才解锁；`src/output.rs` 在 macOS 26 及更早
版本走 legacy DockSwipe，macOS 27+ 先尝试 `SLEventSetIOHIDEvent`，失败后进入带阈值、
350ms 冷却和 flick 保护的 SymbolicHotKey。状态机测试和断链收尾已完成；**K6 的真实
WindowServer/Dock 消费结果、ownership/timestamp、窗口跨 Space 附着仍待真机**，本轮不
将其标记为完成。

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

1. **已处理（待真实 descriptor 采样扩展）：** `src/descriptor.rs` 保留每个字段的 bit offset/width，`src/report.rs` 按 descriptor 位域解码 contact、scan time、count 和 button；6-byte reference profile 继续作为已验证基线。
2. **已处理（待真实设备确认）：** HID 层使用 `HybridAssembler` 聚合同一 Scan Time 的 zero-count 分片，避免把 hybrid 后续报告误判为 lift；跨 Scan Time 的不完整分片会被丢弃。
3. **已处理（待真实设备 fixture 扩展）：** `src/hid.rs` 现在使用 descriptor 发现的 Digitizer/Input Mode (`0x52`) report ID，并把 vendor `0x10` 作为设备特例；descriptor 缺少标准 feature 时拒绝硬编码写入。
4. `src/output.rs:702-718` 构造 parent digitizer 且 `child_event_mask=0`，没有真实 child contacts；它可能被部分 AppKit 应用接受，但不能等同 CalfTrail 的 raw child touch stream，需阶段 D 逐字段真机比对。
5. `src/gesture.rs:2521-2610` 当前实现已改为两个已准入 transform stream 各自保持完整 phase 生命周期，并对相对增量限速；Pan→pinch/rotate 动态转场仍是兼容策略，必须在 Preview/Photos/Figma 矩阵中验证。
6. `src/output.rs` 的 DockSwipe 依赖私有 SkyLight ABI；macOS 27+ 优先使用 `SLEventSetIOHIDEvent`，缺失或失败时只降级到 SymbolicHotKey，不写入未经验证的 legacy 字段；ownership、timestamp、phase、velocity 仍无本机证据。
7. **已处理（待真机验证）：** `src/output.rs:2960-2971` 的 Smart Zoom 已收敛为 Mac Mouse Fix 参考的 type 29/subtype 22、单 HID tap；仍需阶段 D3 用 recorder 和应用结果确认该单一路径在目标 macOS 上被消费。
8. `src/config.rs:72-80` 默认网络监听地址为空，`src/net.rs:190-198` 解析为 `0.0.0.0`；无 token 时局域网任意主机可注入事件。README 已保留警告，生产部署应显式绑定回环/LAN 和 token。
9. `src/gesture.rs:2640-2664` 的 Launchpad/Show Desktop 是离散 Dock notification/hotkey，不是连续原生四指输入流；矩阵已改成“离散命令 + 待真机”。
10. `src/output.rs` 和 `src/gesture.rs` 的其他速度/阈值参数仍需真机校准，不应写成 Apple 官方参数。
11. `android/app/src/main/java/com/mtc/touchpad/TouchPadView.kt:209-220` 的 `ACTION_CANCEL` 误震动已修复，需保留 Android 设备回归。

### 3.6a Android 深按触觉专项复盘（2026-08-29）

本轮针对“深按条震动太弱”做了 AnySearch Node CLI + Ketch Exa/Firecrawl/grep.app 的交叉检索，并在 Redmi 23078RKD5C（Android 16 / SDK 36，`goodix_ts`）上复测。关键证据：

- [Apple Support: Force Click and haptic feedback](https://support.apple.com/en-us/102309) 把体验定义为普通点击后继续用力，感到第二个更深的 click；它同时说明 Force Touch 轨迹板依赖压力传感器与专用触觉硬件。
- [Apple HIG: Playing haptics](https://developer.apple.com/design/human-interface-guidelines/playing-haptics) 将 macOS 触觉分为 Generic、Alignment、Level change，并要求离散事件优先使用短促反馈。
- [Android: Analyze vibration waveforms](https://developer.android.com/develop/ui/views/haptics/actuators) 说明手机常用 LRA，清晰 click 通常为 10–20ms；偏离执行器共振频率会显著降低输出。
- [Android: Create custom haptic effects](https://developer.android.com/develop/ui/views/haptics/custom-haptic-effects) 要求按设备能力在 primitives、幅度控制和仅开关三类路径间降级。
- [AOSP VibrationAttributes](https://android.googlesource.com/platform/frameworks/base.git/+/refs/heads/main/core/java/android/os/VibrationAttributes.java) 将 `USAGE_PHYSICAL_EMULATION` 定义为模拟实体反应（如 edge squeeze），比普通 `USAGE_TOUCH` 更符合深按条语义。
- 开源实现 [MusicRecognizer VibrationManagerImpl](https://github.com/aleksey-saenko/MusicRecognizer/blob/2e96802e03bcc1e91c795ea791f3dcbfdebbddb0/feature/recognition/src/main/java/com/mrsep/musicrecognizer/feature/recognition/platform/VibrationManagerImpl.kt) 与 [react-native-nitro-haptics HybridHaptics.kt](https://github.com/oblador/react-native-nitro-haptics/blob/ccda845fa012fa1e8da2b3152d03003a7fc304ec/android/src/main/java/com/haptics/HybridHaptics.kt) 都采用显式 timings/amplitudes，并按 `hasAmplitudeControl()` 降级。

设备实测：该机支持 `AMPLITUDE_CONTROL`，但 `supportedPrimitives=[]`；预置 `HEAVY_CLICK` 记录为 `TOUCH / MEDIUM / 约75ms`，主观反馈偏软。系统桌面实际使用厂商预置 `163`（`HARDWARE_FEEDBACK`，约80ms），因此 Xiaomi/Redmi 且 HAL 明确支持 163 时深按优先复用该校准效果；其他设备在 `ACTION_DOWN` 先发普通短 click（8ms 起始冲击 + 5ms 阻尼尾），跨过保持阈值后再发 `createWaveform([0,11,4,8], [0,peak,0,0.34*peak])` 的更深冲击，以 `USAGE_PHYSICAL_EMULATION` 播放，默认 `peak=255`，并保留无幅度控制时的预置/one-shot fallback。

已知边界：Android 应用不能突破系统触觉总开关、系统强度档位或手机 LRA 的物理输出；没有可公开控制的“苹果 Taptic Engine 波形”。因此强度滑杆是 APK 内波形峰值（40–255）的适配参数，不是物理加力，也不把效果宣称为 Mac Force Touch。

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

### 3.7 Mac mini 无触控板时能否强行显示设置面板

这条路径已按“官方约束 → 深链 → defaults/配置描述 → 开源实现”顺序核对：

- Apple [Change Trackpad settings on Mac](https://support.apple.com/guide/mac-help/change-trackpad-settings-mchlp1226/mac) 直接写明，修改 Trackpad 设置必须使用带内置触控板的 Mac，或连接无线触控板；[View and customize mouse or trackpad gestures](https://support.apple.com/en-gw/guide/mac-help/mh35869/mac) 对查看手势也有同样前置条件。
- Ventura+ 的 `com.apple.Trackpad-Settings.extension` 和旧式 `Trackpad.prefPane` 入口来自公开的 [macOS settings URL 列表](https://github.com/paralevel/macos-settings-urls)。它们适合把用户带到已有页面，但页面仍由 System Settings 按硬件能力决定是否渲染；深链不是注册设备的 API。
- Apple [SystemPreferences 配置](https://developer.apple.com/documentation/devicemanagement/systempreferences) 的 `EnabledPreferencePanes` / `DisabledPreferencePanes` 只改变可见性。社区中常见的 `defaults write ... AppleMultitouchTrackpad`、`-currentHost`、`killall cfprefsd`、`activateSettings -u` 解决的是缓存/持久化同步，不能让无触控板的 Mac 产生 HID digitizer。
- [yannbertrand/macos-defaults](https://github.com/yannbertrand/macos-defaults) 等开源项目能批量设置 `Clicking`、`Dragging`、`TrackpadThreeFingerDrag` 等历史 key，但没有任何设备注册或 Trackpad pane 强制显示实现。能让原生 pane 真正出现的可靠方式仍是连接 Magic Trackpad/内置硬件；虚拟 HID 需要 DriverKit entitlement 和签名，且不保证被 Apple pane 当作 Trackpad 接受。

因此本项目不加入“伪造 pane”或高风险系统 plist 注入。Mac mini 的配置体验由 `companion-tui` 承担，系统 pane 仅作为连接真实 Apple trackpad 后的可选校准入口。

### 3.8 Shift / Command / Control / Option 组合调研

- Apple 的 [Right-click on Mac](https://support.apple.com/en-my/guide/mac-help/mh35853/mac) 明确把 Control-click 定义为 secondary click；这要求我们传递 Control flag，而不是在手势层擅自把所有单指点击改成右键。
- Apple 的 [Change Zoom settings for accessibility](https://support.apple.com/guide/mac-help/mh40579/mac) 明确说明“Use scroll gesture with modifier keys to zoom”可选择 Control、Option 或 Command，并以所选 modifier 配合触控板滚动；Shift 不在该列表中。
- Apple 的 [Use Multi-Touch gestures](https://support.apple.com/en-us/102482) 列出触控板的点击、两指滚动、捏合、旋转和四指手势，但没有 Shift + 滚动的系统手势。社区对 Magic Trackpad 的实测也记录了 Shift + 垂直双指滚动仍保持垂直，因此现有无条件轴向转换只能被视为兼容层。
- Apple 的 [Mac keyboard shortcuts](https://support.apple.com/en-us/102650) 将 Command、Control、Option、Shift 定义为通用 modifier；Command/Option/Shift 的点击、拖拽结果取决于 Finder、Safari、编辑器等当前 App。Quartz 事件携带 flags 即可保留这些语义。
- 现有实现审查发现：`Event::post` 原本只把当前 flags OR 到事件上，且 click 的 down/up 分别采样，可能造成修饰键快释放时一对 click flags 不一致；`scroll` 已有 Cmd/Ctrl mask latch 和 Shift 轴向转换，但没有 Option 的 TUI 选择，也没有严格原生的 Shift 开关。

本轮采用“事件 flags 统一传递 + 少量有证据的类型转换”方案：Control/Option/Command 只在所选 Zoom mask 命中时把整个 scroll session 转为 magnification；四种 modifier 对 click/drag/gesture 的其他含义全部交给 macOS/AppKit/App；Shift 轴向转换改为显式兼容开关，默认关闭。

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
- [x] F3a. `HIDScrollZoomModifierMask` 只应用已知 Quartz modifier 位，未知位/负值写入 `unsupported` 诊断并保留默认策略。
- [x] F4. 采用 `NSHapticFeedbackManager.defaultPerformer()` 提供设备感知触觉确认，并修复 Android `ACTION_CANCEL` 误震动。
- [x] F5a. 接入 `com.apple.trackpad.scaling` / `com.apple.scrollwheel.scaling` 的有界兼容性归一化、显式 TOML 覆盖和缺失 domain 降级。
- [x] R1-R3. 完成旋转/缩放相对增量过滤、phase 生命周期统一和 1:1 旋转默认值；新增纯逻辑回归测试。
- [x] R7. 收紧两指 transform 锁定：低于明确意图位移的 spread/rotation 保持 tap/未分类，避免 pinch 误触。
- [x] F5b. 修正 `DragLock` 误覆盖三指 `release_delay_ms` 的映射；三指 500ms 换把悬停恢复为配置默认。
- [x] C2. 从 descriptor 发现 Input Mode feature report ID，新增非 `0x08` report ID 回归 fixture，并处理无编号 report 的 payload 规则。
- [x] C1. 按 descriptor 位域解码非字节对齐 contact/trailing fields，新增 bit-packed report 回归 fixture。
- [x] C4. 新增同 Scan Time hybrid 聚合、跨 Scan Time 丢弃和 button 合并回归测试。
- [x] C3. 新增 parallel、single-finger hybrid、two-finger hybrid fixture 回归测试。
- [x] H1-H5. 核查 Mac mini 无实体触控板时的 Apple pane 硬件门槛、深链、defaults/MDM 边界和开源实现；确认没有受支持的“强行显示”接口。
- [x] H6-H8. 新增 `companion-tui` 作为无 Trackpad pane 的配置入口，并让 `companion-net` 忽略虚拟输入不适用的残留 `Clicking=0`。
- [x] J1-J5. 完成 Shift/Command/Control/Option 的 Quartz flags 传递、Zoom modifier session latch、Shift 兼容开关和 TUI 配置入口；系统合成热键改走保留 flags 的派发路径。
- [x] J6. 修正横向滚动总开关优先级：`horizontal = false` 现在同时抑制原生横向分量和 Shift 兼容映射。
- [x] K1-K5. 实现三指拖拽进入四指切 Space 时的按键保持、macOS 26 DockSwipe 与 macOS 27+ HIDEvent 优先/SymbolicHotKey fallback、阈值/冷却/flick 防误触，以及取消/断链安全收尾。
- [x] K1b. 修正 macOS 26 legacy DockSwipe 的字段类型契约（header/phase/axis 使用 double 槽，进度编码与 inverted 标记使用 integer 槽），并按 Mac Mouse Fix 的 200/500ms 策略重发 Ended；generation 令牌会在新手势或进程退出时取消旧重发。
- [ ] K6. 在 macOS 25/26/27 的真实 MacBook/Magic Trackpad 与 Finder/Preview/Safari/Numbers 上完成联合拖拽验收。
- [ ] F5. 在目标 macOS 版本用真实 MacBook/Magic Trackpad 验证 performer 是否可用、触发时机和系统“触控反馈”开关；未完成前不宣称硬件 click parity。
- [x] G1. 通过 AnySearch + Ketch 完成 Android/Apple/开源触觉检索，确认本机 primitives 能力与系统强度约束。
- [x] G2. 深按条改为“落指普通 click + 阈值后的更深冲击”两阶段反馈，使用幅度可控的短 waveform 与 `USAGE_PHYSICAL_EMULATION`，保留能力降级路径。
- [x] G3. APK 增加“按下震动强度”参数（40–255），完成 Gradle 单测/构建、ADB 安装与 `dumpsys vibrator_manager` 实机日志验收。
- [x] G4. 根据首次体感反馈修正深按条时序：`ACTION_DOWN` 立即发普通 click，阈值后只发一次带阻尼尾的更深冲击，避免“延迟的一次震动”错觉。
- [x] G5. 对 Xiaomi/Redmi + 支持 163 的 HAL 优先使用系统桌面同款 `HARDWARE_FEEDBACK` 预置效果，其他设备继续走可移植波形。
- [x] F5c. 为 `companion-net` 增加虚拟输入偏好合并路径：忽略 Mac mini 残留的 `Clicking=0`，默认保留手机 tap-to-click，同时保留显式 TOML 关闭能力。
- [x] I20. Android 资源消耗收敛为单一平衡方案：QWEA0 玻璃保留完整动态背景、色差、色散、传感器高光和触点光学层，仅在手指交互期间开启动态重绘；背景采样使用 `globalDownsampleFactor=0.5`、`downsampleScale=3`、优化捕获和降采样模糊，色差/色散中间缓冲使用 0.35 降采样；关闭帧统计；壁纸解码最长边限制为 1600px；不增加高质量/省电/兼容三档设置。
- [x] I21. Android API 31+ 默认切换到自有 `GpuGlassView` 单场景 AGSL 合成器：背景只保留一张半分辨率 Bitmap，单个 RuntimeShader 在一次绘制中完成 SDF 凸面折射、RGB 色散、边缘高光和触点光源；现有 QWEA0 仍作为 API 26–30 兼容回退。触点质心、全屏圆角和自定义折射/色散/饱和度/高光参数均与共享输入和设置链路同步，不新增质量模式。

### 4.1 本轮验证记录

- `~/.cargo/bin/cargo test --workspace`：通过，155 个库测试 + 3 个 `companion-config` 测试 + 6 个 `companion-tui` 测试 + 9 个协议测试（包含 R1 transform filter、R7 两指误触回归、C1 bit-packed decode、C2 descriptor report-ID、C3 fixture、C4 hybrid 聚合、无编号 report、虚拟输入 `Clicking=0` 隔离回归，以及 J3 Option Zoom mask、J4 Shift 轴向、J1 配置映射和 J6 横向总开关回归）。
- `~/.cargo/bin/cargo check --all-targets`：通过。
- `~/.cargo/bin/cargo check --target aarch64-apple-darwin --all-targets`：通过（交叉检查 macOS binary/module wiring；仅有既有 dead-code 警告）。
- `~/.cargo/bin/cargo run --bin companion-tui -- --help`：通过，TUI binary 可构建并暴露 `--config` 覆盖入口。
- 修饰键事件回归审查：点击 down/up 使用同一 modifier 快照；pinch/rotate/swipe/scroll 保留当前四键 flags；延迟右键保存触控抬起时的快照；Cmd+Ctrl+D、Control+Arrow 与 SymbolicHotKey 系统动作使用固定注册 chord，避免额外实时 modifier 让 WindowServer 拒绝匹配。
- Shift 兼容边界回归审查：映射在横向总开关之后生效，`[scroll].horizontal = false` 不会被兼容模式绕过。
- 联合拖拽回归审查：现有 `three_finger_drag_to_four_finger_swipe` 和 `link_timeout_releases_drag_button_carried_into_four_finger_swipe` 通过；输出层新增 SymbolicHotKey 阈值、350ms 冷却、80ms flick 保护和 Drop 时 fallback 状态隔离，macOS 行为仍待真机。
- 联合拖拽追加回归：配置的拖拽锁定窗口内，3F 全抬起后快速落 4F 会继续保持左键并启动 Space swipe；显式 `release_delay_ms=0` 会立即结束拖拽；五指/掌托接触不会推进四指 Space 手势。
- 2026-08-29 K1b 修订：Mac Mouse Fix 与 dockswipe 的 macOS 26 配方显示 DockControl 的 header/phase/axis 字段必须写 double 槽；本项目此前写入 integer 槽，导致日志显示事件已发出但 Dock 不消费。已按来源修正，并加入 200/500ms Ended 重发；重发由手势 generation 取消，避免旧 Ended 取消新一轮橡皮筋动画。K1b 同时将“携带左键拖拽”的 macOS 26 Space handoff 自动路由到 SymbolicHotKey，独立四指 swipe 仍保留连续 DockSwipe。
- 2026-08-29 修订：根据 Mac Mouse Fix 后续运行时版本修订，macOS 26 的独立 swipe 保留连续 DockSwipe；携带左键拖拽的 handoff 使用 SymbolicHotKey 规避 sender 校验，macOS 27+ 将 `SLEventSetIOHIDEvent`/`HIDEvent` 作为独立 swipe 的优先路径，仅在不可用时切换 SymbolicHotKey。
- red-green：临时移除 `policy_for_mode(..., virtual_input=true)` 的隔离时，`virtual_input_keeps_tap_to_click_default` 按预期失败（`Off` vs `On`）；恢复后虚拟输入两项回归均通过。
- `android/./gradlew test`（工作目录 `android/`）：通过，Gradle `BUILD SUCCESSFUL`。
- `android/./gradlew assembleDebug`：通过；`adb install -r android/app/build/outputs/apk/debug/app-debug.apk` 返回 `Success`，已在设备 `192.168.3.131:34743` 启动 `com.mtc.touchpad/.MainActivity`。
- I20 资源回归（2026-08-29）：使用 ARM AAPT2 完成 `testDebugUnitTest assembleDebug`，并在 `192.168.3.137:44899` 重装启动。主页稳定后 `dumpsys meminfo` 为 PSS `187.1 MB`、Graphics `108.7 MB`、GL mtrack `93.7 MB`、EGL mtrack `15.0 MB`；`dumpsys cpuinfo` 为 `1.4%`。执行 5 秒交互滑动时，CPU 快照约 `11.1%`、GPU p50 `8 ms`、janky `2.12%`，完整动态镜头正常渲染；壁纸纹理约 `1200x797`/`3.65 MB`，没有 4K 级原图纹理。此前约 `300 MB` PSS 的记录包含多个 Dialog，不能作为严格同场景 A/B；当前采用单一平衡配置，不提供三档质量 UI。
- G3 追加验收：深按条触发后 `dumpsys vibrator_manager` 显示 `usage: PHYSICAL_EMULATION`、`adaptive=1.00`，实际播放 `[0ms@0, 10ms@0.68, 6ms@0, 18ms@1.00]`；设置页显示 `按下震动强度 255 / 255`。
- G4 代码验收：`DeepPressBarView.ACTION_DOWN` 立即调用 `Haptics.click()`；阈值回调调用一次 `Haptics.deepPress()`，新深按波形为 `[0ms@0, 11ms@1.00, 4ms@0, 8ms@0.34]`。
- G5 实机验收：Redmi/HyperOS 触发后日志显示 `usage: HARDWARE_FEEDBACK`、`Prebaked=163(MEDIUM, with fallback)`，确认厂商校准路径已被选中。
- `git diff --check`：通过。

### 4.3 设置界面产品化（2026-08-29）

- [x] L1. TUI 重构为 Apple 原生三段导航：`Point & Click`、`Scroll & Zoom`、`More Gestures`；虚拟输入独有的三指拖移、换手延迟、加速度曲线、后端选择和系统同步集中在 `Companion` 扩展区。`Click` 与 `Quiet Click` 保留为明确的硬件专属只读行。
- [x] L2. TUI 增加中文/英文切换（`l`），侧栏/详情双焦点、`Tab` 分组切换、原子保存、重载和未保存退出确认；原生文案和解释取自 Apple Support 的公开页面。
- [x] L3. 新增 `companion-config dump/set` JSON/TOML helper。helper 复用 `config::Config` 的 serde 模型，保留未编辑字段，并通过临时文件 + rename 原子写入；`scroll.modifier_zoom_mask=0` 恢复默认并删除该键。
- [x] L4. 新增 `macos/TrackpadCompanionSettings` SwiftUI package（macOS 13+）：`NavigationSplitView + Form`、系统字体/语义颜色、深色模式、中文/英文切换、原生三段设置和 Companion 扩展；通过 helper 读写，不复制 TOML 解析逻辑。
- [ ] L5. macOS 真机编译与窗口验收：Mac mini/有无触控板、13/14/15/26 系统、深色模式、VoiceOver/键盘导航、窗口恢复和 helper 签名/打包。

#### L 阶段设计依据

Apple 官方 Trackpad 设置页面固定使用三组结构，并明确“内置或已连接无线触控板”是显示 Trackpad 面板的前提：

- <https://support.apple.com/guide/mac-help/change-trackpad-settings-mchlp1226/mac>
- <https://support.apple.com/en-us/102482>

LinearMouse 的开源配置文档也建议常规修改优先使用 GUI、把 JSON 配置作为高级/自动化接口：
<https://github.com/linearmouse/linearmouse/blob/main/Documentation/Configuration.md>。
本项目沿用这一边界，但 helper 的真实格式仍是现有 TOML，以保证 companion-net、TUI、GUI 共享同一契约。
Ketch 的开源代码检索还核对了 `NavigationSplitView` 在 macOS 设置类应用中的实际用法，以及多个 dotfiles 对 `TrackpadThreeFingerDrag` 的写入方式；这些项目支持“原生分组 GUI + 可审计配置桥接”的架构，但不证明 `defaults` 能在无硬件 Mac mini 上注册 Trackpad 面板。
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
- 阶段 C：部分完成（C1-C5 的代码与合成 fixture 已完成；真实 HID descriptor/report 采样与 macOS 设备验收仍待执行）
- 阶段 D：待真机
- 阶段 E：部分完成（已收集 raw MultitouchSupport 资料；虚拟 HID 成本评估与架构决策待后续）
- 阶段 F：部分完成（F1-F4、F5a/F5b/F5c 已完成；haptic performer、私有事件 payload 和真实 Mac 应用矩阵仍待真机）
- 阶段 H：已完成（已核实无受支持的强行显示方案；`companion-tui` 已作为 Mac mini 的配置入口，原生 pane 仍需真实触控板硬件）
- 阶段 G：已完成（Android 深按触觉研究、可调短波形与 ADB 日志验收；不同手机的主观强度仍需设备矩阵 A/B）

- 阶段 I：已完成（Apple 风格产品级 Android/Web 壳、连接/设置分层、玻璃 chrome、触控区高对比、响应式与无障碍降级；真实 companion-net WebSocket 与 macOS 应用矩阵仍待目标主机）
- 阶段 J：部分完成（J1-J6 代码、自动化测试和配置入口已完成；Control/Option/Command Zoom、四种 modifier click/drag 及 Shift 两种模式仍待 macOS 应用矩阵真机验收）
- 阶段 K：部分完成（K1-K5 代码和自动化状态机回归已完成；K6 的 macOS 25/26/27、Finder/Preview/Safari/Numbers 联合拖拽验收待真机）
- 阶段 L：部分完成（TUI 中英/原生分组重制、共享 JSON/TOML helper、SwiftUI GUI 工程骨架已完成；macOS 真机编译、打包与无障碍验收待执行）

### 阶段 I：Apple 风格成品化 UI 重制（2026-08-29）

设计规范：`docs/apple-product-design-spec.md`

- I1：已完成。审计 Android `MainActivity` 与 Web `touchpad.html`，确认现有功能完整但连接字段与工具按钮挤在主屏，触控区层级不够突出。
- I2：已完成。检索 Apple Materials/HIG、Liquid Glass、Material 3、Mousedroid 及开源 Apple HIG skills，锁定“内容层高对比 + 功能层少量玻璃”的跨平台方案。
- I3：已完成。安装并直接调用 `implement-apple-web-ui`、`review-hig-compliance`、`design-macos-interfaces`、`apple-design-foundations`、`apple-design-materials`、`apple-design-interaction`、`apple-design-web`、`apple-design-motion`、`material-3` skills。
- I4：已完成。Android 保持原生 View/Activity 与触控链路，完成 app bar、操作栏、连接面板、深按设置面板和主题资源重制。
- I5：已完成。Web 保持二进制协议与 pointer capture，完成统一顶栏、连接态收缩胶囊、产品标识、状态层级、fullscreen 同步和可访问性降级。
- I6：部分完成。ADB 与 Playwright 截图验收覆盖 Android 横屏、Web 375px/1280px、诊断页两种尺寸；静态 server 无 `/ws`，实时 WebSocket 仍需 macOS companion-net 实例验收。
- I7：已完成。README、Apple 产品设计规范和本规划书已回写；GitHub 发布前仍需真实 macOS 应用矩阵与网络安全复核。
- I8：已完成。调研并核对 `QWEA0/Liquid-Glass-Android`（MIT、JitPack `com.github.QWEA0:liquidglass:v2.0.2`）与 `PallavAg/liquid-glass-web-react`（MIT、SVG 位移图）后，Android 触控面接入 QWEA0 的 API 33+ AGSL/SDF 折射、物理色散、边缘高光和透明 `CLEAR` 材质管线；Web 触控面接入同类几何位移滤镜并保留 `backdrop-filter` 降级，双方均无空闲背景动画。
- I9：已完成并收敛。主题矩阵统一为 7 种 Liquid Glass（`light-glass`、`dark-glass`、`ocean-glass`、`sunset-glass`、`aurora-glass`、`graphite-glass`、`custom-glass`）、6 种编辑器主题（`tokyo-night`、`nord`、`dracula`、`solarized-dark`、`catppuccin-mocha`、`monokai`）以及经典/辅助主题（`classic-light`、`classic-dark`、`high-contrast`）；实验材质从正式入口移除，历史值回退到默认玻璃。Android 使用 SharedPreferences，Web 使用同一 `localStorage` key；Android 使用 QWEA0 `CLEAR` 透镜，Web 使用 SVG 位移透镜，均不运行空闲背景动画。
- I10：部分完成。API 36 Redmi 实机已重新安装并启动 QWEA0 v2.0.2，截图确认全窗口动态后景、SDF 玻璃宿主、边缘高光和触控层无崩溃；Android/Web 触点已加入速度拖尾、方向高光、双层光学环和 280ms 松手衰减；Safari/Firefox 的 SVG `backdrop-filter` 仍按公开 WebKit 限制走 blur fallback，需目标浏览器和 macOS 主机做最终矩阵。
- I11：已完成。Android 端补齐材质实验室主题（凝露、触控水波、雨痕、棱镜、软胶、液态金属、纸张、全息、LCD、陶瓷），材质层不接收触控；材质响应与“触点显示”分离，默认仅由触摸事件驱动有限响应；顶部/底部 chrome 收紧，全屏零边距零圆角，Activity 使用 `fullSensor` 支持竖屏。2026-08-29 在 `192.168.3.137:44899` 完成安装、启动、主题滚动和竖屏截图回归。
- I12：已完成（2026-08-29）。Android 全屏隐藏整个底部轨道并关闭系统导航栏 contrast scrim，Web 删除 `glass-sheen` 无限动画并让全屏触控面移除 inset；QWEA0 触控面改为 `CLEAR` + 透明子层以保留背景可辨识度。APK 已在 `192.168.3.137:44899` 安装，单元测试和构建通过。

### 阶段 I 视觉收尾（2026-08-29）

- I13：已完成。按中央触控面视觉回归移除 Android 背景中的圆形、圆角块和大曲线带，改为与 Web 对齐的连续线性色场和低强度镜面层；清理 `Quiet Glass` 的离散装饰，收紧 Android 控制栏内边距，QWEA0 保持透明度更合适的 `CLEAR` 材质。重新完成 Web 静态回归；Android 真机复验受当前构建容器的 AAPT2 运行时限制，保留上一轮已安装 APK 的实机结果。
- I14：已完成（2026-08-29）。完成 Web/APK 主题 parity 收尾：两端正式选择器与 tester 共用 16 个主题 key；Android 六种编辑器主题、经典主题和辅助主题均由 `ThemePalette` 驱动，设置/测试弹窗、顶部/底部 chrome 与触控面边框跟随当前 palette；Liquid Glass 触控面统一采用 Android QWEA0 `CLEAR` 透镜与 Web SVG/CSS 折射近似，且保留无滤镜/无动画降级路径。当前环境仍无法启动 x86_64 AAPT2（ARM 容器缺少 `/lib64/ld-linux-x86-64.so.2`），已改用 Kotlin 编译排除资源任务和 Web 语法/浏览器回归验证。
- I15：已完成（2026-08-29）。按连接态重排 Web 与 APK 触控页：原底部操作 dock/controlsRail 合并至统一顶栏，成功连接时仅保留可识别的状态胶囊；详细控制移入可纵向滚动的控制中心，顶栏和触控面均不依赖横向滚动；断开或连接中保持展开以提供恢复路径；Web 触控面上移至 `top: 72px/bottom: 24px`，Android 触控面底部释放为 18dp；全屏继续使用独立退出按钮，不抢占触控区。
- I16：已完成（2026-08-29）。使用 ARM AAPT2 override 完成最新 APK 打包并安装到 `192.168.3.137:44899`，ADB 返回 `Success`；启动截图确认 Android 连接态 header 已收缩为状态胶囊，触控面保留完整可用区域。
- I17：已完成（2026-08-29）。视觉回归移除其他主题右下椭圆落地阴影，并通过主题专属背景纹理与 `pad-texture` 规则表达编辑器/经典主题；非玻璃主题保持纯色高对比，Liquid Glass 不叠加该阴影，避免层次冲突。

- I18：已完成（2026-08-29）。按窄屏产品化回归重排 Web 与 Android 顶栏：顶栏固定为连接状态、控制中心和全屏三个入口，移除移动端横向滚动依赖；灵敏度、震动、触点显示、深按、诊断、主题和命令收进分组控制中心/对话框。非玻璃与编辑器主题移除右下椭圆阴影，改为各主题对应的网格、纸面或低强度纹理背景；Android Ceramic 同步改为主题色网格。
- I19：已完成（2026-08-29）。修复 Android 连接态紧凑顶栏中全屏按钮因 `LinearLayout` 宽度不足被裁切的问题，加入 44dp 独立命中区和轻量 elevation；全屏进入/退出使用可中断的缩放、淡入和位移过渡。Android 所有模态设置打开时隐藏底层控制中心/全屏入口，Web 控制中心打开时隐藏对应顶栏操作；新增 `HeaderLayoutMetricsTest` 防止紧凑胶囊宽度回退。
- I20：已完成（2026-08-29）。Android QWEA0 玻璃保留完整动态光学栈，交互期间开启动态背景重绘，抬指后停止常驻循环；全局 0.5 降采样、三级模糊降采样、优化捕获、0.35 色差/色散降采样和关闭帧统计共同控制资源；自定义/内置壁纸解码最长边限制 1600px，Activity 销毁时释放位图。ADB 交互回归显示 CPU 约 11.1%、GPU p50 8ms；主页稳定基线 PSS 187.1MB、Graphics 108.7MB，没有新增质量模式设置。

### I21：Android 单场景 GPU 合成器（2026-08-29）

- 状态：已完成。API 31+ 默认使用 `GpuGlassView` 单次 AGSL 合成，QWEA0 保留为 API 26–30 回退；`TouchPadView` 只增加质心位置回调，输入编码和手势语义不变。
- 验证：最终 Debug APK SHA-256 为 `b7486d1a12c4774d00a8b20eeccc064cbfd2bf22cb5794136c82ef9fc40d4da1`，ADB `192.168.3.137:44899` 安装/启动无崩溃；干净主页 PSS `92.5–93.4 MB`、Graphics `29.6 MB`、GL mtrack `13.4 MB`、EGL mtrack `16.2 MB`；交互滑动 GPU p50 `8 ms`、90th `9 ms`、janky `0.08%`，`top` 瞬时 CPU 约 `4%`。空闲 3 秒 gfxinfo 帧数保持不变；全屏稳定后渲染器尺寸为 `2712x1220`，无旧边距暗边。

### 阶段 M：再次全面 native parity 审计（2026-08-29）

审计正文：[`native-parity-audit-2026-08.md`](native-parity-audit-2026-08.md)。本阶段重新核对 Apple 官方事件/设置语义、MultitouchSupport 逆向、Trident/LinearSwipe/Remote Pad 等开源实现、当前代码路径和 Git 历史，并将结论分成 A/B/C/D/T 五级。

- [x] M1. 重新抓取并核对 Apple `phase`、`momentumPhase`、magnification/rotation 增量、scroll lock 和 Trackpad pane 硬件前置条件。
- [x] M2. 核对 OpenMultitouchSupport、mactic、CalfTrail/Hammerspoon/Mac Mouse Fix、Trident/LinearSwipe、Remote Pad/Android NSD 的实际实现和限制。
- [x] M3. 对 `macos_preferences.rs`、`gesture.rs`、`output.rs`、wire protocol、Android/Web 输入和 GUI/打包路径完成参数/事件矩阵审计。
- [x] M4. 审查 `18e96a8`、`20f2206`、`2da296f`、`11cf06d`、`4e94503` 及最近产品化提交，识别经验倍率、私有 ABI 和产品安全边界的演进。
- [x] M5. 运行 Rust/Android 自动化回归并记录 Clippy 基线债务；不把 portable recorder 的通过升级成 Mac 应用消费证明。
- [ ] M6. 在 macOS 目标主机用 recorder 完成 Smart Zoom 单路径、HID/session tap、parent/child payload 和首帧 delta A/B。
- [ ] M7. 在 Preview、Photos、Safari、Maps、Figma、Finder、Numbers、Mission Control、Spaces 完成 tap/scroll/momentum/pinch/rotate/cancel/reverse/联合拖拽矩阵。

#### M 阶段执行回写

- 阶段 M：**部分完成**（M1-M5 研究、代码静态审计和自动化验证已完成；M6-M7 依赖真实 macOS、目标系统版本、Accessibility/Input Monitoring 和应用矩阵）。
- 当前发布判断：允许 beta/GitHub 开发版；不宣称 Apple 原始 Multitouch/Force Touch parity。
- 已关闭的 P1：Smart Zoom 重复投递、默认 Pan→transform 违反 scroll lock。
- 剩余 P1/P2：Notification Center edge 坐标归一化、未知偏好枚举静默启用、未认证默认 LAN 监听，以及所有依赖真机的私有事件矩阵。

### 阶段 N：逆向参数落地与变换路径收敛（2026-08-29）

依据：`docs/reverse-engineering-sources.md`，以及 CalfTrail/Touch、Hammerspoon、Mac Mouse Fix 的公开源码和文档。

- [x] N1. 将 `gestures.pinch.gain`、`gestures.rotate.gain` 加入 TOML，范围归一化为 `0.25..4.0x`，默认 `1.0x`；倍率在 transform safety limiter 之前生效，不会绕过单帧上限。
- [x] N2. 将变换倍率接入共享 `GestureOptions`，因此 HID 与 network 两条输入入口使用同一套参数；TUI 增加中英双语调节项。
- [x] N3. 新增 `gestures.dynamic_transform_compat`，默认关闭。默认 2F scroll 一旦锁定不再转成 pinch/rotate；旧转场逻辑仅在显式兼容模式启用。
- [x] N4. Smart Zoom 收敛到 Mac Mouse Fix 参考的 type 29、subtype 22、单 HID tap，移除 type 32 和 HID/session 双重投递，降低重复触发风险。
- [x] N5. 新增逆向项目、来源 URL、发现字段、许可证状态和“不可直接复制私有 ABI”说明；CalfTrail/Touch、mactic、LinearSwipe、Remote Pad 等未暴露 SPDX 的项目已标记为待核验。
- [x] N6. Rust workspace 回归：147 个核心测试、3 个 companion-config、5 个 companion-tui、9 个协议测试全部通过。
- [ ] N7. macOS 真机仍需验证 gain 手感、单路径 Smart Zoom 消费结果，以及 strict/compat 两种模式在 Preview、Photos、Safari、Maps、Figma 的结果。

阶段 N：**部分完成**（代码、TUI、文档和自动化测试已完成；N7 依赖目标 macOS 真机与应用矩阵）。

### 阶段 O：跨平台触控板逆向与参数对照（2026-08-29）

依据：`docs/reverse-engineering-sources.md` 新增的跨平台资料，包含
libinput、Windows Precision Touchpad、VoodooRMI、OpenMultitouchSupport 和
Apple Mac 触控板的公开实现/规范。

- [x] O1. 核对 libinput 的 pinch/rotate 合同：相对角度增量、相对初始位置的绝对 scale、中心位移、固定手指数和 100/150ms 锁定超时。
- [x] O2. 核对 Windows PTP 的 Contact ID、Tip、Confidence、Width、Height、Pressure、最大触点数和溢出生命周期规则。
- [x] O3. 核对 Windows 的 `CursorSpeed`、`ClickForceSensitivity`、`FeedbackIntensity` 等真实用户参数，确认没有可迁移的旋转/缩放 gain。
- [x] O4. 核对 Synaptics/VoodooRMI/OpenMultitouchSupport 的压力、接触面积、边缘区和拇指/手掌粘性分类策略。
- [x] O4a. 抓取 ChromiumOS `ImmediateInterpreter` 与 `PalmClassifyingFilter` 的源码默认值，记录 pinch 锁定、尺度分辨率、角度余弦、滤波器和 palm 参数及单位。
- [x] O4b. 核对 Fusuma 的 `threshold`/`interval` 优先级和倍率语义，确认它不是物理灵敏度曲线。
- [x] O4c. 核对 Synaptics man page/样例配置的 Finger、Palm、Hysteresis、Coasting、LockedDrag 和 SoftButton 参数，区分驱动默认与设备样例。
- [x] O5. 将可迁移结论回写到架构决策：保持平台中立的 transform core，保留 `confidence`，不伪造 width/height/pressure，继续把 `pinch.gain`/`rotate.gain` 标为 Companion-only。
- [x] O6. 将来源、许可证状态、具体参数速查表和不可验证项写入 `docs/reverse-engineering-sources.md`，不复制第三方私有 ABI 或未核验许可证代码。
- [ ] O7. 在真实 Mac、Windows PTP 和 Linux libinput 设备上采集同一套 pinch/rotate/scroll 轨迹，用于参数分布对照；当前依赖外部硬件，不能在本环境完成。

阶段 O：**部分完成**（公开规范、源码和社区实测已整理；跨设备同动作采样与 macOS 应用消费结果待真机）。

### 阶段 P：ChromiumOS 参数实验档案（2026-08-29）

- [x] P1. 将公开 ChromiumOS 手势识别器的可迁移阈值封装为独立
  `native` / `chromium_os` profile；默认保持 Companion 原生校准值，不覆盖既有用户行为。
- [x] P2. 应用 ChromiumOS 的 1.5mm 滚动锁定、2mm pinch guess、8mm pinch certainty、
  三帧观察窗口和约 0.25% 缩放更新死区；旋转仍沿用已验证的角度锁定与输出死区。
- [x] P3. 在配置解析、启动映射、双指 recognizer、TUI 和 `companion-config` 文档中接通
  profile，并增加解析、行为和序列化回归测试。
- [x] P4. 提供无损 A/B 切换与回退：`gestures.parameter_profile = "chromium_os"`
  或恢复为 `"native"`，改动仅在重启 companion 服务后生效。
- [ ] P5. 在目标 Mac mini + Android 触控端完成同一组慢捏、快捏、轻微旋转和滚动轨迹
  的 A/B 记录，确认误触率、锁定延迟、缩放连续性和四指手势无回归。

阶段 P：**已完成代码与自动化验证；真机 A/B 仍待执行**。

### 阶段 Q：Notification Center 右边缘判定修正（2026-08-29）

- [x] Q1. 移除右边缘识别中混用的旧归一化坐标和 28mm 绝对阈值。
- [x] Q2. 改用 `gestures.surface_width_mm * 0.85` 计算边界，默认虚拟触控面宽度为
  65mm；要求两根手指都从边缘区开始，减少中部左滑误触发。
- [x] Q3. Web 触控面把完整输入区域映射到 65mm 虚拟宽度，Android 测试动作更新到
  56/60mm 起点；增加非边缘和单指边缘回归测试。
- [ ] Q4. 在 Mac mini + Android 真机验证 65mm 默认值与不同屏幕 DPI 下的边缘手感；
  必要时通过 TUI 的“虚拟触控面宽度”微调。

阶段 Q：**已完成代码与自动化验证；真机边缘手势仍待执行**。

### 阶段 R：深按条编辑与触点显示配置（2026-08-29）

- [x] R1. Android/Web 深按条统一支持显示开关、横纵位置、宽度和高度；两端均提供可拖动预览，Android 使用 8dp 触摸布局，Web 使用 Pointer Events。
- [x] R2. “动效”产品设置拆分为仅控制触点覆盖层的“触点显示”，材质层不再随该开关关闭；旧 key 仅用于兼容读取。
- [x] R3. `custom-glass` 暴露折射强度、饱和度、亮度和边缘高光参数，保持其它主题的默认变量和减少透明度降级。
- [x] R4. 移除测试按钮自由布局入口，诊断动作保留固定顺序，避免测试结果受自定义布局影响。

阶段 R：**已完成代码、脚本语法检查和 Android 构建安装；目标 Mac 上的深按按压时延及不同应用消费结果仍属于 M6/M7 真机矩阵。**

### 阶段 S：Android 全屏居中触控面回归（2026-08-29）

- [x] S1. 定位并修复全屏分支清零 `padHost` 外边距、`padFrame` 内填充和玻璃圆角造成的边到边渐变与黑边。
- [x] S2. 全屏仅隐藏顶部 chrome，触控面继续使用普通模式的居中边距、圆角、内填充和边缘高光。
- [x] S3. 增加 `PadLayoutMetrics` 回归测试；在 `192.168.3.137:44899` 完成重装、启动和全屏截图检查。

阶段 S：**已完成代码、单元测试、构建、ADB 安装和真机视觉回归。**

### 阶段 T：分阶段拖拽与合成快捷键补全（2026-08-29）

- [x] T1. 将三指拖拽的完整抬手建模为 `DragLocked`，实现
  `3F -> 0F -> 4F -> 0F -> 3F`：四指阶段不释放左键，目标 Space 再落三指可继续同一拖拽。
- [x] T2. 明确解锁与安全边界：静止 1F/2F 触摸解锁，真实 1F/2F 移动立即回到普通输入；加入第四指前未完整抬手时释放 live drag；断链、取消和进程退出始终释放按键。
- [x] T3. 新增 `persistent_drag_lock` 配置及 TUI 中英双语开关；关闭后保留有限
  `release_delay_ms` 兼容模式，设为 `0` 即严格抬手释放。
- [x] T4. 修复快捷键与触控板组合的时序边界：面向 App 的点击/滚动/Pinch/Rotate/拖拽传递实时 Quartz 修饰位；延迟右键保留触控抬起快照；SymbolicHotKey、Control+Arrow 与 Cmd+Ctrl+D 使用各自固定注册 chord，避免 WindowServer 因额外修饰位拒绝系统动作。
- [x] T4a. 增加 `click_with_modifiers` 输出契约和回归测试，覆盖“按住 Option+Shift 双指点按、抬指后松开键、延迟窗口再确认右键”以及单指双击第二次点按延迟的真实时序。
- [x] T5. 增加状态机、配置映射、TUI 序列化和纯逻辑修饰键回归测试；Linux workspace 测试与 `aarch64-apple-darwin` 交叉检查通过。
- [ ] T6. 在 MacBook/Magic Trackpad + Mac mini 目标环境实测 staged 拖拽、快捷键组合及不同 WindowServer 版本的实际消费结果。

阶段 T：**代码、自动化回归和文档已完成；T6 依赖 macOS 真机。**

### 阶段 T1：持久拖拽后的单指可用性修正（2026-08-29）

- [x] T1.1. 定位 `DragLocked` 吞掉 1F/2F dispatch 的原因；静止触摸仍保留换把候选，超过 `0.4mm` 的单指移动或双指 pan 立即退出锁定。
- [x] T1.2. 退出时释放合成左键、清理 tap-drag/右键残留，并抑制解锁动作在抬手时生成额外单击。
- [x] T1.3. 增加“拖拽后单指恢复指针”回归测试，验证移动、单次释放和无误点击。

阶段 T1：**代码与自动化回归已完成；真机停顿体感仍待验。**
