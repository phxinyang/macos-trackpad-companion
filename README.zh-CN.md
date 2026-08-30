# Trackpad Companion

[English](README.md) | 简体中文

Trackpad Companion 是一款专为 macOS 打造的高精度触控板桥接工具。它可以将你的手机（Android 原生 App 或任何移动端浏览器）以及兼容的 Windows PTP（精确式触控板）设备，变成体验高度还原的 macOS 妙控触控板。

内置高精度 Rust 手势引擎，支持实时识别指针移动、轻点、平滑滚动、捏合缩放、双指旋转以及完整的三指/四指原生手势，并在 macOS 系统层合成对应的事件。

> **说明**：本项目为运行在用户态的系统桥接工具。它不会注册为 Apple 私有的内部触控板驱动，也不支持 Force Touch 物理压感模拟。公开 Quartz 事件具备极佳的应用兼容性；私有手势事件（如捏合、旋转）的效果取决于具体的 macOS 系统版本和目标应用。

## 支持端与特性

| 客户端 / 接入方式 | 功能与特性 | 所需权限 |
| --- | --- | --- |
| **macOS SwiftUI 客户端** | 菜单栏快捷控制、服务守护、二维码配对与系统诊断 | 辅助功能 (Accessibility) |
| **Android 原生 App** | 120Hz 低延迟触控、触觉震动反馈、深按条与扫码配对 | 仅需局域网与相机（扫码） |
| **网页端触控板 (Web)** | 浏览器即开即用（访问 `http://<mac-ip>:4242/`），无需安装 | 仅需局域网 |
| **USB PTP 守护进程** | 直接读取兼容的 Windows 精确式触控板硬件 | 输入监控 + 辅助功能 |
| **TUI 与 CLI 工具** | 支持无头模式、SSH 远程、Mac mini 运维与脚本自动化 | 配置修改无需权限；后台运行需辅助功能 |

macOS 客户端、TUI 与命令行均共用同一个 `companion-config` 工具和相同的 Rust 手势引擎，确保多端配置始终同步、手势表现完全一致。

## 典型使用场景

### 1. Mac mini / 桌面 Mac（无实体触控板）
安装 macOS 原生应用或运行 `companion-net`，即可配合手机或浏览器直接使用完整手势。macOS 在未检测到实体触控板时会隐藏系统的“触控板”偏好设置面板，直接写入 `defaults` 也无法生成虚拟硬件。Trackpad Companion 自带独立完整的配置管理，无需对系统进行复杂的侵入式修改。

### 2. 连接 USB PTP 触控板硬件
直接运行 `companion` 即可接管 HID 触控板。设备需提供标准 Digitizer Touch Pad 集合与触点描述符；解码器支持运行时动态解析描述符及位打包结构（默认参考配置为每触点 6 字节）。

### 3. 免安装浏览器极速体验
启动 `companion-net` 后，在同一局域网下的手机或平板浏览器中打开提示的 URL 即可开始使用。这是最快体验手势引擎的方式，无需安装任何移动端 App。

## macOS 安装指南

