# jaywcjlove macOS 产品与工程理念调研

日期：2026-08-30

范围：PermissionFlow 作者 jaywcjlove 的 macOS 产品目录、公开 README、变更记录、
Swift Package 源码、发布 workflow，以及 Trackpad Companion 当前 macOS 架构。

## 结论先行

作者的共同方法不是追求“所有功能都塞进一个大设置页”，而是把一个高频任务做成
边界清晰的常驻工具：菜单栏是快速控制面，设置窗口是完整配置面，状态和权限始终
可见，数据默认留在本机，复杂流程提供明确的修复路径。

对 Trackpad Companion 最有价值的借鉴有六条：

1. **先定义工作流，再定义控件。** Mousio 把“键盘控制光标、点击、拖拽、网格定位、
   多屏切换”组织成键盘优先的连续操作，而不是一堆孤立开关；Zipora 也把拖入、预览、
   解压、历史、密码和进度做成完整归档流程。
2. **菜单栏承担即时动作。** Mousio 的变更记录持续增加菜单栏中的滚轮、键位布局、
   Focus Screen、退出模式和重启 Hint；KeyClicker 同样把启停、状态和声音选项放进
   菜单栏。设置窗口不应成为每次操作的必经之路。
3. **权限是产品流程，不是错误字符串。** PermissionFlow 提供状态检测、正确的
   System Settings deeplink、跟随窗口的浮动拖拽授权面板；Mousio v3 直接把权限检查
   迁移到 PermissionFlow，说明作者把 TCC 引导当成共享基础设施，而不是每个 app 自己
   拼一套提示框。
4. **反馈要能解释动作。** FocusCursor 用光圈、点击动画和屏幕遮罩解释光标位置；
   Scap 用模式、工具栏、快捷键、可编辑结果和异步进度解释复杂编辑流程。Trackpad
   Companion 也需要让“连接、触点、手势、权限、断链”在视觉上可观察。
5. **本地优先且边界诚实。** Mousio 的隐私声明明确为离线、本地存储、无日志、无云
   服务；PasteQuick 以本地剪贴板历史和应用排除为核心。我们的网络输入工具必须把
   LAN 范围、token、权限和数据留存写进产品界面，而不是藏在日志里。
6. **本地化和发行是基础设施。** app-i18n 将 `.xcstrings` 拆成按语言组织的 `.lproj`
   文件，解决协作冲突和批量翻译问题；多个 app 的 README、FAQ、隐私条款、变更记录、
   Release workflow 和截图都独立维护。语言切换不能继续依赖散落在 View 中的字符串
   三元表达式。

这些原则可以迁移到我们的产品，但不能把闭源 app 的代码、素材、私有资源或文案原样
搬入。当前可直接采用的公开依赖是 PermissionFlow（MIT）；其余多数 app 仓库明确声明
仅作为官网、反馈和需求收集入口。

## 作者产品族的共同模式

### PermissionFlow：把权限引导做成可复用产品

