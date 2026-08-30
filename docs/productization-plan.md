# Trackpad Companion 产品化重构规划书

状态：执行中，P1 已实现，P2/P3/P4/P5 部分完成；连接服务拆分与 PermissionFlow 权限引导已实现（待 macOS 真机与发布环境验收）
版本：2026-08-30
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
启动 companion-net -> 按 Connections 开关绑定 Web/手机服务
        |
手机服务开启时生成本地配对令牌并注册 Bonjour；Web 开启时显示浏览器 URL
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
│ optional UDP / HTTP + WS      │
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

### 5.3 连接模型决策

连接设置只回答一个产品问题：**哪些设备可以通过哪条入口控制这台 Mac**。两个开关互不隐含，不用“服务已启动”代替“服务已开放”。

| 配置 | TCP Web/WS | UDP 手机 | Bonjour / 配对 | 用户看到的结果 |
| --- | --- | --- | --- | --- |
| Web 开、手机开 | 监听 | 监听 | 发布 | 浏览器和手机都可用 |
| Web 开、手机关 | 监听 | 不监听 | 不发布 | 仅浏览器可用，复制 Web 地址 |
| Web 关、手机开 | 不监听 | 监听 | 发布，标记 `web=0` | 仅手机可用，Android 走 UDP 探针 |
| Web 关、手机关 | 不监听 | 不监听 | 不发布 | helper 空闲退出，不占端口 |

端口仍只有一个配置值，便于配对和防火墙规则；TCP 与 UDP 可以安全地共享同一端口号。`port = 0` 时由实际启用的 transport 取得动态端口。token 是服务级保护，不在 UI 中复制出第二份配置；Web 复制 URL 时临时附加 token，手机复制配对链接时携带 token。

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

**执行状态：部分完成。** Android 已接入 `NsdManager`、发现列表、`mtc://pair` intent 解析和输入校验；QR 相机入口、自动重连/退避和真机网络矩阵待后续迭代。Web 触控页的控制中心、灵敏度档位、触点显示开关、深按条编辑、本地状态恢复，以及 BeUI/Transitions/shadcn 风格的 Bottom Sheet、Command Palette、Dock 状态反馈和主题 View Transition 已完成。

#### P3 Web 执行回写（2026-08-29）

