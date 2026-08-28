# Trackpad Companion 产品化重构规划书

状态：执行中，P1 已实现，P2/P3/P4/P5 部分完成（待 macOS 真机与发布环境验收）
版本：2026-08-29
基线提交：`8ed90d6 feat: productize trackpad settings and native macOS client`

## 1. 产品结论

项目现在已经是一个可工作的输入引擎，但还不是“拿到 Mac 上就会用”的产品。核心问题不是再增加一个手势，而是 Mac 端仍以 daemon/终端为中心，连接依赖手填 IP，权限和失败原因分散在日志，安装、后台运行、更新和卸载没有闭环。

本轮产品方向确定为：

> **一个 macOS 菜单栏 companion，自动发现并安全配对手机；SwiftUI 设置窗口负责配置，Rust daemon 负责高频输入；CLI/TUI 保留为开发者和无界面部署入口。**

产品不承诺把用户态 CGEvent 变成 Apple 原始 MultitouchSupport 流。对用户可见的承诺是：连接可靠、状态可理解、权限有向导、输入断链不粘键、设置可恢复、安装可卸载。

## 2. 目标用户与首要场景

### 2.1 目标用户

- Mac mini 没有触控板，需要临时用手机或浏览器控制 Mac。
- 远程桌面、客厅电视、演示场景中需要一个低摩擦的临时触控板。
- 希望使用三指拖移、双指滚动、四指切换 Space，但不愿手动维护 TOML 的高级用户。
- 开发者和 CI 需要可脚本化的 `companion-net`、TUI 和协议测试。

### 2.2 首要成功标准

| 场景 | 目标 |
| --- | --- |
| 首次安装 | 下载后能明确看到下一步；不要求用户理解 Rust、端口或 TCC 术语 |
| 首次连接 | 同一 Wi-Fi 下打开手机即可看到 Mac，扫码或点选后进入触控面 |
| 重复连接 | 最近设备一键连接；网络短暂变化时自动恢复，不重复弹窗 |
| 权限失败 | 看到具体缺失权限、作用和“打开系统设置”按钮，而不是“没有反应” |
| 输入断链 | 250ms 内取消活动手势，释放鼠标键，不制造点击或双击 |
| 配置修改 | GUI/TUI 修改同一份 TOML；重启 daemon 后行为一致 |
| 退出/卸载 | 关闭菜单栏服务后不留后台进程；移除登录项后可删除应用 |

## 3. 保留、重构与暂不做

### 3.1 必须保留

- `gesture::State` 及其纯逻辑测试；Android、Web、HID 三条输入入口继续共享。
- ATP1/ATK1 v1 协议，保持现有 Android、浏览器和 synthetic sender 兼容。
- `companion` HID daemon 和 `companion-net` CLI；GUI 失败时仍可用 CLI。
- `companion-tui`，作为 Mac mini、SSH、无图形环境的完整配置入口。
- macOS 偏好同步的显式 TOML 优先级、虚拟输入对 `Clicking=0` 的隔离、私有 ABI 降级逻辑。
- Android 的深按条、震动强度、主题和已有手势测试工具。

### 3.2 本轮重构

- Mac 端从“终端启动 daemon”变为“菜单栏 app 管理生命周期和配对”。
- 连接从“手填 IP/端口/Token”变为“Bonjour 发现 + 最近设备 + QR/令牌配对 + 手动回退”。
- 权限从启动时报错变为可恢复的 onboarding/checklist。
- 网络状态从日志事实变为统一状态机：`starting`、`waitingForPermission`、`ready`、`paired`、`connected`、`degraded`、`stopped`。
- 配置界面统一使用 SwiftUI GUI；TUI 只作为无图形/高级入口。
- 发布从裸二进制变为 `.app`、`.dmg`、`.zip` 和 GitHub Release 自动产物。

### 3.3 明确暂不做

- 不重写为虚拟 HID digitizer，不在本轮追求 Apple 私有 MultitouchSupport 输入流。
- 不进入 Mac App Store；沙盒、DriverKit entitlement、系统扩展签名另立项目评估。
- 不引入云中继、账号、遥测或第三方远程控制服务。
- 不改变 Command/Control/Option/Shift 的 AppKit 语义，不把应用级快捷键硬编码进手势层。
- 不为了“看起来原生”伪造 Mac mini 的 Trackpad 设置面板。

## 4. 用户路径

### 4.1 首次使用

```text
下载 Trackpad Companion.app
        |
首次启动 -> 菜单栏显示“需要设置”
        |
权限向导：Accessibility（网络输入）/ Input Monitoring（实体 HID，可选）
        |
启动 companion-net -> 生成本地配对令牌 -> 注册 Bonjour 服务
        |
显示 Mac 名称、状态、二维码和短 URL
        |
手机：发现列表 / 扫码 / 手动输入三选一
        |
手机收到连接确认 -> 进入触控板
```

