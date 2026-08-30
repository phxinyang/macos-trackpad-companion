# Trackpad Companion

[English](README.md) | 简体中文

Trackpad Companion 可以把手机、浏览器或兼容的 PTP（Precision Touchpad，精确式触控板）设备变成 macOS 的触控输入面。Rust 引擎识别指针移动、点按、滚动、捏合、旋转以及三指/四指手势，再输出应用可以接收的 macOS 事件。

这是一个 beta 阶段的用户态桥接工具。它不会注册成 Apple 私有的 MultitouchSupport 设备，也不能合成 Force Touch 的压力等级。公开 Quartz 事件的兼容性较好，私有手势事件会受到 macOS 版本和目标应用影响。

## 功能概览

| 入口 | 用途 | 权限 |
| --- | --- | --- |
| macOS SwiftUI 应用 | 设置、服务控制、配对和诊断 | 辅助功能 |
| Android 客户端 | 完整触控面、震动和深按条 | 局域网 |
| 浏览器客户端 | 直接打开 `http://<mac>:4242/` 使用 | 局域网 |
| USB PTP 守护进程 | 直接读取兼容的 HID 触控设备 | 输入监控 + 辅助功能 |
| TUI 和 CLI | SSH、Mac mini、自动化和故障恢复 | 配置本身不需要权限，守护进程仍需要对应权限 |

macOS 应用和 TUI 都通过同一个 `companion-config` helper 读写配置，手势也共用同一个 Rust 引擎。这样在一个客户端里修改参数，不会产生另一套不兼容的行为。

## 选择使用方式

### Mac mini，没有实体触控板

安装 macOS 应用或运行 `companion-net`，再使用 Android 或浏览器客户端。没有内置或无线触控板时，macOS 本来就不会显示 Apple 的“触控板”设置页。用 `defaults` 写入 `com.apple.AppleMultitouchTrackpad` 只能修改残留偏好，不能注册虚拟硬件，也不能让设置页出现。Trackpad Companion 自己保存和应用配置，不需要系统层面的伪造方案。

### Mac 上连接 USB PTP 设备

运行 `companion`，让它直接打开 HID 数字化器。设备需要提供 Digitizer Touch Pad 集合和描述符定义的触点字段。解码器运行时读取描述符，也支持描述符声明的位打包布局；项目参考配置是每个触点 6 字节。

### 只用浏览器

运行 `companion-net`，在同一个局域网的设备上打开终端输出的 URL，就可以使用触控面。这是不用安装 Android 软件、快速测试手势引擎的方式。

## macOS 安装

从 GitHub Release 下载 DMG，打开后把 `Trackpad Companion` 拖到“应用程序”。第一次启动时，打开“总览”>“权限”并点击辅助功能操作；PermissionFlow 会打开正确的“隐私与安全性”页面，并引导你把应用拖进授权列表。应用支持 macOS 13 或更高版本，并把 Rust 网络 helper 和配置 helper 放在应用包内。

没有配置发布签名时，构建结果会明确标记为开发用 unsigned 包。要在开发机之外分发，需要使用 Developer ID 签名并完成公证。

## 从源码构建

Rust 二进制文件与构建主机相关。请在实际运行它们的 Mac 上编译，不要把 Linux 构建出来的 ELF 文件复制到 macOS。

```sh
git clone https://github.com/phxinyang/macos-trackpad-companion.git
cd macos-trackpad-companion
cargo build --release
```

运行 HID 守护进程：

```sh
./target/release/companion -v
```

运行网络守护进程：

```sh
./target/release/companion-net -v
```

在 macOS 上构建原生设置应用和 DMG：

当前 PermissionFlow 依赖需要 Swift 6（Xcode 16 或更高版本）才能构建 macOS 设置目标。

```sh
./packaging/macos/build-app.sh
./packaging/macos/package-dmg.sh
open dist/macos/Trackpad-Companion-*-macos.dmg
```

应用包会把 `companion-net`、`companion-config` 和 PermissionFlow 本地化资源放到 `Contents/Resources`，用户运行时不需要另外安装 Homebrew。
菜单栏提供 Web/手机入口开关、登录时启动、服务重试、实时帧统计和复制操作，不需要每次打开完整设置窗口。
应用会监视当前 Wi-Fi/以太网接口，网络切换后自动重新绑定并发布 Bonjour；helper 意外退出时自动重试一次，连续失败会保留明确的诊断状态。最近使用的本地端点会保存在 Mac 上，但配对 Token 不会写入 macOS 偏好。
退出应用时会同步停止内嵌的 `companion-net`，等待实例锁释放后再结束进程，立即重新打开不会再撞上旧服务。

## 连接手机或浏览器