- [x] 控制中心设置面板：语义表单、Escape/背景关闭、焦点恢复、减少透明度/动效降级。
- [x] 灵敏度状态：精准/自然/演示/自定义档位与 55%–160% 范围，写入 `localStorage` 并即时作用于坐标换算。
- [x] 深按条：位置/宽度/确认时长可调；按住进度动画达到阈值后发送 ATP1 `flags.bit0`，松开、取消、切后台均发送释放帧。
- [x] 触点反馈：速度拖尾、方向高光、双层光学环和 280ms 松手衰减可开关；不影响输入帧。
- [x] Web 质量证据：Node JavaScript 语法检查、`git diff --check`、Chromium 390×844 与 1440×900 截图检查通过；目标 Safari/Firefox 仍需最终矩阵。
- [x] Web 动态玻璃性能收敛（2026-08-29）：保留 SVG 位移、CSS 模糊、色差边缘和触点高光；参考 `naughtyduk/liquidGL` 的快照/动态 dirty 思路、`PallavAg/liquid-glass-web-react` 的“形状变化才重建位移图”、`ybouane/liquidglass` 的按交叠区域重绘，以及 Josh Comeau 的 backdrop-filter 遮罩边界优化。当前实现只生成一次 128px 位移图、复用 data URL，触点输入仍 60Hz 发送但视觉 Canvas 最高 30Hz；Canvas DPR 限制 1.5 且总像素不超过 3 MP，触控面启用 `contain: paint`，关闭设置面板跳过 `content-visibility`，不新增质量模式。
- [x] Android 实机材质回归：API 36/HyperOS 的浅色 CLEAR 玻璃保持背景可辨识度，避免大面积透镜变成灰板；ADB 重装、启动和全屏截图通过。
- [x] 组件模式整合：参考 BeUI 的 Dock/Bottom Sheet/Theme Toggle/Command Palette、Transitions.dev 的状态过渡、shadcn/ui 的可拥有语义组件原则，全部改写为当前无构建 Web 客户端的原生实现；来源和许可证边界记录于 [`docs/ui-component-sources.md`](ui-component-sources.md)。
- [x] Liquid Glass 升级：Web SVG 位移图改为凸面四次方 squircle 边缘场，增加折射范围、specular sheen 和触点光源；Chromium 截图验证通过，Safari/Firefox 继续走可读降级。
- [x] 按下材质反馈：触控开始/结束驱动触控面玻璃边缘高光与局部亮度，触点 canvas 仍保持非交互层，不拦截 ATP1 pointer stream。
- [x] 主题系统收敛：保留 7 种 Liquid Glass（晨曦、夜幕、海洋、日落、极光、石墨、自定义）、6 种编辑器主题（Tokyo Night、Nord、Dracula、Solarized Dark、Catppuccin Mocha、Monokai）与经典/高对比表面；实验性材质仍保留 Android 实现但不进入正式选择器，旧值回退到默认玻璃。
- [x] Android 材质实验室：新增凝露水滴、触控水波、雨痕玻璃、棱镜晶体、软胶表面、液态金属、纸张纹理、全息彩膜、复古 LCD、陶瓷白；材质层与 `TouchPadView` 输入链路解耦，水波和材质高光只由触摸触发并在 760ms 内收束。
- [x] Android 动效与布局收紧：材质响应与“触点显示”持久化开关分离；默认不运行常驻材质循环；顶部状态栏和底部工具栏压缩；全屏移除上下 chrome、边距和圆角，保留一个可退出的浮动关闭按钮；Activity 改为 `fullSensor` 并启用短边刘海布局。
- [x] Android 真机回归（2026-08-29）：`192.168.3.137:44899` 安装 `app-debug.apk` 成功，`testDebugUnitTest` 与 `assembleDebug` 通过；竖屏布局、主题列表滚动和全屏零边距已截图检查。AAPT2 在 ARM 主机需使用本机静态 override 路径，代码和资源本身无错误。
- [x] 本轮收尾（2026-08-29）：修复 Android 全屏底部轨道残留白条，关闭导航栏 contrast scrim；Web 移除 `glass-sheen` 无限背景动画并让全屏触控面铺满窗口；Android QWEA0 使用透明度更合适的 `CLEAR` 透镜，避免大面积灰色覆盖；两端主题入口收敛并完成 Web/ADB 回归（当前容器的 AAPT2 运行时限制另行记录）。
- [x] 视觉与控制中心收敛（2026-08-29）：移除触控面 solid fallback 的椭圆阴影；Web 与 Android 顶栏仅保留状态、连接、控制中心和全屏，灵敏度、震动、深按、测试、主题和壁纸统一收进控制中心；新增独立 `custom-glass` 材质档案。
- [x] 壁纸系统（2026-08-29）：Web 支持四张离线预设壁纸、浏览器本地图片压缩存储和恢复主题背景；Android 支持四张 APK 内置壁纸、系统相册选择和可持久化 URI。壁纸仅作为背景层，不参与触摸协议。
- [x] Android/Web 视觉对齐（2026-08-29）：移除 Android 中央材质的圆形、圆角块和大曲线背景装饰，改为连续线性色场与低强度镜面边缘；控制栏内边距收紧，横屏功能按钮完整可达，APK 已重新安装并截图确认。
- [x] 顶栏产品化重排（2026-08-29）：将 Web 的底部操作 dock 与 APK 的 controlsRail 收敛为统一顶栏入口；顶栏只保留连接状态、控制中心和全屏，连接成功后默认收缩为右上角状态胶囊，低频功能移入分组控制中心/对话框，移动端不再依赖横向滚动寻找按钮。
- [x] APK 安装回归（2026-08-29）：使用本机 ARM AAPT2 完成 `assembleDebug`，产物 SHA-256 为 `2c53e77a6042aed11e5da8e6711a610d6d4c99a28a26296153a4441cd0f726cc`；已通过 `adb -s 192.168.3.137:44899 install -r` 安装并启动，前台截图确认连接态顶栏显示状态、控制中心和全屏入口，控制中心内容可纵向滚动。
- [x] 主题层次收尾（2026-08-29）：移除非玻璃主题右下角椭圆落地阴影；经典/编辑器主题改用各自的网格、纸面或低强度纹理背景，触控面只保留主题色表面与边框，Liquid Glass 保留透镜边缘高光。
- [x] 全屏与弹窗回归（2026-08-29）：Android 紧凑顶栏的全屏按钮扩大为独立 44dp 命中区，修复被父容器裁成窄条的问题；进入/退出全屏加入 170–300ms 的淡入、位移和缩放过渡，弹出控制中心、主题、连接、诊断或深按设置时隐藏底层顶栏入口，关闭后恢复。Web 同步加入全屏阶段缩放动画，并在控制中心打开时隐藏顶栏操作。上一版 APK SHA-256 为 `cd6a3a5c87951b6f7218c8cb30369d09f250c558b1a461761d10742a073971bc`。
- [x] 按钮触感收敛（2026-08-29）：Web/Android 标准操作按钮统一为 8px/8dp 几何圆角，紧凑顶栏的图标按钮不再使用胶囊；按下反馈统一为 95ms、0.8% 缩放和 0.5px 下沉，Android 额外使用非对称按下/释放插值器与 96% 透明度，连续点按会先取消上一段动画。上一版 Debug APK SHA-256 为 `f7b538b205186e8cf659cae08c11c69a2c8383d4cc15103a1b1f5270c743d5a3`。
- [x] 深按条几何与语义收敛（2026-08-29）：Web 使用 10px 圆角并在支持的平台启用 squircle；Android 固定 10dp 上限圆角，进度填充裁到同一圆角路径，补齐轻微按下/释放反馈，并为自绘控件增加可聚焦的无障碍语义。此前 Debug APK SHA-256 为 `db55604bb4a1fc37cf068c88633da53e35a57de0bbf9c62eb16a189ef392b76d`。
- [x] Android 资源占用收敛（2026-08-29）：采用单一平衡渲染方案，不增加质量档位；QWEA0 触控面保留完整动态背景、色差、色散和传感器高光，动态重绘只在交互期间开启，背景采样使用 0.5 全局降采样、三级模糊降采样、优化捕获和 0.35 色差/色散降采样；所有壁纸解码最长边限制为 1600px，并在 Activity 销毁时释放位图。ADB 主页稳定基线 PSS `187.1 MB`、Graphics `108.7 MB`、GL mtrack `93.7 MB`，交互期间 CPU 约 `11.1%`、GPU p50 `8 ms`。
- [x] Android 单场景 GPU 合成（2026-08-29）：API 31+ 默认使用自有 `GpuGlassView`，单张半分辨率背景 Bitmap + 单个 RuntimeShader 一次完成折射、RGB 色散、边缘高光和触点光源；API 26–30 保留 QWEA0 回退。触点质心、全屏圆角和自定义玻璃参数保持同步，不增加质量档位。最终 Debug APK SHA-256 为 `b7486d1a12c4774d00a8b20eeccc064cbfd2bf22cb5794136c82ef9fc40d4da1`；ADB 干净主页 PSS `92.5–93.4 MB`、Graphics `29.6 MB`，交互 GPU p50 `8 ms`，空闲不持续重绘。

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
| Bonjour | app supervisor 发布 `_mtc-trackpad._tcp` + TXT schema v1（含 `web`/`phone` 能力） | 未发现时继续手动 URL |
| 配对 URI | `mtc://pair` | 同时接受现有 `http(s)` URL |
| 健康探针 | `GET /health` | 复用现有 TCP 端口；旧无 token daemon 的 `404` 保持兼容 |
| 配置 | GUI managed mode 的 token/`net.web_enabled`/`net.phone_enabled` 服务设置 | 旧无 token CLI 配置继续可运行；新字段默认 true |
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