### 4.2 日常使用

- 菜单栏图标只显示连接状态，不抢焦点，不弹出大窗口。
- 点击图标显示：当前手机、延迟、收包速率、服务状态、权限状态、打开设置、复制连接地址、重新生成令牌、停止服务。
- 连接断开先在手机上显示“正在重连”，按指数退避重试；Mac 端不重复生成令牌。
- Mac 睡眠/唤醒后重新发布 Bonjour；旧连接收到的帧按 peer quarantine 规则丢弃。

### 4.3 失败路径

| 失败 | 用户看到的恢复动作 |
| --- | --- |
| Accessibility 未授权 | “输入不会生效” + 打开 Privacy & Security → Accessibility |
| 端口被占用 | 自动尝试端口 0；显示最终端口，不要求用户手动改配置 |
| 无 Bonjour | 显示手动 URL/局域网 IP；保留同网直连 |
| Token 不匹配 | 手机回到配对页；Mac 提供“显示新二维码/复制新链接” |
| 另一台手机已连接 | 显示当前 peer，提供“断开并接管”确认 |
| daemon 崩溃 | 菜单栏显示 degraded，自动重启一次；连续失败后给日志路径和复制诊断按钮 |
| Mac 无触控板 | 不阻塞网络输入；只在 HID 模式提示 Input Monitoring，GUI/TUI 仍完整可用 |

## 5. 目标架构

```text
┌──────────────────────────────┐
│ Trackpad Companion.app        │  SwiftUI + MenuBarExtra
│  Permission/Pairing/Settings  │
│  ServiceSupervisor             │
└──────────────┬───────────────┘
               │ Process / bundled helper
               v
┌──────────────────────────────┐
│ companion-net                 │  Rust network daemon
│ UDP + HTTP + WebSocket        │
│ Bonjour advertisement         │
│ peer/auth/idle-lift state     │
└──────────────┬───────────────┘
               v
┌──────────────────────────────┐
│ Shared Rust gesture/output    │
│ Config + ATP1 + CGEvent      │
└──────────────┬───────────────┘
               │
       Android / Browser / HID
```

### 5.1 选定的边界

- SwiftUI 只负责生命周期、权限、配对、状态和设置展示，不复制手势逻辑。
- Rust 继续负责网络、协议、配置、状态机和 CGEvent；GUI 通过 `companion-config` 和 supervisor 通信。
- GUI 默认启动 `companion-net` 子进程，CLI 继续可独立运行。这样可以先实现产品体验，不把 Rust crate 直接嵌入 Swift 造成 FFI 和 TCC 归属同时变化。
- `companion-config` 扩展为稳定的 JSON 命令接口：`dump`、`set`、`doctor`；TUI 和 GUI 共享它。

### 5.2 最脆弱的假设

本计划假设：签名后的 app 及其 bundle 内 `companion-net` 能在用户授予 Accessibility 后稳定发布 CGEvent。如果 macOS 将 TCC 授权严格归属到子进程而非主 app，第一阶段必须把授权检查和提示放在子进程，并在 GUI 显示具体的 helper 条目；若仍不稳定，再评估 Rust 静态库 + Swift FFI 的进程内方案。不能用“事件调用返回成功”代替真机验证。

## 6. 分阶段执行

每阶段都能独立合并和运行，不依赖后续阶段才能工作。

### P1：Mac 服务监督与权限状态

**结果：** 双击 `.app` 后可以启动/停止 `companion-net`，菜单栏显示真实状态；CLI 行为不变。

- SwiftUI 增加 `ServiceSupervisor`：启动、停止、重启、stdout/stderr 状态显示、退出码和权限错误识别。
- 增加 `PermissionModel`：Accessibility 检查/提示；HID 模式额外检查 Input Monitoring；提供打开对应系统设置的 deep link。
- Rust 增加机器可读 `--status-json` 或 helper status 输出，避免 GUI 解析自然语言日志。
- 使用统一状态模型和错误码，网络未绑定、权限缺失、端口冲突、子进程退出分别展示。
- 验收：冷启动、重复启动、停止时无残留进程、权限拒绝、子进程崩溃、Mac 睡眠唤醒。

**执行状态：已实现，待 macOS 真机验收。** `companion-config doctor` 已提供结构化配置/平台诊断；SwiftUI 设置 app 已提供服务总览、启动/停止/重启、Accessibility 深链和 helper 状态显示。崩溃自动退避、日志归档和更细的错误码仍是 P4/P6 工作。Linux 已通过 Rust workspace 测试，SwiftUI 与 TCC/进程归属仍需 macOS runner 和真机确认。

### P2：Bonjour 发现与安全配对

**结果：** Mac 自动出现在同网设备列表；默认新安装进入安全配对，不再把空 token 当成产品默认。

