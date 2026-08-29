import AppKit
import PermissionFlow
import SwiftUI

/// The menu-bar surface is intentionally a quick-control center, not a second
/// settings window. It exposes the two high-frequency connection switches and
/// recovery actions while keeping detailed gesture tuning in SettingsView.
struct MenuBarView: View {
    @ObservedObject var supervisor: ServiceSupervisor
    @Environment(\.openWindow) private var openWindow
    @AppStorage("TrackpadCompanion.language") private var languageRaw = AppLanguage.preferred.rawValue

    private var language: AppLanguage {
        AppLanguage(rawValue: languageRaw) ?? .preferred
    }

    var body: some View {
        Text(language.text("Trackpad Companion", "触控板伴侣"))
            .font(.headline)

        Label(statusText, systemImage: supervisor.state.symbol)
            .foregroundStyle(statusColor)

        if !supervisor.networkAvailable {
            Label(
                language.text("Waiting for network", "等待网络"),
                systemImage: "wifi.exclamationmark"
            )
            .font(.caption)
            .foregroundStyle(.orange)
        } else if !supervisor.networkInterface.isEmpty {
            Text(language.text("Network: \(supervisor.networkInterface)", "网络：\(supervisor.networkInterface)"))
                .font(.caption)
                .foregroundStyle(.secondary)
        }

        if let port = supervisor.boundPort {
            Text(language.text("Listening on port \(port)", "监听端口 \(port)"))
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
        }
        if let recent = supervisor.recentConnection, supervisor.boundPort == nil {
            Text(language.text("Last used \(recent.host):\(recent.port)", "上次使用 \(recent.host):\(recent.port)"))
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
        }
        if supervisor.metrics.hasTraffic {
            Text(language.text(
                "\(supervisor.metrics.engineFrames.formatted(.number)) frames processed",
                "已处理 \(supervisor.metrics.engineFrames.formatted(.number)) 帧"
            ))
            .font(.caption.monospacedDigit())
            .foregroundStyle(.secondary)
        }
        if supervisor.metrics.decodeErrors > 0 {
            Label(
                language.text("\(supervisor.metrics.decodeErrors.formatted(.number)) decode errors", "解码错误 \(supervisor.metrics.decodeErrors.formatted(.number)) 次"),
                systemImage: "exclamationmark.triangle"
            )
            .font(.caption)
            .foregroundStyle(.orange)
        }

        Divider()

        Toggle(isOn: Binding(
            get: { supervisor.webEnabled },
            set: { supervisor.setConnectionEnabled(path: "net.web_enabled", enabled: $0) }
        )) {
            Label(language.text("Web access", "Web 访问"), systemImage: "safari")
        }

        Toggle(isOn: Binding(
            get: { supervisor.phoneEnabled },
            set: { supervisor.setConnectionEnabled(path: "net.phone_enabled", enabled: $0) }
        )) {
            Label(language.text("Phone access", "手机连接"), systemImage: "iphone")
        }

        Toggle(isOn: Binding(
            get: { supervisor.launchAtLogin },
            set: { supervisor.setLaunchAtLogin($0) }
        )) {
            Label(language.text("Launch at login", "登录时启动"), systemImage: "power.circle")
        }
        if supervisor.launchAtLoginRequiresApproval {
            Text(language.text("Approve Trackpad Companion in System Settings → General → Login Items.", "请在系统设置 → 通用 → 登录项中批准 Trackpad Companion。"))
                .font(.caption)
                .foregroundStyle(.orange)
        }

        Divider()

        switch supervisor.state {
        case .waitingForPermission:
            PermissionFlowButton(
                pane: .accessibility,
                suggestedAppURLs: [Bundle.main.bundleURL],
                configuration: PermissionFlowConfiguration(
                    promptForAccessibilityTrust: false,
                    localeIdentifier: language.localeIdentifier
                )
            ) { _ in
                Label(language.text("Open permission guide", "打开权限引导"), systemImage: "hand.raised")
            }
        case .failed, .degraded:
            Button(language.text("Try again", "重试"), systemImage: "arrow.clockwise") {
                supervisor.retry()
            }
        default:
            EmptyView()
        }

        Button(language.text("Start service", "启动服务"), systemImage: "play.fill") { supervisor.start() }
            .disabled(supervisor.state == .running || supervisor.state == .starting)
        Button(language.text("Stop service", "停止服务"), systemImage: "stop.fill") { supervisor.stop() }
            .disabled(supervisor.state == .stopped)
        Button(language.text("Open settings", "打开设置"), systemImage: "gearshape") {
            openSettings()
        }

        if !supervisor.endpoint.isEmpty {
            Button(language.text("Copy Web URL", "复制 Web 地址"), systemImage: "doc.on.doc") {
                supervisor.copyWebURL()
            }
        }
        Button(language.text("Copy pairing link", "复制配对链接"), systemImage: "qrcode") {
            supervisor.copyPairingURI()
        }
        .disabled(supervisor.pairingURI.isEmpty)

        Divider()
        Button(language.text("Quit", "退出"), systemImage: "power") { NSApp.terminate(nil) }
    }

    private func openSettings() {
        openWindow(id: "settings")
        NSApp.activate(ignoringOtherApps: true)
        NSApp.sendAction(#selector(NSWindow.makeKeyAndOrderFront(_:)), to: nil, from: nil)
    }

    private var statusText: String {
        switch supervisor.state {
        case .stopped: return language.text("Stopped", "已停止")
        case .starting: return language.text("Starting", "启动中")
        case .waitingForPermission: return language.text("Permission needed", "需要授权")
        case .running: return language.text("Ready", "就绪")
        case .degraded: return language.text("Degraded", "受限")
        case .failed: return language.text("Needs attention", "需处理")
        }
    }

    private var statusColor: Color {
        switch supervisor.state {
        case .running: return .green
        case .failed: return .red
        case .waitingForPermission: return .orange
        default: return .secondary
        }
    }
}