从 [GitHub Releases](https://github.com/phxinyang/macos-trackpad-companion/releases) 下载最新的 DMG 安装包，打开后将 `Trackpad Companion` 拖入 `Applications` 目录即可。

* **首次启动与权限引导**：打开应用后，进入「总览 > 权限」，点击「请求辅助功能权限」。内置的 PermissionFlow 模块会自动打开系统「隐私与安全性 > 辅助功能」设置页，并引导完成授权。
* **系统要求**：支持 macOS 13 及更高版本（二进制已内置所有 Rust 网络与配置 helper）。
* **开源分发说明**：本地构建默认采用 ad-hoc 签名。由 tag 触发的 GitHub Release 必须配置 Developer ID 签名与 Apple 公证凭据；缺少发布密钥时工作流会停止，不会上传开发包。

## 从源码构建

> 💡 **提示**：Rust 二进制文件依赖 macOS 本地系统库，请在目标 Mac 上直接编译，请勿将 Linux 构建的 ELF 产物直接复制到 macOS。

```sh
# 克隆仓库并编译核心引擎
git clone https://github.com/phxinyang/macos-trackpad-companion.git
cd macos-trackpad-companion
cargo build --release
```

运行 HID 硬件守护进程：
```sh
./target/release/companion -v
```

运行网络监听服务：
```sh
./target/release/companion-net -v
```

构建 macOS 原生 SwiftUI 应用与 DMG 安装包：
> 当前 PermissionFlow 依赖需要 Swift 6.2（Xcode 26 或更高版本）。

```sh
./packaging/macos/build-app.sh
./packaging/macos/package-dmg.sh
open dist/macos/Trackpad-Companion-*-macos.dmg
```

应用包会将 `companion-net`、`companion-config` 及本地化资源打包至 `Contents/Resources`，无需用户单独配置 Homebrew 环境。

## 连接手机与客户端

macOS 客户端的「连接」页面提供两个独立的通道开关：
* **网页端访问 (Web)**：通过 TCP 开放网页触控板与 WebSocket 服务；
* **手机端访问 (Phone)**：通过 UDP 提供高频触控通道，并通过 Bonjour（mDNS）广播配对服务。

### 推荐连接流程

1. **选择通道**：在 Mac 菜单栏或设置的「连接」页面中，打开所需的通道开关（默认均开启）。
2. **手机 App 扫码连接**：
   * 打开 Android App，点击「扫描二维码」；
   * 对准 Mac 屏幕上的配对二维码扫码，Mac IP、端口和配对 Token 会自动解析并完成低延迟连接（全流程仅在本地局域网完成，不经过任何云端）。
   * *备用连接*：若无法使用相机，可在 App 连接页选择「IP 连接（备用）」，手动输入 Mac IP、端口与 Token。
3. **浏览器即开即用**：
   * 复制 macOS 界面上显示的 Web 地址（如 `http://192.168.1.100:4242/?token=...`），直接在手机浏览器中打开即可。

### 协议测试工具（免手机）

如果手头暂时没有手机，可以使用内置的 Python 探针在本地直接测试协议与手势效果：

```sh
# 模拟触点位移轨迹
python3 tools/synthetic_sender.py --host 127.0.0.1 --mode circle
# 模拟双指滚动
python3 tools/ws_probe.py --host 127.0.0.1 --mode scroll
# 在 Mac 上逐项触发手势并目视确认效果
python3 tools/gesture_probe.py
```

*若 UDP 服务开启了认证，请在探针命令后附加 `--token <your-token>`。*

## 安全机制与权限规范

网络监听服务能够向 Mac 注入鼠标与键盘事件，因此安全性至关重要：

```sh
# 自动生成随机配对 Token 并检查配置
./target/release/companion-config ensure-token
./target/release/companion-config dump
```

* **无 Token 保护**：若未配置 Token，`companion-net` **严格仅监听本地回环地址 `127.0.0.1`**；若此时尝试显式绑定非回环地址（如 `0.0.0.0`），服务会**直接拒绝启动**以防未授权访问。
* **有 Token 保护**：配置 Token 后，服务默认监听 `0.0.0.0`，允许已携带 Token 的局域网客户端接入。
* **脱敏规则**：配对链接与 Token 等同于操作密码。向外提交 Issue 或日志时，诊断脚本会自动脱敏敏感参数。详见 [SECURITY.md](SECURITY.md)。

## 配置文件手册

默认配置文件路径：
```text
$XDG_CONFIG_HOME/macos-trackpad-companion/config.toml
# 若环境变量未设置，则回退至：~/.config/macos-trackpad-companion/config.toml
```

配置文件缺失时会自动采用默认参数。推荐通过 macOS 界面或 TUI 交互式配置，也可使用 CLI 脚本修改：

```sh
# 查看当前解析后的完整配置
./target/release/companion-config dump
# 调整指针灵敏度
./target/release/companion-config set --path cursor.sensitivity --value 28
# 执行配置与环境诊断
./target/release/companion-config doctor
```

精简配置示例：

```toml
[net]
port = 4242
web_enabled = true
phone_enabled = true
# token = "your-pairing-token" # 监听局域网需配置非空 Token

[cursor]
sensitivity = 28.0
accel_exponent = 1.35
accel_ref = 70.0

[scroll]
sensitivity = 20.0
natural = true       # 自然滚动方向
horizontal = true    # 支持双指横向滚动
momentum = true      # 惯性动量平滑

[macos]
sync_system_settings = true
haptic_feedback = "auto" # auto | on | off

[gestures]
tap_to_click = "on"
secondary_click = "on"
smart_zoom = "on"
dictionary_lookup = "on"
right_edge_swipe = "on"
parameter_profile = "native" # native | chromium_os
surface_width_mm = 65.0

[gestures.pinch]
enable = "on"
gain = 1.0

[gestures.rotate]
enable = "on"
gain = 1.0

[gestures.three_finger_drag]
enable = "on"
persistent_drag_lock = true
release_delay_ms = 500
```

完整配置字段、独立 App 策略与手势后端说明详见 [docs/configuration.md](docs/configuration.md)。

## 原生手势全景指南

- **单指手势**：
  - **指针移动**：支持线性移动与仿 Mac 加速曲线。
  - **点按操作**：轻点点按（Tap to Click）、双击、轻点拖移（Tap-to-Drag）以及按住拖移（Press-and-Hold Drag）。
- **双指手势**：
  - **平滑滚动**：高精度 2D 分阶段滚动（支持自然/反向、动量惯性衰减、Shift 横向滚动兼容映射）。
  - **缩放与旋转**：双指捏合缩放（Pinch to Zoom）与双指旋转（Rotate，兼容 AppKit/Safari 等）。
  - **边缘轻扫**：从右侧边缘向左滑入打开/关闭 macOS 通知中心（Right-Edge Swipe）。
  - **智能缩放**：双指双击放大或恢复网页内容（Smart Zoom）。
- **三指手势**：
  - **三指拖移**：带智能防抖的鼠标左键拖拽（Three-Finger Drag），支持 `persistent_drag_lock`（允许跨桌面 Space 继续拖拽）。
  - **三指轻点**：查询词典与数据检测器（Dictionary Lookup）。
- **四指手势**：
  - **向上轻扫**：打开调度中心（Mission Control）。
  - **向下轻扫**：打开应用程序窗口（App Exposé）。
  - **向左/向右轻扫**：在全屏 Space 与多个桌面之间平滑切换。
  - **径向捏合（四指收缩）**：打开启动台（Launchpad）。
  - **径向张开（四指散开）**：显示桌面（Show Desktop）。
- **物理修饰键完整透传**：
  - 键盘上的 Control、Option、Command 和 Shift 会实时合并到鼠标、滚动、缩放和旋转事件流中，保证快捷操作符合预期。

## 与原生触控板的技术边界

为了提供清晰透明的预期，项目在此列出各项能力的实现方式与边界：

| 手势与功能 | 实现方式 | 兼容性表现 |
| --- | --- | --- |
| **指针移动、点按、拖拽** | 公开 Quartz `CGEvent` 鼠标事件 | 全系统及所有第三方应用完美支持 |
| **平滑双指滚动与惯性** | 公开分阶段滚动事件与数学惯性衰减 | 完美支持 Safari、Chrome、文档等应用 |
| **双指捏合缩放、双指旋转** | 逆向提取的私有 `CGEvent` 字段注入 | 兼容主流 AppKit 与 Safari 原生应用 |
| **桌面切换、调度中心、启动台** | 仿真 DockSwipe 与快捷调度路由 | 针对现代 macOS 版本适配合成 |
| **Force Touch 物理压感** | 公开 `CGEvent` 无法模拟硬件压电传感器 | 不支持硬件级多级压力感应 |

完整逆向调研报告与协议分析详见 [docs/reverse-engineering-sources.md](docs/reverse-engineering-sources.md)。

## 诊断与开发调试

```sh
# 运行环境与配置体检
./target/release/companion-config doctor
# 启动终端交互式 TUI
./target/release/companion-tui
# 只读采集诊断报告（应用状态、权限、进程与端口）
./scripts/diagnose-mac.sh collect
# 探测 4242 端口与网络可用性
./scripts/diagnose-mac.sh probe --port 4242
# 前台实时 trace 抓取日志
./scripts/diagnose-mac.sh trace --port 4242
```

提交代码前请运行完整测试套件：
```sh
cargo test --workspace
cargo check --all-targets
```

## 正式发布

推送版本 tag 后，GitHub Actions 会创建一个同时包含 Android 签名 APK/AAB、
Developer ID 签名并完成公证的 macOS DMG/ZIP、自动发布说明和 `SHA256SUMS`
的 GitHub Release。发布凭据仅保存在 GitHub Actions Secrets 中。所需密钥、
版本字段和 tag 流程见 [docs/releasing.md](docs/releasing.md)。

## 目录结构速览

| 目录 | 职责 |
| --- | --- |
| `src/` | Rust 核心守护进程、网络监听、手势状态机与 macOS 事件输出 |
| `crates/touchpad-proto/` | 跨端共享的 ATP1 触控协议编解码库 |
| `macos/TrackpadCompanionSettings/` | 原生 macOS SwiftUI 控制中心与状态栏 App |
| `android/` | Android 原生 120Hz 触控 App 与测试工程 |
| `static/` | 网页端 GPU 液态玻璃触控板与手势测试页 |
| `packaging/macos/` | macOS 应用打包、签名与 DMG 制作脚本 |
| `docs/` | 架构设计、协议格式、配置手册与技术调研文档 |
| `tools/` | 协议测试探针与手势模拟工具 |

## 开源许可证

本项目采用 [MIT 许可证](LICENSE)。

macOS 客户端通过 SwiftPM 引入了 [PermissionFlow](https://github.com/jaywcjlove/PermissionFlow)（采用 [MIT 许可证](https://github.com/jaywcjlove/PermissionFlow/blob/v2.11.2/LICENSE)）。各端素材与调研引用详见 `docs/` 及 `static/assets/` 下的相关文档。