- SwiftUI app 的 bundled supervisor 发布 `_mtc-trackpad._tcp` Bonjour 服务，TXT 至少包含协议版本、HTTP/WS 版本、显示名称、认证模式和服务实例 ID；裸 `companion-net` 继续提供手动 URL 回退。
- 管理模式首次启动生成 CSPRNG token，写入用户配置；旧 CLI 无 token 的行为保留兼容，但 GUI 新建配置默认启用 token。
- 定义 `mtc://pair?host=&port=&token=` 配对 URI；二维码只包含局域网地址和令牌，不经过云端。
- peer 变更、令牌轮换、旧设备撤销要写入诊断日志并取消活动触点。
- 验收：多台 Mac 同网、端口 0、无 Bonjour、路由器隔离、错误令牌、令牌轮换、旧手机被撤销。

**执行状态：部分完成。** app 已发布 `_mtc-trackpad._tcp`（TXT schema v1），GUI 首次启动通过 `companion-config ensure-token` 生成稳定的 256-bit Token，并生成 `mtc://pair` 链接；Bonjour 在 macOS 真机和网络隔离环境下仍待验收。

### P3：Android/浏览器连接体验

**结果：** 手机不再以手填 IP 为主流程，手动输入保留为救援路径。

- Android 使用 `NsdManager` 浏览 `_mtc-trackpad._tcp`；显示 Mac 名称、地址、端口、认证状态和最近连接时间。
- 增加 QR 扫描入口；扫描结果只接受 `mtc://` 和同源 `http(s)` 地址，拒绝非本地或格式不完整的 URI。
- 保留手动地址/端口/Token 表单，并增加输入校验、IPv6、端口范围、token 显示/隐藏和“测试连接”。
- `UdpSender` 增加连接状态、最近错误、重连退避、peer 接管提示；不改变 ATP1 编码。
- Web 页面优先使用当前 URL 的 token；从配对 URI 打开时保存同源 token，退出/撤销时清除。
- 验收：首次发现、无发现手动回退、Wi-Fi 切换、Mac 重启、后台恢复、错误令牌、两个手机竞争连接。

**执行状态：部分完成。** Android 已接入 `NsdManager`、发现列表、`mtc://pair` intent 解析和输入校验；QR 相机入口、自动重连/退避、Web 配对状态和真机网络矩阵待后续迭代。

### P4：原生设置与诊断整合

**结果：** GUI 成为默认设置入口，TUI 成为高级入口，两者字段和状态一致。

- SwiftUI settings app 吸收现有 `TrackpadCompanionSettings`：三组 Apple 原生设置 + `Companion` 扩展。
- 增加 Overview 页面：服务开关、连接地址/二维码、权限卡片、当前 peer、延迟/收包率、打开日志目录。
- 增加 Diagnostics 页面：配置路径、系统偏好是否可用、unsupported keys、daemon 版本、复制诊断包。
- `companion-config doctor` 输出 JSON：配置解析、端口、权限提示、Bonjour、服务状态和建议动作。
- TUI 增加 `Overview`/`Diagnostics` 只读信息，但不牺牲 SSH 可用性。
- 验收：GUI/TUI 修改同一字段后重启 daemon 行为一致；坏配置有恢复建议；导出诊断不含 token 明文。

**执行状态：部分完成。** SwiftUI 已整合 Overview、服务控制、权限状态、配对链接、脱敏 `doctor` 诊断和日志目录入口；TUI Overview/Diagnostics、当前 peer/延迟统计和诊断包导出待后续迭代。

### P5：`.app`、DMG、签名、公证和 GitHub Release

**结果：** 用户下载一个可拖入 Applications 的 macOS 应用，而不是解压裸二进制。

- 建立 `packaging/macos`：版本读取、SwiftUI 构建、Rust `companion-net`/`companion-config` 构建、`.app` 目录、Info.plist、图标、资源和 helper 布局。
- 使用 app bundle 内的 `Contents/Library/LaunchAgents` + `SMAppService` 管理登录项；用户可在 System Settings → General → Login Items 关闭。
- 首发支持 Apple Silicon；同时建立 x86_64 构建门，只有在 Intel Mac 上完成权限、CGEvent 和私有手势验收后才发布 universal。
- Developer ID + Hardened Runtime；直接分发使用 ZIP/DMG，发布前 `codesign --verify`、`spctl --assess`、`notarytool submit`、`stapler staple`。
- GitHub Actions 增加 macOS 构建矩阵、artifact 校验、版本 tag 触发 Release；无签名 secrets 时只产出未签名开发包，不伪装成正式发行版。
- 验收：全新用户安装/首次启动/权限、升级覆盖、卸载登录项、Gatekeeper、公证 ticket、Apple Silicon 真机。