### 阶段 U：壁纸色彩与材质透明度收敛（2026-08-30）

- [x] U1. 将 Web/Android 背景图从主题渐变与材质层中拆出，避免自定义壁纸被重复染色；无壁纸时才使用主题场景渐变。
- [x] U2. 两端增加背景可见度、背景饱和度、背景亮度和触控面透明度，并持久化到各自本地设置；非液态主题也支持透底但保持稳定的主题 surface。
- [x] U3. Web 用独立背景伪元素承载裁切和滤镜，固体主题选择壁纸时关闭高强度纹理；Android 使用 `ColorMatrix` 和低强度可读性 scrim，API 31+ GPU 玻璃继续采样同一场景。
- [x] U4. 完成浏览器实际截图回归、脚本/Kotlin 编译、ARM AAPT2 重打包和 ADB 真机验收；新版 APK 已安装到 `192.168.3.137:44899`，SHA-256 为 `698b56090262b3088c0b3e006bd78496b29d1751020e4706b238a7e709861a7d`，启动截图确认壁纸与 GPU 玻璃生效。

阶段 U：**代码、自动化检查、APK 构建、ADB 安装和真机视觉验收均已完成。**

### 阶段 V：全屏材质连续性与动效收敛（2026-08-30）

- [x] V1. Android 全屏只隐藏顶部 chrome，锁定切换前的居中触控面几何与液态
  玻璃圆角，不再把表面透明度临时压低，避免进入全屏时出现亮度闪烁。
