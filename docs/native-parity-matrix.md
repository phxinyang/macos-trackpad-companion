# macOS 触控板原生体验全功能实现对照表 (Native Trackpad Parity Matrix)

本文档是 `macos-trackpad-companion` 项目实现“对齐 Apple Magic Trackpad / MacBook 原生触控板体验”的核心工程契约与基线规范。

---

## 一、系统级手势与交互功能实现对照矩阵

| 序号 | 手势功能 (Gesture / Setting) | macOS 原生底层行为规范 (`AppleMultitouchDriver` / AppKit / WindowServer) | 项目当前实现架构与算法 | 物理与时序参数对比 (原生 vs 本项目) | 状态与验证结论 |
|---|---|---|---|---|---|
| **01** | **单指平滑位移与指针加速<br>(1F Pointer Motion)** | 速度矢量模长求模计算非线性增益，同时等比例缩放 X/Y 分量以保持方向性；维护亚像素状态防止微位移丢步与撕裂 | 采用 `accelerate_cursor_vector` 计算模长增益；次像素浮点残差累加至 `cursor_carry_*_px`；超速保护拦截 | • 原生：$v_{\text{mod}} = \sqrt{v_x^2+v_y^2}$ 计算增益<br>• 本项目：`ref=70.0mm/s`, `exp=1.35`, 极速限幅 `1200mm/s` | **自动化覆盖；待真机手感校准** |
| **02** | **单指轻点单击<br>(1F Tap to Click)** | 短触碰在抬手瞬间触发左键单击，派发 `LeftMouseDown` + `LeftMouseUp` (`click_count=1`) | `TAP_MAX_DURATION=240ms`，`TAP_MAX_MOVE_MM=1.0mm`；精确记录接触点坐标与系统纳秒级时间戳 | • 原生：时限 $\le 250\text{ms}$，位移 $\le 1.0\text{mm}$<br>• 本项目：$240\text{ms}$ / $1.0\text{mm}$ | **自动化覆盖；待真机** |
| **03** | **单指快速双击 / 多击<br>(Multi-Click Sequence)** | 连续快速轻点同一区域时，递增 `kCGMouseEventClickState`，并驱动应用打开或文本段落选择 | `record_click` 维护 $500\text{ms}$ 连击时钟与 $25\text{px}$ 移动范围；down/up 严格携带相同 count | • 原生：$500\text{ms}$ / $25\text{px}$ 判定窗口<br>• 本项目：$500\text{ms}$ / $25\text{px}$ | **自动化覆盖；待真机** |
| **04** | **单指轻点拖拽<br>(1F Tap-Drag)** | 辅助功能模式：轻点第 2 下按住并拖动；第 2 下若短促抬起则为双击，长按或滑动才锁定拖拽 | **延迟压键机制**：第 2 下落指时保持光标钉住不压键；位移 $\ge 0.35\text{mm}$ 或按住 $\ge 200\text{ms}$ 锁定拖拽，$<200\text{ms}$ 抬手直接结算为双击 | • 原生：确认时间 $200\text{ms}$，移动门限 $0.35\text{mm}$<br>• 本项目：`TAP_DRAG_CONFIRM=200ms`, `DRAG_ENGAGE_MM=0.35mm` | **自动化覆盖；待真机** |
| **05** | **双指右键轻点<br>(2F Secondary Click)** | 两指并拢轻点，抬手触发右键菜单；单指肉腹（Fat-finger）误报时自动降级单指左键；单指移动/持握时彻底隔离前置双指残留 | `TwoFingerUnclassified` 抬手触发右键；两指初始间距 $< 8.0\text{mm}$ 自动回退为单指左键；单指位移 $> 0.4\text{mm}$ 或停留 $> 150\text{ms}$ **强制清除前置 `pending_two_finger_tap` 状态残留** | • 原生间距：两指物理距离通常 $\ge 10\text{mm}$<br>• 本项目：$< 8.0\text{mm}$ 降级左键，单指动作绝对隔离 | **自动化覆盖；待真机** |
| **06** | **双指平滑次像素滚动<br>(2F Pixel Scroll)** | `NSScrollWheel` 携带 `Phase::Began -> Changed -> Ended`；支持 Q16.16 高精度定点微位移与自然滚动反向 | 注入 `kCGSessionEventTap`；写入 `FixedPtDeltaAxis1/2` (Q16.16) 与 `PointDeltaAxis1/2`；2 帧落指静止观察窗消除噪声 | • 原生：1 line $\approx 10\text{px}$，65536 固定定点<br>• 本项目：`FixedPt = px * 65536.0`，完全符合 AppKit 标准 | **自动化覆盖；待真机** |
| **07** | **双指滚动物理惯性<br>(2F Scroll Momentum)** | 抬手后由 RunLoop 驱动 `momentumPhase` 自然衰减，手指触碰瞬间物理制动拦截 | 60Hz 衰减飞轮；采集抬手前 $50\text{ms}$ 内峰值速度作为种子；新触碰或断链立即派发 `MomentumPhase::Ended / Cancelled` 物理制动 | • 原生衰减：尾部 $50\text{ms}$ 冲量捕捉<br>• 本项目：`INERTIA_PEAK_WINDOW=50ms`, `SEED=25mm/s` | **自动化覆盖；待真机衰减校准** |
| **08** | **双指智能缩放对焦<br>(2F Smart Zoom)** | 双指快速双击，向系统发送 `GESTURE_SUBTYPE_SMART_MAGNIFY (0x17)` 局部对焦；双击时绝不提前弹出右键菜单 | 识别 $350\text{ms}$ 双指快速双击，绑定瞬时光标坐标 `CGEventSetLocation` 投递 `GESTURE_SUBTYPE_SMART_MAGNIFY`；第 2 次轻点优先消费并清除连击态 | • 原生：$350\text{ms}$ 双击间隔，光标对焦<br>• 本项目：$350\text{ms}$，坐标精准命中 | **私有 payload；待真机应用验证** |
| **09** | **双指缩放与 360° 旋转<br>(Pinch & 360° Rotate)** | 相对增量流；AppKit `NSEvent.magnification` 和 `NSEvent.rotation` 都是应累加到当前状态的变化量；滚动/轻扫开始后由 AppKit 锁定，magnify/rotate 可在多点序列中切换；旋转逆时针为正 | 采用上一帧瞬时间距/角度差分；锁定后每个已准入 stream 都保持完整 `Began -> Changed* -> Ended/Cancelled` 生命周期；增量使用独立 transform 时间基准并受速率和 `±0.08` 缩放硬上限保护；旋转默认输出 **1:1** 的 `-angle_d.to_degrees()`，不再叠加未经验证的经验加速；Pan→pinch/rotate 转场仍作为兼容策略保留 | • 原生：相对增量、方向约定和 scroll lock 有公开语义<br>• 本项目：portable 分类测试覆盖；事件 tap、首帧行为、child payload、转场策略和真实应用结果仍待 macOS 验证 | **代码合同已收紧；私有 payload/应用兼容待真机** |
| **10** | **三指轻点查词<br>(3F Look Up Dictionary)** | 三指并拢轻点，调用系统词典与数据探测器（`Cmd+Ctrl+D` 或系统 Lookup 接口） | 贯通 **$3\text{F}\to 2\text{F}\to 1\text{F}\to 0$ 全链路异步抬指管道**，按触碰总时长与总位移判定；`Cmd+Ctrl+D` 注入 15ms 真实物理脉冲时序并双路投递 Session+HID | • 原生：$300\text{ms} \sim 380\text{ms}$ 时限<br>• 本项目：$380\text{ms}$ / $2.5\text{mm}$ (直抬) / $420\text{ms}$ (分步抬) | **键等价物路径；待真机** |
| **11** | **三指拖移与悬停换把<br>(3F Drag & Regrip)** | 经典三指拖移：手指位移即按住左键拖拽；支持抬手 500ms 悬停换把（Drag Lock / 跨屏延续），中途可加入第 4 指切桌面携窗 | `0.35mm` 触发门限；默认 `release_delay_ms=500`（500ms 悬停延续锁定）；进入 4 指切桌面时**保持 `drag_button_held=true`** | • 原生：支持 Drag Lock 换把悬停<br>• 本项目：默认 500ms 换把悬停，单指轻触即释放 | **自动化覆盖；与 Apple Drag Lock 语义不同，待真机** |
| **12** | **四指桌面平移切换<br>(4F Spaces Swipe)** | 四指横扫通过 DockControl / SkyLight 驱动多桌面平滑过渡动画 | **手指数跳变重锚**（$4 \to 3$ 或 $3 \to 4$ 时几何重置，累加 `cumulative_dx`，避免丢指跳变）；macOS 26 走 Space SymbolicHotKey（10mm 阈值 / 350ms 冷却），macOS 27+ 优先走 `SLEventSetIOHIDEvent` 连续 HID payload，运行时不可用再降级 SymbolicHotKey，旧版保留 DockSwipe | • 原生行程：$50\text{mm}$ 对应 1.0 满行程进度<br>• 本项目：`SWIPE_PROGRESS_REF_MM=50.0mm`；26 为离散切换，27+ 尽量保留连续动画 | **联合拖拽状态机已覆盖；私有/系统路径待 macOS 版本实测** |
| **13** | **四指调度中心 / Exposé<br>(4F Mission Control)** | 四指向上推滑出调度中心，向下推滑出 App Exposé | 纵向 $3.0\text{mm}$ 轴向锁定，派发连续 Vertical DockControl 流 | • 原生锁定：$3.0\text{mm}$ 轴向死区<br>• 本项目：`SWIPE_AXIS_LOCK_MM=3.0mm` | **私有 DockControl；待真机** |
| **14** | **四指捏合启动台<br>(4F Pinch-in Launchpad)** | 四指向心捏合（拇指与三指相向聚拢）展开系统 Launchpad 网格 | 径向收缩比率 $R/R_0 \le 0.72$ 且质心平移 $<4.5\text{mm}$；派发 `CoreDockSendNotification("com.apple.launchpad.toggle")` + SkyLight HotKey 160 | • 原生向心收缩：$\Delta R \ge 28\%$<br>• 本项目：$R/R_0 \le 0.72$，单次触碰锁存防重入 | **离散命令 + 私有路径；待真机** |
| **15** | **四指张开显示桌面<br>(4F Spread-out Show Desktop)** | 四指离心张开（拇指与三指反向推开）推开所有窗口露显纯净桌面 | 径向扩散比率 $R/R_0 \ge 1.28$ 且质心平移 $<4.5\text{mm}$；派发 `CoreDockSendNotification("com.apple.showdesktop.awake")` + SkyLight HotKey 36 | • 原生离心扩散：$\Delta R \ge 28\%$<br>• 本项目：$R/R_0 \ge 1.28$，单次触碰锁存防重入 | **离散命令 + 私有路径；待真机** |
| **16** | **双指右边缘滑入通知中心<br>(2F Right Edge Swipe)** | 双指从触控板右边缘向左滑入展开/收起通知中心 | 边缘区域起始 $x \ge 28\text{mm}$，向左滑动 $\Delta x \le -3.8\text{mm}$ 实时触发通知中心唤出；派发 SkyLight HotKey 163 与 ControlCenter 时钟锚点 | • 原生右缘判定：$x \ge \text{EdgeZone}$，$\Delta x \le -3.5\text{mm}$<br>• 本项目：$x \ge 28.0\text{mm}$，$\Delta x \le -3.8\text{mm}$ 实时触发 | **触控区模拟 + 私有热键；待真机** |
| **17** | **单指软件长按拖拽<br>(1F Press-and-Hold Drag)** | 单指在原地静止停留 $>450\text{ms}$ 自动扣下左键进入拖拽，移动即可选区/拉动，抬指自动释放 | `HOLD_TIME = 450ms` 且位移 $\le 1.0\text{mm}$ 时自动激活 `set_left_button_held(true)`；在 `OneFinger -> Idle` 时释放；默认关闭，需 `[gestures.press_and_hold_drag] enable = "on"` | • 本项目：`HOLD_TIME=450ms`, `TAP_MAX_MOVE=1.0mm`；这不是普通 macOS 默认行为 | **可选辅助功能；自动化覆盖；待真机** |
| **18** | **网络会话隔离与断链保护<br>(Session & Link Safety)** | 网络丢包或客户端断开不能导致指针卡死、按键粘连或虚假点击 | `PeerGate` 实施 600ms 会话隔离与时钟重置；`on_link_timeout` 发送 `Cancelled` 并彻底释放所有按键 | • 本项目：600ms 隔离、迟到帧丢弃、显式 Cancel 状态收尾 | **自动化覆盖；网络安全语义不等于 Apple 输入栈** |