`companion-net` 提供浏览器客户端，并通过 UDP 和 WebSocket 接收相同的 ATP1 帧。网络允许组播 DNS 时，Android 应用会通过 Bonjour 发现 `_mtc-trackpad._tcp`。也可以在 macOS 应用里复制 `mtc://pair?...` 链接，或手动填写地址。

推荐流程：在 macOS 应用的“连接”页只打开需要的入口；打开“手机触控板”后，用 Android 的“扫描二维码”读取页面上的二维码，地址、端口和 Token 会自动填入并立即连接。二维码只在局域网内携带配对信息，不经过云端。扫码页使用 APK 内置的二维码模型，不依赖 Google Play 动态扫码模块；首次使用只需允许相机权限。相机不可用时，在 Android 连接页向下滚动使用“IP 连接（备用）”，填写 Mac 的局域网 IP、端口和配对 Token；两台设备需要连接同一 Wi-Fi。只打开 Web 服务时，直接复制 macOS 页面显示的 Web 地址到浏览器。

```sh
./target/release/companion-net --port 4242 -v
```

没有手机时，可以用脚本快速检查协议链路：

```sh
python3 tools/synthetic_sender.py --host 127.0.0.1 --mode circle
python3 tools/ws_probe.py --host 127.0.0.1 --mode scroll
# 在 Mac 上逐项触发并目视确认手势
python3 tools/gesture_probe.py
```

如果 UDP 监听已配置认证，请给 `synthetic_sender.py` 或 `gesture_probe.py`
传入相同的 `--token`。

Android 和浏览器客户端都用毫米作为坐标单位，并采用相同的各向同性缩放。浏览器把触控面映射到配置中的 65 mm 虚拟宽度。Android 优先使用触控面报告的物理 DPI，不可用时回退到设备密度。

## 安全与权限

网络监听器可以向 Mac 注入指针和手势事件。任何不完全可信的局域网都应设置 token，绝不要把监听端口暴露到公网：

```sh
./target/release/companion-config ensure-token
./target/release/companion-config dump
```

macOS 应用第一次进入托管配置时会自动生成 token。浏览器通过 `?token=...` 发送，WebSocket 可以使用 Bearer header，UDP 客户端则把 ATP1 放进文档规定的 ATK1 外层。

当 `token` 缺失或为空且未设置 `listen_ip` 时，`companion-net` 只绑定
`127.0.0.1`。如果显式指定非回环 `listen_ip` 却没有 token，程序会拒绝启动；
配置 token 但省略 `listen_ip` 时，默认仍为 `0.0.0.0`，允许已认证的局域网客户端连接。

不同输入路径需要的权限不同：

- `companion-net` 只需要辅助功能权限来发送合成事件，不读取本地 HID，所以不需要输入监控。
- `companion` 需要输入监控来读取原始 HID 报告，同时需要辅助功能来发送合成事件。
- Android 和浏览器客户端只需要局域网访问权限。

配对链接中包含网络 token。请把它当作密码处理，提交 issue 前删掉 token、主机名、完整路径和未经脱敏的日志。安全报告方式见 [SECURITY.md](SECURITY.md)。

## 配置

默认配置路径：

```text
$XDG_CONFIG_HOME/macos-trackpad-companion/config.toml
```

未设置 `XDG_CONFIG_HOME` 时，使用 `~/.config/macos-trackpad-companion/config.toml`。配置文件不存在也没关系，程序会使用默认值；未知字段会被拒绝。

交互式修改建议使用 GUI 或 TUI。脚本可以使用 JSON helper：

```sh
./target/release/companion-config dump
./target/release/companion-config set \
  --path cursor.sensitivity --value 28
./target/release/companion-config doctor
```

精简配置示例：