- [x] V2. 进入/退出动画统一为可中断的短时 ease-out，表面使用轻微缩放收敛，
  浮动退出按钮用同步的缩放与淡入动画保持视觉锚点。
- [x] V3. Web 与 Android 的全屏行为对齐，桌面和窄屏保留边距、圆角、边缘高光
  与用户壁纸；Web 浮动设置按钮由 `data-visible` 驱动过渡，不再依赖 `display` 硬切。
- [x] V4. 完成单测、构建、Web 语法检查和 ADB 真机截图回归；最新 APK SHA-256
  为 `93c35a4b9f2a84d0301ff40ce6be3e81720dd13511241481c4149696559d22c1`。

阶段 V：**已完成开发和真机视觉验收；macOS 主机端的长时稳定性与手势消费继续由既有真机矩阵覆盖。**

### 阶段 W：全屏退出与 Web 兼容回退（2026-08-30）

- [x] W1. Android 全屏响应系统返回键，优先退出全屏而不是关闭应用或丢失连接。
- [x] W2. Web 在 Fullscreen API 不可用时保留可用的 chrome-only 全屏体验，并提供
  同一浮动退出入口，覆盖 iOS Safari 与嵌入式 WebView。
- [x] W3. 完成脚本检查、Android 构建和 ADB 返回键验收。
- [x] W4. 增加复用现有 TCP 端口的 `GET /health` 探针，Android 只有在服务可达且
  token 校验通过后才显示“已连接”，并兼容旧版无 token daemon。最新 APK
  SHA-256：`a51bfe83edf634e25cce7d7547a308390ee90320ff76fb5334d23d9c7a020b90`。

阶段 W：**已完成开发、构建和真机验收。**

### 阶段 X：连接服务产品化拆分（2026-08-30）

- [x] X1. `[net]` 增加 `web_enabled` 与 `phone_enabled`，默认均为 `true`，旧配置无需迁移；关闭项不创建对应 TCP/UDP socket。
- [x] X2. 统一端口策略：两项同时开启时共享端口；只开一项时从该 transport 获取端口；两项关闭时 helper 直接退出且不暴露端口。
- [x] X3. Web 页面、WebSocket 和 `/health` 使用同一 token 授权；Web 关闭时不再提供 TCP 探针，手机端通过 Bonjour/配对能力标记跳过 TCP 探测。
- [x] X4. SwiftUI 新增独立“连接”页：Web 访问、手机连接、监听状态、复制 Web 地址/配对链接和安全提示；运行中的开关修改自动重启 helper，停止状态不被意外拉起。
- [x] X5. Bonjour TXT 与 `mtc://pair` 增加 `web`/`phone` 能力字段；Android 发现/配对读取字段并支持 UDP-only Mac。
- [x] X6. `companion-config doctor` 输出两项服务状态；配置文档、协议文档和架构说明同步更新。