---

## 二、macOS 原生系统偏好设置对照表 (System Preferences Keys)

同步实现位于 `src/macos_preferences.rs`。启动时读取主域
`com.apple.AppleMultitouchTrackpad`，缺失字段回退到
`com.apple.driver.AppleBluetoothMultitouch.trackpad`，自然滚动读取
`.GlobalPreferences/com.apple.swipescrolldirection`。合并优先级为：
显式 TOML 字段 > macOS 设置 > Rust 默认值；读取失败或未知枚举只会记日志并
保留已有配置。该同步是启动时快照，不会写回系统设置。没有内置触控板、
只有外接设备或系统隐藏 Trackpad pane 时，缺失 domain/key 视为正常降级，
不会阻止 companion 启动。

Mac mini 没有受支持的“强行显示” Trackpad pane 接口：深链只能打开已存在
的页面，`defaults`/`cfprefsd`/`activateSettings -u` 只能刷新持久化值。需要
编辑 companion 行为时，使用 `companion-tui`；它只写项目自己的 `config.toml`，
硬件专属能力继续按 `unsupported` 记录。

| 功能项 | 对应 Apple 原生偏好键 (`com.apple.AppleMultitouchTrackpad`) | 对应 `config.toml` 配置项 | 默认值与行为说明 |
|---|---|---|---|
| 轻点以点按 | `Clicking = 1`（本地 HID） | `gestures.tap_to_click` | 本地 HID 入口按实体触控板设置；`companion-net` 为虚拟输入，忽略残留 `Clicking=0`，默认开启 |
| 辅助点按 (右键) | `TrackpadRightClick = 1` | `gestures.secondary_click` | 默认开启，两指轻点触发右键 |
| 自然滚动 | `.GlobalPreferences/com.apple.swipescrolldirection = 1` | `[scroll] natural` | 全局值优先；默认 `true`（内容随手指同向移动） |
| 指针追踪速度 | `.GlobalPreferences/com.apple.trackpad.scaling` | `[cursor] sensitivity` | 兼容性归一化：观察到的 0..3 滑杆映射到 0.5..2.0x；不是 Apple 公布的 px/mm 公式 |
| 指针加速开关 | `.GlobalPreferences/com.apple.trackpad.scaling = -1` | `[cursor] accel_exponent` | 仅 `-1` 映射为线性 `1.0`；其他加速细节保留项目曲线 |
| 滚动速度标量 | `.GlobalPreferences/com.apple.scrollwheel.scaling` | `[scroll] sensitivity` | 以常见 `0.6875` 为 1.0x，结果限制在 5..80 px/mm；不是稳定公开 API |
| 滚动开关 | `TrackpadScroll = 1` | `[scroll] enable` | 关闭时不发 2F 滚动 |
| 横向滚动 | `TrackpadHorizScroll = 1` | `[scroll] horizontal` | 关闭时只保留纵向分量 |
| 滚动惯性 | `TrackpadMomentumScroll = 1` | `[scroll] momentum` | 关闭时抬手不启动 inertia |
| 智能缩放 | `TrackpadTwoFingerDoubleTapGesture = 1` | `gestures.smart_zoom` | 默认开启，向系统派发 `0x17` 局部放大 |
| 捏合缩放 | `TrackpadPinch = 1` | `[gestures.pinch] enable` | 默认 `on`，支持针对特定 App 开启/关闭 |
| 旋转 | `TrackpadRotate = 1` | `[gestures.rotate] enable` | 默认 `on`，支持针对特定 App 开启/关闭 |
| 查词与数据检测器 | `TrackpadThreeFingerTapGesture = 2` | `gestures.dictionary_lookup` | 默认开启，三指轻点调用词典 |
| 三指拖移 | `TrackpadThreeFingerDrag = 1` | `[gestures.three_finger_drag] enable` | 默认 `on`，三指按住左键拖拽 |
| 拖移锁定与换把悬停 (Drag Lock) | `Dragging` / `DragLock` | `[gestures.one_finger_tap_drag]` 与三指 `release_delay_ms` 分开配置 | `DragLock` 仅记录诊断，不覆盖三指 500ms 换把；三指悬停请直接设置 `release_delay_ms` |
| 四指轻扫切换全屏 App / 桌面 | `TrackpadFourFingerHorizSwipeGesture = 2` | `[gestures.swipe.horizontal] backend = "synthetic"` | 默认 `synthetic`；私有 DockSwipe，待 macOS 版本实测 |
| 四指调度中心 (Mission Control) | `TrackpadFourFingerVertSwipeGesture = 2` | `[gestures.swipe.vertical] backend = "synthetic"` | 默认按配置；私有 DockSwipe，待 macOS 版本实测 |
| 四指捏合启动台 (Launchpad) | `TrackpadFourFingerPinchGesture = 2` | 触控区识别 + Dock 命令 | 仅记录设置；离散命令，待真机 |
| 四指张开显示桌面 (Show Desktop) | `TrackpadFourFingerPinchGesture = 2` | 触控区识别 + Dock 命令 | 仅记录设置；离散命令，待真机 |
| 单指长按拖拽 (Press-and-Hold) | 辅助功能选项 | `[gestures.press_and_hold_drag]` | 默认关闭；设为 `"on"` 才启用 |
| 滚动缩放修饰键 | `HIDScrollZoomModifierMask` | `[scroll] modifier_zoom_mask` | 启动时读取并锁定整个 scroll session；未知位记录告警 |
| Shift + 双指纵向滚动 | 无 Apple Trackpad 专用开关 | `[scroll] shift_scroll_horizontal` | 默认保留原始轴向以符合原生；`true` 才启用兼容性的纵向转横向 |
| Control/Command/Option + 点击或拖拽 | Quartz modifier flags + AppKit/App 行为 | 无额外 gesture 开关 | 事件携带真实键盘修饰键；Control-click 由 macOS 解释为 secondary click，其余由前台 App 解释 |
| 触觉反馈 | `ActuateDetents = 1` + 设备感知的 AppKit haptic performer | `macos.haptic_feedback` / `Output::haptic` -> `NSHapticFeedbackManager.defaultPerformer()` | `auto` 跟随系统；点击/拖拽接合/系统手势确认；不伪造 Force Touch 压力，外接无触觉设备静默降级 |