```toml
[net]
port = 4242
# listen_ip = "192.168.1.20" # 需要非空 token
# token = "replace-with-a-random-token"

[cursor]
sensitivity = 28.0
accel_exponent = 1.35
accel_ref = 70.0

[scroll]
sensitivity = 20.0
natural = true
horizontal = true
momentum = true

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

完整字段、默认值、按应用策略和 swipe backend 见 [docs/configuration.md](docs/configuration.md)。

## 手势行为

- **单指手势**：
  - 指针移动：线性/加速曲线光标位移。
  - 轻点点按（Tap to Click）、双击、轻点拖移（Tap-to-Drag）以及按住拖移（Press-and-Hold Drag）。
- **双指手势**：
  - 平滑滚动：分阶段 2D 滚动（自然/反向方向、动量惯性、Shift 横向滚动兼容映射）。
  - 双指捏合缩放（Pinch to Zoom）与双指旋转（Rotate，兼容 AppKit/Safari 等）。
  - 右边缘轻扫（Right-Edge Swipe）：从右侧边缘向左滑入打开/关闭 macOS 通知中心。
  - 智能缩放（Smart Zoom）：双指双击放大网页或内容。
- **三指手势**：
  - 三指拖移（Three-Finger Drag）：带防抖过滤的鼠标左键长按拖拽，支持 `persistent_drag_lock`（可跨全屏空间继续拖拽）。
  - 三指轻点（Three-Finger Tap）：词典查询与数据检测器。
- **四指手势**：
  - 向上轻扫：调度中心（Mission Control）。
  - 向下轻扫：应用程序窗口（App Exposé）。
  - 向左/向右轻扫：在全屏 Space 与桌面间平滑切换。
  - 径向捏合（4指收缩）：启动台（Launchpad）。
  - 径向张开（4指散开）：显示桌面（Show Desktop）。
- **修饰键保留**：
  - Control、Option、Command 和 Shift 会完整保留在鼠标、滚动、捏合、旋转和拖移事件中；系统快捷键路径仅使用已注册组合键以防 WindowServer 拒绝。

Apple 没有公开捏合或旋转的灵敏度设置。`gestures.pinch.gain` 和 `gestures.rotate.gain` 是 Companion 自己的兼容参数，不代表 Apple 的物理标定值。`chromium_os` profile 是基于公开识别器的实验选项，也不是 macOS 系统设置。

## 与原生触控板的边界

项目明确区分可兼容的部分和无法伪造的部分：

| 手势 | 实现方式 | 兼容性 |
| --- | --- | --- |
| 指针、点按、拖移 | 公开 Quartz 鼠标事件 | 大多数应用可用 |
| 滚动 | 公开的分阶段滚动和可选惯性 | 应用兼容，但不是 Apple 私有流 |
| 捏合、旋转 | 逆向得到的私有 CGEvent 字段 | 只在部分应用和系统版本中可用 |
| Space、调度中心 | 私有 Dock 或系统快捷键路径 | 版本敏感，属于合成事件 |
| Force Click 压力 | 公开 CGEvent 无法提供 | 不模拟压力等级 |

完整调研记录见 [docs/reverse-engineering-sources.md](docs/reverse-engineering-sources.md)，其中包含抓取字段、开源项目对比和现有证据的边界。

## 诊断与开发

```sh
./target/release/companion-config doctor
./target/release/companion-tui
# 只读采集：应用、helper、权限提示、进程、端口和最近日志
./scripts/diagnose-mac.sh collect
# 检查本机 4242 端口和浏览器入口
./scripts/diagnose-mac.sh probe --port 4242
# 前台 trace 运行，复现后按 Ctrl-C，保留真实退出码
./scripts/diagnose-mac.sh trace --port 4242
# 只打开辅助功能设置页，不会自动授予 TCC 权限
./scripts/diagnose-mac.sh permissions --open
```

诊断脚本不使用 `sudo`，不会修改配置、上传数据或开放新的监听端口。报告和
trace 日志默认写入 `diagnostics/`，权限为 `0600`；提交 issue 前请检查并删除
配对 token、主机名、完整路径和原始用户数据。完整说明见
[diagnostics/README.md](diagnostics/README.md)。

提交改动前运行：

```sh
cargo test --workspace
cargo check --all-targets
```

macOS 打包会在 GitHub Actions 的 macOS runner 上验证。推送 `v0.2.0` 这样的 tag 后，`.github/workflows/release-macos.yml` 会构建 ZIP 和 DMG，并把它们作为 Release 附件发布。签名和公证凭据不会进入仓库。

## 仓库结构

| 目录 | 职责 |
| --- | --- |
| `src/` | Rust 守护进程、网络监听、手势引擎和 macOS 输出 |
| `crates/touchpad-proto/` | ATP1 编解码器 |
| `macos/TrackpadCompanionSettings/` | 原生 SwiftUI 设置应用 |
| `android/` | Android 触控客户端和测试 |
| `static/` | 浏览器触控面和手势测试页 |
| `packaging/macos/` | App 包和 DMG 脚本 |
| `docs/` | 架构、配置、协议、调研和规划书 |
| `tools/` | 可重复的发送器和协议探针 |

运行时所有权见 [docs/architecture.md](docs/architecture.md)，开发检查见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 许可证

本项目使用 MIT，见 [LICENSE](LICENSE)。第三方调研来源和资源来源记录在 `docs/` 及 `static/assets/` 下对应的文档中。macOS 设置应用通过 SwiftPM 引入 [PermissionFlow](https://github.com/jaywcjlove/PermissionFlow)，使用其 [MIT 许可证](https://github.com/jaywcjlove/PermissionFlow/blob/v2.11.2/LICENSE)。