**执行状态：部分完成。** `packaging/macos/build-app.sh` 可在 macOS 13+ 构建内嵌 helper 的 unsigned `.app`/`.zip`，`package-dmg.sh` 可生成 DMG；CI 已增加 bundle 内容校验和 artifact。登录项、图标、签名、公证和 Finder 安装验收待 release 环境。

### P6：发布前质量与运维

**结果：** 每个版本有可复现的质量门槛和回滚路径。

- Rust：workspace tests、macOS target check、协议兼容 fixture、helper JSON schema 检查。
- Android：NSD/QR/重连单测、ADB 安装和触控回归。
- macOS：权限、登录项、服务 supervisor、Bonjour、Finder/Preview/Safari/Photos/Figma、三指拖拽+四指切 Space 真机矩阵。
- 发布脚本保留上一版本配置，不自动删除用户设置；服务失败可切回 CLI/TUI。
- GitHub Release 附带 changelog、已知限制、SHA-256、支持的 macOS 版本和权限说明。

## 7. 公共接口与数据变更

本计划批准后会新增以下公共表面，所有变更都要带版本兼容策略：

| 表面 | 变更 | 兼容策略 |
| --- | --- | --- |
| Bonjour | app supervisor 发布 `_mtc-trackpad._tcp` + TXT schema v1 | 未发现时继续手动 URL |
| 配对 URI | `mtc://pair` | 同时接受现有 `http(s)` URL |
| 配置 | GUI managed mode 的 token/服务设置 | 旧无 token CLI 配置继续可运行 |
| Helper CLI | `companion-config doctor/ensure-token` | 旧 `dump/set` 保留 |
| macOS 包 | `.app` 内嵌 daemon/helper | 裸 `companion-net`/TUI 不删除 |
| 日志 | 结构化状态/错误码 | 自然语言日志继续给人看 |

不新增第二套手势配置、不改变 ATP1 字段、不把 SwiftUI 直接写 TOML、不在 GUI 内写 macOS `defaults`。

## 8. 风险、回滚与依赖

- **TCC 归属风险：** P1 在真机优先验证；失败时 GUI 仍能启动 CLI，并明确提示授权 helper。
- **Bonjour 不可用：** 发现是增强，不是唯一入口；手动 URL/二维码保留。
- **令牌迁移：** 只对 GUI managed mode 自动生成；现有 CLI 空 token 不强制修改，避免升级后突然断连。
- **登录项失败：** app 前台可手动启动 daemon；`SMAppService` 注册失败显示系统设置入口。
- **私有手势失效：** 继续使用已有版本探测和公共事件降级；P5 不把签名/公证当作手势兼容证明。
- **回滚：** 删除新 `.app` 不改用户 TOML；停用 Login Item 后可恢复上一版本裸 daemon；协议 v1 客户端无需迁移。

第三方/账户依赖：Apple Developer ID 证书和 notarization 凭据只用于正式发布；Bonjour、Network Framework、Android NSD、SwiftUI 均使用平台能力，不引入云账号或运行时服务。

## 9. 研究依据

- Apple Trackpad 设置和硬件显示条件：<https://support.apple.com/guide/mac-help/change-trackpad-settings-mchlp1226/mac>
- Apple `SMAppService`：<https://developer.apple.com/documentation/servicemanagement/smappservice>
- Apple `AXIsProcessTrustedWithOptions`：<https://developer.apple.com/documentation/applicationservices/1459186-axisprocesstrustedwithoptions>
- Apple `NWBrowser`/Bonjour：<https://developer.apple.com/documentation/network/nwbrowser>
- Apple Developer ID 与公证：<https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution>
- Android NSD：<https://developer.android.com/develop/connectivity/wifi/use-nsd>
- Entangle：<https://github.com/gabrieldonadel/entangle>，采用 LAN-only、Bonjour 自动发现和共享 WebSocket 协议。
- iControl：<https://github.com/aianisulislam/iControl>，采用菜单栏 helper、WebSocket、QR/token 配对和 `SMAppService` 登录项。
- LinearMouse 配置：<https://github.com/linearmouse/linearmouse/blob/main/Documentation/Configuration.md>，建议常规修改走 GUI，高级自动化保留结构化配置。

## 10. 批准后的执行顺序

1. P1：Mac app supervisor、权限状态、结构化 status/doctor。
2. P2：Bonjour、token managed mode、配对 URI/二维码数据。
3. P3：Android NSD/QR/重连和 Web 配对入口。
4. P4：Overview/Diagnostics 与现有 SwiftUI 设置整合。
5. P5：`.app`/DMG/签名/公证/GitHub Release。
6. P6：真机矩阵、发布门槛、回滚演练和文档收尾。

计划批准后，每个阶段单独提交；阶段完成状态写回本文件，并在 GitHub Actions 和真机验收记录中留下证据。