阶段 X：**代码与 Rust workspace 测试已完成；SwiftUI/macOS 真机、Bonjour 网络隔离和 Android UDP-only 连接待在 Mac + Android 环境验收。**

### 阶段 Y：PermissionFlow Accessibility 权限引导（2026-08-30）

- [x] Y1. SwiftUI 设置页接入 PermissionFlow `PermissionFlowButton`，使用官方
  Accessibility 状态检测和跟随 System Settings 的浮动拖拽授权面板；保留
  `AXIsProcessTrusted()` 作为 helper 启动前的真实门禁。
- [x] Y2. 移除服务启动时自动触发的重复 TCC 弹窗；用户从权限面板返回应用后，
  自动重新检测 AX 状态并启动等待中的 `companion-net`。
- [x] Y3. 将 PermissionFlow SwiftPM 资源包复制进 `.app/Contents/Resources`，
  并把 `en`/`zh-Hans` 语言环境传给按钮和浮动面板；记录 PermissionFlow MIT
  许可证来源。
- [ ] Y4. 在 macOS 13+（含 Mac mini 无实体触控板）完成未授权、拖入授权、
  回到应用自动启动、中英文面板切换及签名 DMG 验收。

阶段 Y：**代码与打包链路已完成；PermissionFlow 浮动授权、TCC 归属和真机 DMG 验收待 Mac 执行。**

### 阶段 Z：作者产品理念第一批落地（2026-08-30）

- [x] Z1. 菜单栏升级为快速控制中心：直接切换 Web/手机入口，显示服务状态和端口，
  提供复制地址、权限引导、失败重试、启动/停止和打开设置；详细手势参数仍只在设置窗口维护。
- [x] Z2. 将 companion-net 已有的 `udp_rx`、`ws`、`decode_err`、`engine_in` 周期统计
  解析为 SwiftUI 可观察指标，在总览页显示解码错误和最近更新时间，不新增协议字段。
- [x] Z3. 把语言/服务状态/指标模型、Bonjour 发布器和菜单栏 View 从 `App.swift` 拆出，
  保持单一配置边界和中英语言同步。
- [x] Z4a. 使用 macOS 原生 `SMAppService` 提供“登录时启动”，并在系统唤醒后重新检查
  权限、刷新登录项状态，必要时恢复等待中的 helper。
- [x] Z4b. 将 `ServiceSupervisor` 移到独立文件，集中管理 helper 生命周期、权限、
  登录项、唤醒和 Bonjour；`App.swift` 只保留 App/Scene 与设置界面组合。
- [x] Z4c. 使用 `NWPathMonitor` 监听 Wi-Fi/以太网切换；网络恢复或接口变化时自动
  重新绑定 helper 并重新发布 Bonjour，网络不可用时显示可恢复的 degraded 状态。
- [x] Z4d. helper 非预期退出自动恢复一次，连续失败进入 failed 并保留最后日志；
  用户主动停止不会被后台恢复逻辑重新拉起。
- [x] Z4e. 保存最近一次本地连接端点（不保存 Token），并在总览页显示；登录项
  `requiresApproval` 状态明确提示用户到系统设置批准。
- [ ] Z4f. 将剩余双语文案迁移到独立 `.strings` 资源并加入本地化快照测试。

阶段 Z：**Z1-Z4e 代码已完成并通过静态检查；PermissionFlow、网络切换、登录项和
helper 恢复仍需在 macOS 真机/签名 DMG 上验收，Z4f 本地化资源化待后续迭代。**

### 阶段 R：深按条可视化编辑与触点显示拆分（2026-08-29）