---

## 三、架构优化与关键问题根因技术解析

### 1. 四指切桌面“中途抖动”
* **根因**：手机电容屏高速滑动时偶发丢指（$4 \to 3$ 持续 20ms 后恢复为 4）。旧算法直接使用当前帧质心减去起始质心（`cx - initial_cx`），手指数变化导致物理质心瞬间产生数毫米跳变，引起 `progress` 剧烈抖动。
* **解法**：在 `dispatch_swipe` 中增加手指数变化重锚逻辑。触点数量变化时仅重新对齐 `last_centroid`，进度按连续位移微分累加至 `cumulative_dx / cumulative_dy`，彻底消除丢指跳变。

### 2. 单指双击被吞（误变拖拽）
* **根因**：为了提高拖拽响应速度，旧代码在第 2 次落指瞬间即下发 `LeftMouseDown`。系统接收到的是 `Click(1) -> LeftMouseDown -> LeftMouseUp`，被识别为不连续操作导致双击丢失。
* **解法**：设立 `TAP_DRAG_CONFIRM (200ms)` 判定窗口。第 2 次落指时光标保持钉住且不压键；若在 200ms 内抬手且未滑动，**100% 结算为标准的双击（`click(MouseButton::Left)`）**；若按住超过 200ms 或移动超 $0.35\text{mm}$ 才进入拖拽。

