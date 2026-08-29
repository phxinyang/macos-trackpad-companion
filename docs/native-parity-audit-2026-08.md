# Native Trackpad Parity Audit (2026-08)

## 0. 审计结论

审计对象是 `codex/native-trackpad-productization` 当前代码、最近 Git 记录、已有
`native-parity-matrix.md`/`native-parity-execution-plan.md`，以及 Apple 官方资料、
MultitouchSupport 逆向项目、手势工具和远程触控板项目。

结论很明确：项目已经是一个可用的用户态远程输入产品，但还不是 Apple 原生
Multitouch 输入栈的等价实现。基础光标、点击、滚动和网络保护达到 **native-like
应用层可用**；pinch/rotate、系统手势和触觉属于 **有条件近似**；压力、悬停、掌托
拒绝、Force Touch 和系统设置面板属于 **硬件/驱动边界**。后续发布说明必须沿用
下面的等级，不再使用未经真机证据支持的“完全原生”。

| 等级 | 含义 | 当前范围 |
|---|---|---|
| A | 公开语义、生命周期和输出字段基本一致；仍需目标 Mac 验收 | scroll phase/momentum 字段形状、鼠标 click/drag 生命周期的设计 |
| B | 应用层行为接近原生，但依赖 AppKit/WindowServer 是否接受合成事件 | cursor/click/scroll、dictionary、三指拖拽、modifier flags |
| C | 兼容映射或私有 payload；可用性依赖系统版本/应用 | pinch/rotate、Smart Zoom、DockSwipe、Launchpad、Mission Control、scaling |
| D | 当前输入源没有相同硬件/驱动语义，不能实现等价行为 | pressure、hover、palm/resting、真实 Force Click、Apple Trackpad pane |
| T | 代码已有路径，但没有目标 macOS/应用矩阵证据 | 所有私有事件跨应用结果、haptic performer、联合拖拽 |

### 发布判断

- **可以继续作为 beta/GitHub 开发版发布**：光标、单击/右键、普通滚动、断链收尾、
  Android/Web 连接和配置入口都有自动化覆盖。
- **不能宣称 Apple 输入流 parity**：网络协议的 contact 记录只有
  `id/tip/confidence/x/y`（另有帧级 scan time/button），没有 pressure、contact area、
  axis、angle、hover 状态或真实 digitizer child events。
- **Mac 真机是当前最大的验证缺口**：Linux portable recorder 只能证明状态机调用顺序，
  不能证明 Preview、Photos、Safari、Maps、Figma、Mission Control 或 Spaces 消费了
  这些事件。

## 1. Apple 原生能力模型

Apple 的 [Handling Trackpad Events](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/EventOverview/HandlingTouchEvents/HandlingTouchEvents.html)
把输入分为三层：低层多点 touch、驱动解释后的 gesture，以及 mouse/scroll 事件。
手势通过 `NSResponder` 的 `magnifyWithEvent:`、`rotateWithEvent:`、`swipeWithEvent:`
传递；gesture sequence 必须处理 `Began`、`Changed`、`Ended` 和 `Cancelled`。

几个对本项目最关键的合同：

1. magnification 和 rotation 是**相对增量**，应用将每一帧的变化量累加到当前状态。
2. magnify/rotate 可以在同一多点接触中切换；scroll 和 swipe 一旦开始则锁定到该手势，
   直到结束或取消。
3. scroll 是 `NSScrollWheel`，同时有 `phase` 和 `momentumPhase`。非动量滚动路由到
   当前指针下的 view；动量滚动继续路由到 flick 开始时的 view。
4. scroll 可能先发 `mayBegin` 再发 `began`；动量段使用 `momentumPhase = began/changed/ended`，
   而不是把最后一帧普通 scroll 当作动量尾巴。