- [x] R1. Android 深按设置增加可视化预览，支持拖动调整横纵位置、拖拽手柄调整宽高，继续复用现有 SharedPreferences 与深按发送链路。
- [x] R2. Web 控制中心增加同等深按编辑预览，新增横向位置和高度参数，位置/尺寸变化即时反映到真实深按条，并尊重 `prefers-reduced-motion` 的静态降级。
- [x] R3. Android/Web 删除“动效开关”产品入口，改为“触点显示”；旧 `visual_effects` 配置只做一次兼容迁移，不再控制材质层动画。
- [x] R4. `custom-glass` 增加折射、饱和度、亮度、边缘高光运行时参数，Web 即时生效，Android 应用后重载 QWEA0 玻璃宿主；其它主题切换会清除自定义 inline 覆盖。
- [x] R5. 明确移除测试页的测试按钮自由布局实验，避免把诊断卡片变成不稳定的自由画布；测试动作仍保持原有顺序和可重复触发。

阶段 R：**代码完成，Android APK 已通过单元测试/构建并安装到 `192.168.3.137:44899`；深按真机手感和玻璃参数 A/B 仍需目标 Mac 应用矩阵复核。**

### 阶段 S：全屏居中触控面与黑边回归（2026-08-29）

- [x] S1. 修复 Android 全屏将 `padHost`、`padFrame` 和玻璃裁剪清零导致的边到边渐变及系统保留区黑边。
- [x] S2. 全屏仅隐藏顶栏并保留普通模式的 18dp 外边距、8dp 内填充、30dp 玻璃圆角和边缘高光，保持中间一整块触控面的视觉与命中区域。
- [x] S3. 增加 `PadLayoutMetrics` 纯逻辑回归测试，锁定全屏使用紧凑居中边距；真机截图确认退出按钮和深按条仍可用。

阶段 S：**代码、单元测试、Debug 构建、ADB 安装和全屏真机截图均已完成。**

### 阶段 T：GitHub 仓库整理与 macOS 原生发行包（2026-08-29）

- [x] T1. 删除已跟踪的机器专属 macOS 诊断快照，增加诊断目录说明，并将
  SwiftPM/Xcode、DMG、App、签名材料、IDE 元数据和本地环境文件加入忽略规则。
- [x] T2. 增加 `LICENSE`、`CONTRIBUTING.md`、`SECURITY.md` 和
  [`docs/architecture.md`](architecture.md)，明确 Rust 核心、SwiftUI、Android、
  浏览器和打包层的所有权边界。
- [x] T3. 保留 Rust `companion-config` 作为唯一配置边界；SwiftUI 继续通过
  helper 读写 TOML，不写 `defaults`，并把可复用设置行与总览组件拆到
  `macos/.../Views/`。
- [x] T4. 打磨原生 SwiftUI 窗口：稳定默认尺寸、sidebar 产品头部、状态徽标、
  总览指标、深浅色系统颜色、语言持久化、Mac mini 无实体触控板说明和配对 URL 处理。
- [x] T5. 完善 `build-app.sh`/`package-dmg.sh`：版本清洗、可选图标、嵌套 helper
  签名、DMG 临时目录清理、高压缩和 Finder 安装布局（900×620 背景、明确图标坐标、
  隐藏 `.background` staging 目录），并新增 tag 触发的 macOS Release workflow。
- [x] T5a. 重做 DMG 安装画布：背景只承载产品名、单条安装指引和转移箭头；Finder
  图标固定在 `{245,340}` 与 `{655,340}`，窗口固定为 900×620，并在挂载前、Finder
  保存布局后分别写入 `.hidden`、`chflags hidden` 和 Finder invisible 元数据，避免
  `.background` 出现在安装窗口中。
- [x] T6. README 增加 `.dmg` 安装和 GitHub Release 说明；CI 在 macOS runner 上同时
  构建 `.app`、ZIP 和 DMG，并验证 bundle 内容。

阶段 T：**代码与发布链路已完成；Linux 环境无法运行 SwiftUI、codesign、hdiutil，
因此 `.app`/`.dmg` 的实际产物和窗口/权限真机验收交由 macOS CI 与目标 Mac 完成。**