### 3. 单指轻点偶发触发双指右键
* **根因**：大面积指腹触碰电容屏时，传感器偶尔拆分为 2 个极近触点（$<8\text{mm}$），状态机误将其归入双指右键。
* **解法**：设立 `FAT_FINGER_SPLIT_MM = 8.0mm` 间距红线。凡是两指初始间距小于 8mm 的触碰，在抬手时一律判定为肉指拆分，**自动降级为单指左键单击**。

### 4. 双指缩放与旋转串扰
* **历史背景**：旧代码曾在 pinch/rotate 之间按 dominant stream 静默抑制事件，造成 `Began/Changed/Ended` 生命周期不完整。当前已改为对每个已准入 stream 保持完整相对增量序列，但仍不能在缺少 macOS 真机应用证据时宣称跨应用兼容已根治。
* **解法**：锁定后两个已准入 stream 独立输出相对增量；各自使用小死区抑制噪声，并通过独立 transform 时间基准和单帧上限过滤丢帧跳变。旋转默认保持几何 1:1，待真机 A/B 再评估加速。

### 5. 三指拖拽接四指切桌面卡死
* **根因**：三指拖拽跨桌面时状态机流转到 `SwipeLatched`，但退出分支漏掉了按键释放。
* **解法**：在所有进入 `Idle` 以及断链 `cancel_touch` 的总入口处，加入无条件释放 `set_drag_button_held(false)` 的安全熔断。

### 6. macOS 27+ 兼容与跨平台测试隔离
* **解法**：
  * macOS 27+ 采用 `SkyLight.framework` 的 `SLEventSetIOHIDEvent` 动态构建 `HIDEvent`（type 23 + type 9 速度子事件），不可用时降级到 SymbolicHotKey；macOS 26 使用 SymbolicHotKey，25 及更早版本保持经典完整 12 字段 CGEvent 形状；
  * 构建平台无关的 `output_portable.rs` 模拟层，使 113 个手势测试用例在 Linux CI/VM 环境下即可毫秒级运行验证。