5. 触控板设置页只在有内置触控板或连接无线触控板时显示。Apple 的
   [Change Trackpad settings on Mac](https://support.apple.com/guide/mac-help/change-trackpad-settings-mchlp1226/mac)
   明确说明了这一前置条件。

当前 `Phase` 只有四种值，没有 `MayBegin`；这是一个可记录但不应掩盖的协议差异。项目
   通过延迟 2F 锁定减少误触，但它并不等价于 Apple 驱动的 `mayBegin` 事件。

## 2. 参数能力矩阵

下表将 Apple 设置分为“直接等价、应用层近似、兼容映射、硬件专属”。“同步”表示启动
时读取快照，不表示写回系统，也不表示输入驱动会采用同一内部参数。

| Apple 设置/偏好 | 当前实现 | 等级 | 差异与证据 |
|---|---|---:|---|
| Tracking speed | `.GlobalPreferences` 的 `com.apple.trackpad.scaling` 映射到 `cursor.sensitivity` | C | 代码明确使用观察到的 `0..3 -> 0.5..2.0x` 有界映射；Apple 没公开 px/mm 公式，不能称物理等价 |
| Click pressure / First/Second threshold | 仅记录 `FirstClickThreshold`、`SecondClickThreshold` 为 unsupported | D | 网络输入无压力，无法重建按压力度 |
| Quiet Click | 未驱动 | D | 这是实体 Trackpad 声音/驱动行为，不能由 CGEvent 恢复 |
| Force Click and haptic feedback | `ActuateDetents` 控制 `NSHapticFeedbackManager.defaultPerformer()` | B/D | 只能发 Generic/Alignment/LevelChange 确认；不能产生压力转场或 Force Touch 事件 |
| Tap to click (`Clicking`) | 本地 HID 入口映射；virtual input 忽略物理域中的残留 `Clicking=0` | B | 对手机 surface 合理，但与实体触控板开关不是同一语义；Mac mini 的旧 plist 不能决定手机是否能点按 |
| Secondary click | `TrackpadRightClick` -> 2F tap right click | B | 判定由本项目状态机完成，未与 Apple driver 的 corner/两指细节做真机 A/B |
| Natural scrolling | `.GlobalPreferences/com.apple.swipescrolldirection` -> `scroll.natural` | B | 方向语义直接可用，仍通过合成 scroll event 输出 |
| Scroll enable/horizontal/momentum | `TrackpadScroll`、`TrackpadHorizScroll`、`TrackpadMomentumScroll` -> 三个 output 开关 | B | 开关路径清晰；事件不是 Apple 原始滚轮流 |
| Pinch / Zoom in or out | `TrackpadPinch` -> private gesture type 29/field 0x71 | C/T | 采用 CalfTrail/Hammerspoon 形状，但 parent-only digitizer、tap 位置和 app 兼容性没有目标 Mac 证据；`gestures.pinch.gain` 是 companion-only 的 0.25..4.0x 响应倍率 |
| Smart Zoom | `TrackpadTwoFingerDoubleTapGesture` -> `smart_magnify()` | C/T | 已收敛为 Mac Mouse Fix 参考的 type 29/subtype 22、单个 HID tap；仍需真机确认 Safari/Preview 是否只消费一次 |
| Rotate | `TrackpadRotate` -> private gesture type 29/field 0x72 | C/T | 使用几何 1:1 相对角度，`gestures.rotate.gain` 提供 0.25..4.0x companion-only 倍率；此前 `18e96a8` 的 2.0x 经验曲线已移除 |
| Look Up & data detectors | `TrackpadThreeFingerTapGesture` -> Cmd+Ctrl+D 脉冲 | B | 是键等价物，不是系统 raw lookup gesture；受前台 app/键盘布局影响 |
| Three-finger drag | `TrackpadThreeFingerDrag` -> left button held + `LeftMouseDragged` | B | 应用层拖拽接近原生；不携带 Apple 的三指 identity/pressure/drag lock stream |
| Dragging / DragLock | `Dragging` 只映射单指双击拖拽；`DragLock` 只诊断；三指换把使用独立 `release_delay_ms` | C | 这是有意拆分，避免把实体 DragLock 误当三指换把；两者不能宣称同义 |
| Swipe between pages | 当前没有稳定的页面 swipe 输出 | D/C | `CGEvent` scroll 不能证明 Safari/Chromium 会接受历史手势；需要 ⌘[ / ⌘] 等显式键等价物才是可控路线 |
| Four-finger full-screen app/Spaces | DockSwipe 私有 payload；macOS 26 保留连续路径，27+ 尝试 SkyLight HIDEvent，失败再用 SymbolicHotKey | C/T | macOS 版本分支和私有 ownership/phase/velocity 均待真机 |
| Mission Control / App Exposé | Vertical synthetic/notification/hotkey | C/T | 离散 notification/hotkey 不等于连续系统 rubber-band |
| Launchpad / Show Desktop | CoreDock notification + symbolic hotkey fallback | C/T | 命令能否触发不代表四指径向输入已被系统接管 |
| Notification Center | 2F right-edge candidate + symbolic hotkey 163 | C/T | 需要检查 edge zone 是否按实际 pad 宽度归一化；当前写死 mm 和一段旧 normalized fallback |
| Control-click | 传递 Control flag，交给 AppKit 解释为 secondary click | B | 符合 Apple [Right-click on Mac](https://support.apple.com/en-my/guide/mac-help/mh35853/mac) |
| Command/Option/Shift click/drag | 只传 Quartz flags，不猜应用动作 | B | Finder、Safari、编辑器的意义属于 App 层；这是正确边界 |
| Control/Option/Command + scroll zoom | `HIDScrollZoomModifierMask` 在 Began 锁定，转为 magnify | B/C | 符合 Apple Accessibility Zoom 的三种 modifier；不是普通 Trackpad 的内建手势 |
| Shift + scroll | 默认保留原轴；显式 `shift_scroll_horizontal=true` 才转换 | B | Apple 没把 Shift 列为 Zoom modifier；兼容转换不应默认开启 |
| Palm rejection / resting hand | confidence 只可从输入端传递，Mac 端没有自己的 edge/area filter | D | 网络协议没有面积/密度，Android/Web 只能自行拒绝 |
| Hover / contact pressure / axis / angle | 未进入 wire protocol | D | 与 OpenMultitouchSupport 的 raw 数据模型不具备字段 parity |

## 3. 手势与事件路径审计

| 手势 | 当前状态机/输出 | 与原生的实际差异 | 结论 |
|---|---|---|---|
| 1F cursor | `accelerate_cursor_vector` + sub-pixel carry + CGEvent mouse moved | Apple 加速度曲线未公开；项目曲线可调但不是系统同一曲线 | B，需手感 A/B |
| 1F tap/click | `TAP_MAX_DURATION=240ms`、1mm 位移门限，`click_count` 自己维护 | `DOUBLE_CLICK_INTERVAL=500ms` 与距离 25px 是硬编码，不读取用户的 Mouse double-click 设置 | B，参数需后续系统读取 |
| 1F tap-drag | 第 2 次落指延迟 200ms，移动 0.8mm 或超时才压键 | 这是合理的双击/拖拽消歧，但不是 Apple driver 的压力/drag lock | B |
| 2F secondary click | 未分类 2F 抬手后延迟确认，双击窗口触发 Smart Zoom | 具体两指间距、手掌分裂、corner secondary click 未做实体设备对照 | B/T |
| 2F scroll | `Began -> Changed -> Ended`；Q16.16 fixed point；CFRunLoopTimer inertia | 没有 `MayBegin`；momentum 是项目 EMA + 指数衰减，不是硬件/WindowServer 产生的动量流 | B/T |
| 2F pinch/rotate | 两个 admitted stream 各自 Began/Changed/Ended；相对增量限速和 deadzone | parent-only payload、无 child contacts；应用可能只接受真正 `NSTouch`/digitizer sequence | C/T |
| Pan -> transform | 在 2F pan 已开始后，`scale_rel>=0.25` 或角度阈值会发 scroll Ended，再发 pinch/rotate Began | 违反 Apple 文档的 scroll lock；这是为误分类补救的兼容策略，不是 native mode | P1 |
| 3F dictionary | `Cmd+Ctrl+D` 15ms key pulse，支持 split lift | 不等价于系统 Lookup/data-detector gesture；键盘布局和前台应用会影响结果 | B/T |
| 3F drag | `0.35mm` engage，鼠标左键保持，async lift/regrip | 原生的三指 identity、pressure、DragLock 仍不可见 | B |
| 3F drag + 4F Spaces | 进入 `FourFingerLive` 不松左键；macOS 26 保留 DockSwipe，27+ 尝试 HIDEvent/失败后 symbolic hotkey | Apple 没公开并行合同；27+ fallback 是离散跳转，不能持续 rubber-band | C/T |
| 4F swipe | centroid 累积、finger-count change 重锚、轴锁 3mm | 系统 DockSwipe 私有 ABI；无真机不能验证方向、进度、velocity、连续动画 | C/T |
| 4F radial gestures | `R/R0 <= 0.72` / `>=1.28` 触发通知或 hotkey | 离散命令不等于 Apple Launchpad/Show Desktop raw gesture | C/T |
| link/session | 250ms idle cancel、600ms peer quarantine、sender restart reset | 这是项目额外安全层，Apple 硬件没有网络故障这一维 | A/B |

### 3.1 当前最重要的行为判断

双指缩放“容易触发”的问题不能只靠继续调倍率解决。当前已经有两帧观察窗、1mm
意图位移、4% ratio、3° rotate、单帧速率上限等保护；如果仍误触，下一步应先录制
真实接触轨迹并判断是 classifier lock、输出 payload 还是应用消费问题。Apple 的公开
规则支持相对增量和生命周期，不支持本项目的经验阈值就是“原生阈值”。

## 4. 逆向与开源证据

### 私有 MultitouchSupport

- [OpenMultitouchSupport](https://github.com/Kyome22/OpenMultitouchSupport) 暴露 raw
  touch 的 `id`、position、pressure、axis、angle、density、hover/starting/touching/
  breaking 等状态，并要求关闭 App Sandbox。
- [mactic](https://github.com/MatMercer/mactic/blob/main/docs/implementation.md) 记录
  `MTDeviceCreateList`、`MTRegisterContactFrameCallback`、`MTActuatorActuate` 等符号，
  以及 Apple Silicon PAC 导致的 `dlopen/dlsym` 选择、dyld shared cache 和经验性的
  `MTDevice` offset 64。其 96-byte `MTTouch` 布局由 M3/macOS Sequoia 实测，明确警告
  会随硬件/系统变化。
- 这些项目证明“真机 raw recorder/触觉 actuator”可行，但不能证明无触控板的 Mac mini
  可以注册 Apple Trackpad pane，也不能让网络输入自动获得 pressure/hover。

### 手势工具

- [Trident](https://github.com/cyanyux/trident) 明确要求关闭或改动系统三指 Spaces
  手势，并在三指手势期间使用 scoped event tap 抑制 stray left/right click；它还把
  palm rejection、haptic、菜单栏状态、启动助手和自动更新当作成品功能。
- [LinearSwipe](https://github.com/ChilledEther/LinearSwipe) 的系统设置要求同样说明
  三指自定义手势与 Spaces 存在冲突；这支持本项目把三指拖拽/四指切桌面当作冲突域，
  而不是默认认为二者天然并行。
- [CalfTrail TouchSynthesis](https://raw.githubusercontent.com/calftrail/Touch/master/TouchSynthesis/TouchEvents.c)、
  [Hammerspoon gesture PR](https://github.com/Hammerspoon/hammerspoon/pull/2512) 和
  [Mac Mouse Fix TouchSimulator](https://github.com/noah-nuebling/mac-mouse-fix/blob/master/Helper/Core/Touch/TouchSimulator.m)
  的公开代码反复使用 gesture subtype、IOHID phase 和 magnification/rotation field，但
  不同项目在 HID tap/session tap、child touch、首帧 delta 和 DockSwipe ownership 上并不
  完全一致。它们是兼容性证据，不是 Apple 公共 API。

### 远程输入产品

- [Remote Pad](https://github.com/Gyeony95/Remote-Pad-Release/releases/tag/v1.0.0) 使用
  Bonjour 自动发现、局域网 only、自动重连、Accessibility 引导和菜单栏驻留；输入仍是
  `CGEventPost` 合成。
- Android [NSD](https://developer.android.com/develop/connectivity/wifi/use-nsd) 文档要求
  从服务解析动态端口，而不是把端口硬编码。当前项目的 `_mtc-trackpad._tcp`、token、
  手工 host/port fallback 方向正确，但 QR/token 配对和真机网络矩阵仍是发布前工作。

### Mac mini 设置面板

Apple 官方设置页明确要求真实 Trackpad。深链（例如
`x-apple.systempreferences:com.apple.Trackpad-Settings.extension`）、`defaults`、
`cfprefsd` 刷新和 `activateSettings -u` 只能打开已有 pane 或更新持久化值，不能创建
HID digitizer 能力。DriverKit 虚拟设备还需要受限 entitlement、签名、权限和分发评估。
因此 `companion-tui`/SwiftUI GUI 是正确的产品入口，伪造原生 pane 不是可靠路线。

## 5. 代码审查发现（按优先级）

### 已修复：Smart Zoom 重复投递

位置：`src/output.rs:2960-2974`。

触发条件是一次双指 Smart Zoom。此前 `smart_magnify()` 同时投递 type 29/type 32
和 HID/session 两组事件，一个动作最多经过四次投递。现在只保留 Mac Mouse Fix
公开源码中的 type 29、subtype 22、单 HID tap 形状；仍需目标 Mac recorder 验证应用结果。

影响：重复投递风险已从代码路径移除。剩余风险是某些系统版本可能只消费另一种
私有形状；这属于 M6 真机矩阵，而不是继续叠加事件的理由。

### 已修复默认值：Pan 已锁定后仍允许转成 pinch/rotate

位置：`src/gesture.rs:2513-2567`。

Apple 文档明确说 scroll 一旦开始就锁定到 scroll，直到结束；当前代码现在默认保持
该行为，只有显式打开 `gestures.dynamic_transform_compat` 才会在 `TwoFingerPan`
中依据 `scale_rel`、`frame_rot`、`total_rot` 和相对 alignment 结束 scroll，再开启
pinch/rotate。这是为早期误分类保留的兼容路径，可能在用户滚动时把手指滞后解释为缩放/旋转。

影响：原生默认不再因为滚动中的手指滞后而切入变换；兼容开关的行为仍需在 recorder
中单独验收，且不能写成 Apple 原生合同。

### P2: Notification Center edge zone 混用物理 mm 与旧 normalized 坐标

位置：`src/gesture.rs`（已在阶段 Q 修正）。

旧实现同时检查 `x >= 28.0`（物理 mm）和 `0.65..1.0`（旧版 normalized 坐标），
导致 65mm 触控面的中部也会被标记为 right-edge candidate。现已改为使用显式虚拟
surface width 的 85% 边界，并要求两根手指都在边缘区；剩余差异仅是不同发送端的
surface width 标定问题。

### P2: 未知枚举值被静默当作启用

位置：`src/macos_preferences.rs:217-221`、`normalize()` 中对
`TrackpadThreeFingerTapGesture` 和 `TrackpadTwoFingerFromRightEdgeSwipeGesture` 的调用。

`nonzero()` 将任意非零整数映射成 `true`，而 `bool01()`/`enum_02()` 会把未知值放入
`unsupported`。这与模块“未知枚举只记录并保留默认”的契约不一致；系统升级后出现新
枚举时，dictionary/right-edge 可能被错误打开而没有告警。

### P2: Smart/gesture 输出 tap 和 modifier 的真机路由未定案

位置：`src/output.rs:1694-1698`、`:1713-1715`、`:2686-2689`。

当前 pinch/rotate/scroll 主要走 session tap；Hammerspoon 的历史 PR、Mac Mouse Fix 和
CalfTrail 在不同路径上有相互矛盾的实践。session tap 对简单 `NSResponder` 可能有效，
但不能推导 `NSMagnificationGestureRecognizer`、WebKit history swipe 或 Dock 都有效。
必须用同一 recorder 在 HID/session、parent-only/child payload、首帧 0 delta/有效 delta
之间做最小 A/B，而不是继续凭调用成功调参数。

### P2: 发布默认网络面扩大注入面

位置：`src/config.rs:154-171`、`src/net.rs:190-198`。

默认 `listen_ip=None` 会绑定 `0.0.0.0`，默认 token 为空；同一 LAN 上任何能连到端口的
主机都可以注入指针和手势。README 已有警告，GUI 首次启动会生成 token，但直接运行
`companion-net` 仍是开放监听。GitHub 发布包应把 token/配对设为首启门槛，或至少默认
绑定回环并由 GUI 显式扩大到 LAN。

### P3: 成品交付仍缺 macOS 签名/真机验证

当前 `packaging/macos` 可以生成 unsigned `.app`、ZIP/DMG，SwiftUI GUI 也有 helper
监督和 Accessibility 深链；但未在本环境编译窗口、验证 VoiceOver/键盘导航、检查
Gatekeeper/签名身份、LaunchAgent 重启和不同 macOS major 版本。这个缺口不影响 beta
代码测试，但阻止“下载即用”的发布承诺。

## 6. Git 历史信号

- `18e96a8` 曾加入 2.0x rotation curve；`20f2206` 随后改为并发 stream、时间速率限幅
  和 1:1 rotation，说明经验倍率没有足够证据，应继续保持为实验参数而不是默认。
- `2da296f` 曾用 dominant stream 防止 pinch/rotate 串扰；`20f2206` 又恢复两个 admitted
  stream 的完整生命周期。两次方向变化说明真正未决的是“应用消费合同”，不是再加一层
  hysteresis 就能证明原生。
- `11cf06d` 把 Command/Control scroll 转 zoom、Shift 转横向；后续 J 阶段把 Zoom mask
  锁定和 Shift 兼容开关改为显式 opt-in，这次收窄与 Apple Accessibility 资料一致。
- `4e94503`、`8ed90d6`、`b8d6ba6`、`ef00f7c` 集中在最近一两天加入 settings sync、TUI、
  SwiftUI、监督、诊断和打包。功能面增长很快，但最新提交仍缺 Preview/Photos/Dock 的
  真机 recorder 证据；下一阶段应冻结事件合同，减少继续扩大私有 payload 的范围。

## 7. 证据与验证状态

本轮使用 Ketch Exa/Firecrawl 检索并抓取官方/开源原文；grep.app 三次 code search 返回
504/上游失败，随后切换 Sourcegraph 但返回空结果，不能把失败当作“没有实现”。已实际
核对的主要来源：

- Apple：Trackpad settings、Handling Trackpad Events、NSEvent phase/magnification/
  momentum、Multi-Touch gestures、Control-click、Accessibility Zoom。
- 逆向：OpenMultitouchSupport、mactic implementation、CalfTrail/Touch、Hammerspoon、
  Mac Mouse Fix（对应链接已在现有规划书中保存）。
- 开源/产品：Trident、LinearSwipe、Remote Pad、Android NSD。

本轮本机验证：

- `~/.cargo/bin/cargo test --workspace`：通过，144 个核心测试、3 个 companion-config、
  5 个 companion-tui、9 个协议测试，共 161 项通过。
- `~/.cargo/bin/cargo check --all-targets`：通过。
- `cd android && ./gradlew test`：通过。
- `cd android && ./gradlew assembleDebug`：通过。
- `git diff --check`：通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：失败，命中仓库已有的
  `collapsible_if`、`redundant_pattern_matching`、`approx_constant`、doc quote 和
  `bool_assert_comparison` 等 13 个 lint；本轮未扩大到全仓清理。
- macOS SwiftUI、CGEvent 实际投递、DockSwipe、Accessibility/VoiceOver 和应用矩阵：
  当前环境未完成，必须在目标 Mac 上执行。

## 8. 下一阶段执行优先级

1. **P0 真机 recorder**：记录一个输入动作在 CGEvent tap、AppKit responder 和目标应用
   的实际回调；验证已收敛的 Smart Zoom 单路径和 pinch/rotate 的 tap/child 形状。
2. **P1 原生分类模式**：默认保持 2F scroll lock；已实现显式兼容开关，补一组“滚动中
   手指滞后/轻微 spread 不会缩放”的真机回放测试。
3. **P1 参数归一化**：移除 normalized/physical 混用，按 `Layout` 或网络 surface
   明确 pad 宽度计算 edge zone；所有未知枚举统一进入 `unsupported`。
4. **P1 网络发布门槛**：首次启动生成 token、GUI/CLI 都显示配对状态；未认证时默认
   回环或明确打印安全警告并阻止后台自启。
5. **P2 应用矩阵**：Preview、Photos、Safari、Maps、Figma、Finder、Numbers、Mission
   Control、Spaces，逐项验证 tap/scroll/momentum/pinch/rotate/cancel/reverse/连续两次。
6. **P2 设置能力分层**：GUI/TUI 对每个参数显示 direct/approximate/unsupported；保留
   Mac mini 无 pane 的 companion 配置入口，不再投入 plist 伪造。
7. **P3 raw 路线决策**：完成 MultitouchSupport 真机 recorder、虚拟 HID DriverKit
   entitlement/签名/分发成本评估后，再决定是否进入真正 digitizer stream。

## 9. 审计状态

- 研究、原文抓取、代码路径、参数矩阵、Git 历史和自动化验证：**已完成**。
- 真实 Mac 的事件 recorder、目标应用消费结果、macOS 25/26/27 联合拖拽和 GUI 打包：
  **待真机**。
- 本文是当前决策基线，不等同于“原生 parity 已完成”；每个 P1/P2 关闭后应回写
  `docs/native-parity-execution-plan.md` 并附系统版本、架构、权限和录制证据。