PermissionFlow 支持 Accessibility、Full Disk Access、Input Monitoring、Screen
Recording 等隐私页面；对支持拖入授权的页面，它会打开正确的 System Settings 页面，
显示跟随设置窗口的浮动面板，并把当前 `.app` 做成可拖动来源。`PermissionFlowButton`
还会读取授权状态并显示 Granted/Grant/Checking 等状态；语言通过 SwiftUI `Locale`
注入。见 [README](https://github.com/jaywcjlove/PermissionFlow)、
[PermissionFlowButton.swift](https://github.com/jaywcjlove/PermissionFlow/blob/v2.11.2/Sources/PermissionFlow/PermissionFlowButton.swift)、
[PermissionFlowController.swift](https://github.com/jaywcjlove/PermissionFlow/blob/v2.11.2/Sources/PermissionFlow/PermissionFlowController.swift)。

工程上它把状态 provider、窗口跟踪、面板 UI、deeplink 和资源本地化拆成独立模块，
并在 `PermissionFlowResources` 中为打包资源缺失提供降级路径。这个分层比“按钮点击后
执行一个 URL”更适合我们的 TCC 归属问题。

### Mousio 与 Mousio Hint：键盘优先、常驻、可解释

Mousio 的公开定位是“不依赖鼠标或触控板”，提供键盘驱动光标、网格导航、多显示器、
Dock 风格启动器、Focus Screen 和统计功能；它把无障碍用户和追求效率的用户放在同一
个产品叙事里。见 [Mousio README](https://github.com/jaywcjlove/mousio)。

Mousio Hint 是独立的 companion，负责在 UI 元素旁显示快捷键提示；README 明确指出它
需要读取 UI 元素位置，因此不能在 sandbox 中正常工作。这个“主 app 负责动作，辅助 app
负责可见提示”的边界值得参考，但我们不应为提示再拆一个进程。见
[Mousio Hint README](https://github.com/jaywcjlove/mousio-hint)。

Mousio 的变更记录显示出持续的产品闭环：增加滚轮和布局设置、菜单栏入口、Hint 重启、
多屏切换性能、权限迁移、macOS 26/27 兼容，以及按钮和 sidebar 样式修正。见
[Mousio CHANGELOG](https://github.com/jaywcjlove/mousio/blob/main/CHANGELOG.md)。

### FocusCursor、KeyClicker、Scap：反馈、状态、可恢复编辑

- [FocusCursor](https://github.com/jaywcjlove/focus-cursor) 将光标环、点击动画、绘图和
  周围遮罩用于演示与教学；它的价值不是装饰，而是让远程观众知道“输入落在哪里”。
- [KeyClicker](https://github.com/jaywcjlove/key-clicker) 把启停状态、按键显示、声音
  选项和权限检查放进菜单栏；FAQ 解释系统音效重叠，变更记录还记录了权限 UI、修饰键
  识别和 macOS 27 菜单图标兼容。
- [Scap](https://github.com/jaywcjlove/scap) 将截图、编辑、绘图板和裁剪分成明确模式，
  工具使用快捷键，OCR 结果可编辑，导出异步化。其变更记录反复修复工具栏覆盖、透明
  画布命中、缩放复位、权限提示和多屏焦点问题，体现“每个交互状态都要有回归”的习惯。

对我们来说，触控面不是一块“能发包的黑色区域”：应有连接状态、最后一帧时间、权限
状态、当前手势阶段和断链收尾反馈；诊断不能只吐原始日志。

### PasteQuick、Zipora、Menuist：从单功能到完整工作流

- [PasteQuick](https://github.com/jaywcjlove/paste-quick) 把本地剪贴板历史、应用排除、
  图片预览和列表定制组合成一个低摩擦流程，隐私边界写得很清楚。
- [Zipora](https://github.com/jaywcjlove/zipora) 的 README 不把自己描述成“解压工具”，
  而是拖入、预览、历史、选择目标、密码、进度和 Finder 定位组成的归档工作流；变更
  记录持续修正历史操作、进度锁、密码提示和外部拖拽。
- [Menuist / RightMenu Master](https://github.com/jaywcjlove/rightmenu-master) 同时
  提供 Finder 右键扩展和菜单栏收藏导航，覆盖常用文件夹、历史、脚本、复制、移动、
  QR 分享等高频动作。FAQ 详细记录 Finder 扩展重新授权、重启 Finder 和 macOS 版本
  差异，说明权限失败必须有可执行的修复路径。

这三类产品给我们的直接启发是：连接不应止于“输入 IP”；应支持发现、配对、最近设备、
状态、重试、复制链接、token 修复和安全解释，并在服务故障时给出下一步动作。

### SwiftUI 组件与基础设施：小模块、平台适配、语言可测试

[SFSymbolsPicker](https://github.com/jaywcjlove/SFSymbolsPicker) 是一个很小但完整的
SwiftUI 组件：macOS 用 popover、iOS 用 sheet，搜索采用实时过滤和懒加载网格，按钮和
面板尺寸可替换，语言通过 `\.locale` 传入。它的价值在于组件 API 只暴露选择、面板尺寸
和标签等必要状态，不把平台细节泄漏给调用方。

[MyAppListKit](https://github.com/jaywcjlove/MyAppListKit) 将通用 app 列表、图标、反馈、
评分和更新命令与个人 app 数据拆成不同 product；示例还提供统一的 `Locale.systemPreferred`
和命令菜单。这是“核心库通用，产品数据可选”的清晰所有权边界。

[app-i18n](https://github.com/jaywcjlove/app-i18n) 的设计理由是：单个 `.xcstrings` 容易
产生 Git 冲突、难以拆分翻译任务、批量处理消耗上下文；拆到 `.lproj` 后，语言文件可独立
评审、转换回 Xcode 格式并在多 app 间复用。这个思路适合我们的中英切换，也为未来加入
繁体中文、日文和德语留下空间。

## 与 Trackpad Companion 的差距

当前项目已经具备 Rust 高速输入路径、SwiftUI 设置窗口、菜单栏 supervisor、Bonjour、
token、Web/手机双连接和 PermissionFlow 接入。但对照作者的产品族，仍有这些结构性差距：

| 领域 | 当前状态 | 主要差距 | 证据/位置 |
| --- | --- | --- | --- |
| 应用结构 | `App.swift` 负责 App/Scene 和设置组合；`ServiceSupervisor.swift`、`SettingsModel.swift`、`AppModels.swift`、`Views/` 已分层 | `SettingsModel` 仍以 helper 调用为中心，`.strings` 资源化和更细的连接模型待后续 | `macos/TrackpadCompanionSettings/Sources/TrackpadCompanionSettings/` |
| 快速控制 | 菜单栏已提供 Web/手机开关、启停、重试、权限引导、端口、帧统计和复制链接 | 当前连接仍以单个最近端点为主，完整多设备历史待后续 | `Views/MenuBarView.swift` |
| 连接工作流 | 两个 transport 开关、Bonjour、`mtc://pair`、token、网络切换重绑、最近端点 | Android QR 相机、自动退避和当前 peer/接管流程仍待真机迭代 | `ServiceSupervisor.swift`、`src/net.rs` |
| 权限 | PermissionFlow Accessibility + AX 门禁；宿主 app 关闭 helper 自己的 TCC prompt | 仍需真机确认签名 app 的 TCC 归属、拖拽后自动启动和不同 macOS 版本行为 | `ServiceSupervisor.swift`、`Views/MenuBarView.swift` |
| 本地化 | `AppLanguage.text(english, chinese)` 已集中到模型但仍散落在 SwiftUI View 中 | 文案不可独立审核，无法复用 `.lproj`、复数、格式化和系统 locale；规划中的 Z4f 尚未完成 | `AppModels.swift`、`App.swift`、`Views/` |
| 反馈 | 有服务状态、诊断文本、配对链接、网络状态、最近端点和 UDP/WS/engine 计数 | 延迟、当前 peer、当前手势阶段和断链收尾仍需 Rust 结构化状态输出 | `ServiceSupervisor.swift`、Overview |
| 发行 | `.app`/ZIP/DMG、CI、签名占位 | 缺少完整 CHANGELOG、FAQ、升级/卸载说明和真机发布验收记录 | `packaging/macos/`、README |
| 运行策略 | 菜单栏启动时自动启动 helper；支持 `SMAppService`、睡眠/唤醒刷新、网络切换重绑和一次自动重试 | 仍需在签名 DMG 上验证 TCC、登录项批准和单实例升级路径 | `ServiceSupervisor.swift`、规划书阶段 Z/P5 |

## 迁移到本项目的执行原则

### P0：保持已有 PermissionFlow 方向，但做成权限状态机

- PermissionFlow 只负责打开正确页面、显示拖拽引导和读取宿主进程授权状态。
- `AXIsProcessTrusted()` 继续是 `companion-net` 启动门禁；helper 不弹自己的 TCC 对话框。
- 状态至少区分 `notGranted`、`guidanceOpen`、`checking`、`granted`、`helperStarting`、
  `failed`；回到应用时刷新，授权失败时显示“重新打开引导/查看日志”，不要重复弹窗。
- 真机验收覆盖 Mac mini 无触控板、MacBook/Magic Trackpad、macOS 13/14/15/26，确认
  TCC 条目归属的是签名后的宿主 app，而不是临时路径中的子进程。

### P1：拆出可测试的 macOS 产品壳

建议在不改变 Rust 配置边界的前提下，把当前 `App.swift` 拆成：

```text
macos/TrackpadCompanionSettings/Sources/TrackpadCompanionSettings/
  AppRoot.swift                 # App、WindowGroup、MenuBarExtra、命令
  ServiceSupervisor.swift       # helper 生命周期、权限门禁、输出状态
  ConnectionModel.swift         # Web/手机开关、Bonjour、pairing URI、token 状态
  PermissionModel.swift         # PermissionFlow + AX 状态机
  SettingsModel.swift           # companion-config dump/set 的唯一入口
  Localization.swift            # Locale、String catalog/.lproj 适配
  Views/
    OverviewView.swift
    ConnectionsView.swift
    PermissionView.swift
    MenuBarView.swift
```

拆分不是为了增加文件数量，而是让每个状态都有单一所有者，并能为连接重试、权限
恢复、睡眠唤醒和语言切换写纯逻辑测试。

### P1：把菜单栏做成“快速控制中心”

菜单栏首屏只放当前状态和高频动作：

- Web 访问开关、手机连接开关；
- 服务状态、端口、最后数据包/连接错误；
- “打开设置”“重新启动服务”“复制 Web 地址”“复制配对链接”；
- Accessibility/Token 不满足时显示对应修复入口；
- 退出服务前保留确认或明确停止反馈，避免误触导致手机端无响应。

完整参数仍在设置窗口，菜单栏不变成第二套配置系统。

### P1：建立真正的本地化资源流程

短期可保留中英两种语言，但应把文案从 `text(english, chinese)` 迁移为稳定 key，
将英文和简体中文拆成 `.lproj` 或 String Catalog，并在根视图注入 `Locale`。格式化
数字、端口、延迟和错误信息使用带参数的本地化资源；语言切换后，菜单栏、窗口、权限
面板和复制提示必须同步变化。

### P2：连接产品化

- 保存最近成功的 Mac endpoint/Bonjour service ID，不保存明文 token；
- 发现列表显示能力（Web/手机）、是否受 token 保护、上次连接时间和失败原因；
- Web、UDP、Bonjour、token 失败分别给出可执行动作；
- 网络切换、睡眠唤醒、helper 重启后自动恢复，所有重试都有退避上限；
- 继续维持“Web 和手机两个独立入口”，不新增第三套 transport 配置。

### P2：把触控体验做成可观察的工作流

参考 FocusCursor 和 Scap 的反馈思路，在 Overview/Diagnostics 提供：

- 当前输入来源（Android/Web/HID）和最后一帧时间；
- 当前手势阶段（tap/drag/pinch/rotate/Space handoff）；
- dropped frame、序号跳变、延迟和断链安全收尾；
- 权限、连接和 haptic 的真实能力状态；
- 可复制的脱敏诊断报告，而不是要求用户手动拼日志。

### P3：发行和维护闭环

补齐作者产品普遍具备的低成本维护面：

- `CHANGELOG.md`、FAQ、隐私声明、权限说明、升级和卸载说明；
- 每个 Release 附带 DMG/ZIP、SHA-256、支持的 macOS、权限清单和已知限制；
- CI 在明确的 Xcode/Swift 版本上构建并验证资源 bundle、签名结构和 DMG 内容；
- Issue 模板要求系统版本、架构、连接方式、权限状态和脱敏日志；
- 保留 CLI/TUI 作为高级入口，但不要让普通用户先接触 Rust、端口和 TOML。

## 不应照搬的部分

1. Mousio、Scap、PasteQuick、Zipora、Menuist 等多数仓库的 README 明确声明是官网、
   反馈和需求入口，而不是开源实现；不能把其闭源代码、截图、图标或文案复制到本项目。
2. 作者产品面向不同任务，不能因为它们都有菜单栏就把所有设置塞进菜单栏；我们的高频
   任务是连接和输入，手势参数仍应留在分组设置页。
3. Mousio Hint 需要读取 UI 元素，因此不适合直接作为我们的第二进程；我们已有 SwiftUI
   设置窗口和 Rust helper，应优先在现有边界内增加状态和提示。
4. `MyAppListKit` 的个人 app 数据 product 不属于我们的运行时需求，不引入无关依赖。
5. 作者的 macOS 26/27 修复记录是兼容性信号，不是 Apple 私有 ABI 的证明；我们的
   DockSwipe、gesture event 和 TCC 结论仍必须由目标 Mac 真机矩阵确认。

## 资料与证据边界

### 已使用的一手资料

- [PermissionFlow README](https://github.com/jaywcjlove/PermissionFlow) 与 v2.11.2 的
  [Package.swift](https://github.com/jaywcjlove/PermissionFlow/blob/v2.11.2/Package.swift)、
  [PermissionFlowButton.swift](https://github.com/jaywcjlove/PermissionFlow/blob/v2.11.2/Sources/PermissionFlow/PermissionFlowButton.swift)、
  [PermissionFlowResources.swift](https://github.com/jaywcjlove/PermissionFlow/blob/v2.11.2/Sources/PermissionFlow/PermissionFlowResources.swift)。
- [Mousio README](https://github.com/jaywcjlove/mousio)、
  [Mousio CHANGELOG](https://github.com/jaywcjlove/mousio/blob/main/CHANGELOG.md)、
  [Mousio Privacy Policy](https://github.com/jaywcjlove/mousio/blob/main/docs/privacy-policy.md)。
- [Mousio Hint README](https://github.com/jaywcjlove/mousio-hint)。
- [FocusCursor README](https://github.com/jaywcjlove/focus-cursor)。
- [KeyClicker README](https://github.com/jaywcjlove/key-clicker) 与
  [CHANGELOG](https://github.com/jaywcjlove/key-clicker/blob/main/CHANGELOG.md)。
- [Scap README](https://github.com/jaywcjlove/scap) 与
  [CHANGELOG](https://github.com/jaywcjlove/scap/blob/main/CHANGELOG.md)。
- [PasteQuick README](https://github.com/jaywcjlove/paste-quick)。
- [Zipora README](https://github.com/jaywcjlove/zipora) 与
  [CHANGELOG](https://github.com/jaywcjlove/zipora/blob/main/CHANGELOG.md)。
- [Menuist README](https://github.com/jaywcjlove/rightmenu-master)、
  [Menuist i18n](https://github.com/jaywcjlove/rightmenu-master/tree/main/i18n)。
- [SFSymbolsPicker README](https://github.com/jaywcjlove/SFSymbolsPicker) 与
  [SFSymbolsPicker.swift](https://github.com/jaywcjlove/SFSymbolsPicker/blob/main/Sources/SFSymbolsPicker/SFSymbolsPicker.swift)。
- [MyAppListKit README](https://github.com/jaywcjlove/MyAppListKit)。
- [app-i18n README](https://github.com/jaywcjlove/app-i18n)。
- [awesome-swift-macos-apps](https://github.com/jaywcjlove/awesome-swift-macos-apps)。

### 未能核对或明确排除的资料

- 作者的多数 macOS app 没有公开业务源码，因此其内部窗口层级、数据模型和性能实现
  不可从 README 推断；本报告只提取公开产品行为、维护记录和可见工程边界。
- Exa、Brave 和内部 SearXNG 在本次检索中不可用或无 key，搜索结果改用 DuckDuckGo；
  已知 URL 通过 GitHub 原始文件读取，未把搜索摘要当作实现证据。
- 未对闭源 app 的二进制、私有 API 或用户数据进行逆向；这些不属于本次可授权资料范围。
