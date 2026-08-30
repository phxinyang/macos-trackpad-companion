import Foundation
import PermissionFlow
import SwiftUI
#if os(macOS)
import AppKit
#endif

@main
@MainActor
struct TrackpadCompanionSettingsApp: App {
    @StateObject private var supervisor = ServiceSupervisor()
    @AppStorage("TrackpadCompanion.language") private var languageRaw = AppLanguage.preferred.rawValue

    private var language: AppLanguage {
        AppLanguage(rawValue: languageRaw) ?? .preferred
    }

    var body: some Scene {
        WindowGroup(id: "settings") {
            SettingsView()
                .environmentObject(supervisor)
                .environment(\.locale, Locale(identifier: language.localeIdentifier))
                .frame(minWidth: 820, minHeight: 580)
                .onOpenURL { url in supervisor.handlePairingURL(url) }
        }
        .defaultSize(width: 980, height: 680)
        .windowResizability(.contentMinSize)
        .commands {
            CommandGroup(after: .appSettings) {
                Button(language.text("Toggle Language", "切换语言")) { NotificationCenter.default.post(name: .toggleLanguage, object: nil) }
                    .keyboardShortcut("l", modifiers: [.command, .option])
            }
        }
        MenuBarExtra {
            MenuBarView(supervisor: supervisor)
        } label: {
            Label("Trackpad Companion", systemImage: supervisor.state.symbol)
        }
    }
}

struct SettingsView: View {
    @StateObject private var model = SettingsModel()
    @EnvironmentObject private var supervisor: ServiceSupervisor

    var body: some View {
        NavigationSplitView {
            VStack(alignment: .leading, spacing: 0) {
                SidebarHeader(language: model.language, state: supervisor.state)
                List(SettingsSection.allCases, selection: $model.selectedSection) { section in
                    Label {
                        Text(section.title(model.language))
                    } icon: {
                        Image(systemName: icon(for: section))
                            .symbolRenderingMode(.hierarchical)
                            .foregroundStyle(section == model.selectedSection ? Color.accentColor : Color.secondary)
                    }
                    .tag(section)
                    .padding(.vertical, 3)
                }
                .listStyle(.sidebar)
                .scrollContentBackground(.hidden)
            }
            .navigationTitle(model.language.text("Trackpad", "触控板"))
            .safeAreaInset(edge: .bottom) {
                HStack {
                    languageMenu
                    Spacer()
                    Button {
                        model.reload()
                        supervisor.refreshConnectionSettings()
                    } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                    .accessibilityLabel(model.language.text("Reload", "重载"))
                    .help(model.language.text("Reload configuration", "从磁盘重载配置"))
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 12)
                .background(.bar)
            }
        } detail: {
            ZStack {
                Color(nsColor: .windowBackgroundColor)
                    .ignoresSafeArea()
                Form {
                    Section {
                        Text(model.selectedSection.subtitle(model.language))
                            .font(.callout)
                            .foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    if model.selectedSection == .overview { overview } else { sectionForm(model.selectedSection) }
                    if let error = model.error {
                        Section { Label(error, systemImage: "exclamationmark.triangle").foregroundStyle(.red) }
                    }
                    Section {
                        Label {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(model.language.text("Configuration file", "配置文件"))
                                    .font(.caption.weight(.semibold))
                                Text(model.configPath.isEmpty ? model.language.text("Not available", "不可用") : model.configPath)
                                    .font(.caption.monospaced())
                                    .foregroundStyle(.secondary)
                                    .textSelection(.enabled)
                            }
                        } icon: {
                            Image(systemName: "doc.text")
                                .foregroundStyle(.secondary)
                        }
                    }
                }
                .formStyle(.grouped)
                .scrollContentBackground(.hidden)
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
            }
            .navigationTitle(model.selectedSection.title(model.language))
            .toolbar {
                ToolbarItem(placement: .status) {
                    StatusBadge(state: supervisor.state, language: model.language)
                }
                ToolbarItem(placement: .automatic) {
                    Button(model.language.text("Reload", "重载"), systemImage: "arrow.clockwise") {
                        model.reload()
                        supervisor.refreshConnectionSettings()
                    }
                        .disabled(model.isSaving)
                        .help(model.language.text("Reload configuration from disk", "从磁盘重新加载配置"))
                }
            }
        }
        .navigationSplitViewStyle(.balanced)
        .environment(\.locale, Locale(identifier: model.language.localeIdentifier))
        .onAppear {
            supervisor.refreshPermissions()
            supervisor.refreshLaunchAtLogin()
        }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            supervisor.refreshPermissions()
            supervisor.refreshLaunchAtLogin()
            if supervisor.state == .waitingForPermission && supervisor.accessibilityGranted {
                supervisor.start()
            }
        }
        .task {
            // Load after StateObject installation. Mutating @Published values
            // from SettingsModel.init re-enters SwiftUI's AttributeGraph.
            model.reload()
            supervisor.refreshConnectionSettings()
        }
    }

    @ViewBuilder
    private func sectionForm(_ section: SettingsSection) -> some View {
        switch section {
        case .overview: overview
        case .connections: connections
        case .pointAndClick: pointAndClick
        case .scrollAndZoom: scrollAndZoom
        case .moreGestures: moreGestures
        case .companion: companion
        }
    }

    private var languageMenu: some View {
        Menu {
            ForEach(AppLanguage.allCases) { option in
                Button {
                    model.language = option
                } label: {
                    HStack {
                        Text(option.rawValue)
                        Spacer(minLength: 18)
                        if model.language == option {
                            Image(systemName: "checkmark")
                        }
                    }
                }
            }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "globe")
                Text(model.language.rawValue)
                    .lineLimit(1)
            }
            .frame(minWidth: 76, alignment: .leading)
        }
        .menuStyle(.borderlessButton)
        .fixedSize(horizontal: true, vertical: false)
        .help(model.language.text("Choose language", "选择语言"))
        .accessibilityLabel(model.language.text("Language", "语言"))
    }

    @ViewBuilder
    private var overview: some View {
            Section {
                OverviewHero(state: supervisor.state, language: model.language)
            }
            Section {
                HStack(spacing: 12) {
                    MetricTile(
                        title: model.language.text("Service", "服务"),
                        value: serviceLabel,
                        symbol: supervisor.state.symbol,
                        tint: statusColor
                    )
                    MetricTile(
                        title: model.language.text("Pairing", "配对"),
                        value: supervisor.phoneEnabled
                            ? (supervisor.tokenConfigured ? model.language.text("Protected", "已保护") : model.language.text("Manual", "手动"))
                            : model.language.text("Off", "已关闭"),
                        symbol: supervisor.phoneEnabled ? (supervisor.tokenConfigured ? "lock.shield" : "lock.open") : "minus.circle",
                        tint: supervisor.phoneEnabled ? (supervisor.tokenConfigured ? .green : .orange) : .secondary
                    )
                }
            }
            Section {
                LabeledContent(model.language.text("Service", "服务")) {
                    Label(model.language.text(supervisor.state == .running ? "Running" : supervisor.state.rawValue.capitalized, supervisor.state == .running ? "运行中" : supervisor.state.rawValue), systemImage: supervisor.state.symbol)
                        .foregroundStyle(supervisor.state == .failed ? .red : supervisor.state == .running ? .green : .secondary)
                }
                LabeledContent(model.language.text("Network", "网络")) {
                    Label(
                        supervisor.networkAvailable
                            ? (supervisor.networkInterface.isEmpty ? model.language.text("Available", "可用") : supervisor.networkInterface)
                            : model.language.text("Unavailable", "不可用"),
                        systemImage: supervisor.networkAvailable ? "wifi" : "wifi.exclamationmark"
                    )
                    .foregroundStyle(supervisor.networkAvailable ? .secondary : .orange)
                }
                LabeledContent(model.language.text("Web", "Web")) {
                    if supervisor.webEnabled && !supervisor.endpoint.isEmpty {
                        Text(supervisor.endpoint).font(.caption.monospaced()).textSelection(.enabled)
                    } else {
                        Text(model.language.text("Off", "已关闭")).foregroundStyle(.secondary)
                    }
                }
                if supervisor.phoneEnabled && !supervisor.pairingURI.isEmpty {
                    LabeledContent(model.language.text("Pairing link", "配对链接")) {
                        Text(supervisor.pairingURI).font(.caption.monospaced()).textSelection(.enabled)
                    }
                }
                if let recent = supervisor.recentConnection {
                    LabeledContent(model.language.text("Last connection", "最近连接")) {
                        VStack(alignment: .trailing, spacing: 2) {
                            Text("\(recent.host):\(recent.port)")
                                .font(.caption.monospaced())
                            Text(model.language.text("Used \(relativeTime(recent.lastConnectedAt))", "使用于 \(relativeTime(recent.lastConnectedAt))"))
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
            Section {
                HStack {
                    Button(model.language.text("Start", "启动"), systemImage: "play.fill") { supervisor.start() }
                        .disabled(supervisor.state == .running || supervisor.state == .starting)
                    Button(model.language.text("Stop", "停止"), systemImage: "stop.fill") { supervisor.stop() }
                        .disabled(supervisor.state == .stopped)
                    Button(model.language.text("Restart", "重启"), systemImage: "arrow.clockwise") { supervisor.restart() }
                }
            }
            Section(model.language.text("Permissions", "权限")) {
                Label(model.language.text(supervisor.accessibilityGranted ? "Accessibility is ready." : "Accessibility is required for synthetic cursor, click, scroll, and gesture events.", supervisor.accessibilityGranted ? "辅助功能权限已就绪。" : "合成光标、点击、滚动和手势事件需要辅助功能权限。"), systemImage: supervisor.accessibilityGranted ? "checkmark.shield" : "hand.raised")
                    .font(.callout)
                    .foregroundStyle(supervisor.accessibilityGranted ? .green : .primary)
                if !supervisor.accessibilityGranted {
                    PermissionFlowButton(
                        pane: .accessibility,
                        suggestedAppURLs: [Bundle.main.bundleURL],
                        configuration: PermissionFlowConfiguration(
                            promptForAccessibilityTrust: false,
                            localeIdentifier: model.language.localeIdentifier
                        )
                    ) { state in
                        Label(
                            model.language.text("Open Accessibility Settings", "打开辅助功能设置"),
                            systemImage: state.isGranted ? "checkmark.circle.fill" : "arrow.up.forward.app"
                        )
                    }
                }
            }
            Section(model.language.text("Login item", "登录项")) {
                LabeledContent(model.language.text("Start with macOS", "随 macOS 启动")) {
                    Label(
                        supervisor.launchAtLogin
                            ? model.language.text("Enabled", "已启用")
                            : supervisor.launchAtLoginRequiresApproval
                                ? model.language.text("Needs approval", "待批准")
                                : model.language.text("Off", "已关闭"),
                        systemImage: supervisor.launchAtLogin ? "checkmark.circle.fill" : supervisor.launchAtLoginRequiresApproval ? "exclamationmark.circle" : "circle"
                    )
                    .foregroundStyle(supervisor.launchAtLogin ? .green : supervisor.launchAtLoginRequiresApproval ? .orange : .secondary)
                }
                if supervisor.launchAtLoginRequiresApproval {
                    Text(model.language.text("Approve this app in System Settings → General → Login Items.", "请在系统设置 → 通用 → 登录项中批准此应用。"))
                        .font(.caption)
                        .foregroundStyle(.orange)
                }
            }
            if !supervisor.pairingURI.isEmpty {
                Section(model.language.text("Pairing", "配对")) {
                    Button(model.language.text("Copy pairing link", "复制配对链接"), systemImage: "doc.on.doc") { supervisor.copyPairingURI() }
                        .help(model.language.text("Paste this link into the Android app or a QR generator on the same LAN.", "可将此链接粘贴到 Android 应用或局域网内的二维码工具。"))
                }
            }
            Section(model.language.text("Diagnostics", "诊断")) {
                HStack {
                    Button(model.language.text("Refresh", "刷新"), systemImage: "arrow.clockwise") { supervisor.refreshDiagnostics() }
                    Button(model.language.text("Copy report", "复制报告"), systemImage: "doc.on.doc") { supervisor.copyDiagnostics() }
                    Button(model.language.text("Open logs", "打开日志"), systemImage: "folder") { supervisor.openLogs() }
                }
                if !supervisor.diagnostics.isEmpty {
                    Text(supervisor.diagnostics)
                        .font(.caption.monospaced())
                        .textSelection(.enabled)
                }
            }
            Section(model.language.text("Live activity", "实时活动")) {
                LabeledContent(model.language.text("UDP datagrams", "UDP 数据报")) {
                    Text(supervisor.metrics.udpDatagrams.formatted(.number))
                        .monospacedDigit()
                }
                LabeledContent(model.language.text("WebSocket frames", "WebSocket 帧")) {
                    Text(supervisor.metrics.websocketFrames.formatted(.number))
                        .monospacedDigit()
                }
                LabeledContent(model.language.text("Frames to engine", "送入引擎的帧")) {
                    Text(supervisor.metrics.engineFrames.formatted(.number))
                        .monospacedDigit()
                }
                LabeledContent(model.language.text("Decode errors", "解码错误")) {
                    Text(supervisor.metrics.decodeErrors.formatted(.number))
                        .monospacedDigit()
                        .foregroundStyle(supervisor.metrics.decodeErrors == 0 ? .secondary : .orange)
                }
                if let updatedAt = supervisor.metrics.updatedAt {
                    Text(model.language.text(
                        "Updated \(relativeTime(updatedAt))",
                        "更新于 \(relativeTime(updatedAt))"
                    ))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
            }
            if !supervisor.message.isEmpty {
                Section(model.language.text("Latest status", "最新状态")) {
                    Text(supervisor.message).font(.caption.monospaced()).textSelection(.enabled)
                }
            }
            Section {
                Label {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(model.language.text("Works without a physical trackpad", "没有实体触控板也能使用"))
                            .font(.subheadline.weight(.semibold))
                        Text(model.language.text("Mac mini does not show Apple's Trackpad pane. Trackpad Companion keeps its own settings here and drives the virtual input surface directly.", "Mac mini 没有 Apple 的触控板设置页。Trackpad Companion 在这里管理自己的参数，直接驱动虚拟输入表面。"))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                } icon: {
                    Image(systemName: "info.circle.fill")
                        .foregroundStyle(.tint)
                }
            }
    }

    @ViewBuilder
    private var connections: some View {
            Section {
                Text(model.language.text(
                    "Choose which services this Mac exposes on the local network. Changes apply immediately when the helper is running.",
                    "选择这台 Mac 要在局域网开放的服务。服务运行中修改会立即生效。"
                ))
                .font(.callout)
                .foregroundStyle(.secondary)
            }
            Section(model.language.text("Services", "服务")) {
                ConnectionToggleRow(
                    path: "net.web_enabled",
                    title: "Web access",
                    titleCN: "Web 访问",
                    description: "Serve the browser touchpad and WebSocket input.",
                    descriptionCN: "提供浏览器触控板页面和 WebSocket 输入。",
                    model: model,
                    onChange: { _ in supervisor.applyConnectionConfiguration() }
                )
                ConnectionToggleRow(
                    path: "net.phone_enabled",
                    title: "Phone access",
                    titleCN: "手机连接",
                    description: "Accept the native phone app over authenticated UDP.",
                    descriptionCN: "允许手机应用通过受保护的 UDP 连接。",
                    model: model,
                    onChange: { _ in supervisor.applyConnectionConfiguration() }
                )
            }
            Section(model.language.text("Web", "Web")) {
                connectionStatus(
                    title: model.language.text("Browser touchpad", "浏览器触控板"),
                    enabled: supervisor.webEnabled,
                    active: supervisor.webEnabled && supervisor.boundPort != nil,
                    symbol: "safari",
                    detail: supervisor.webEnabled && !supervisor.endpoint.isEmpty
                        ? supervisor.endpoint
                        : model.language.text("Not available", "未开放")
                )
                if supervisor.webEnabled && !supervisor.endpoint.isEmpty {
                    Button(model.language.text("Copy Web URL", "复制 Web 地址"), systemImage: "doc.on.doc") {
                        supervisor.copyWebURL()
                    }
                }
            }
            Section(model.language.text("Phone", "手机")) {
                connectionStatus(
                    title: model.language.text("Native phone connection", "手机触控板"),
                    enabled: supervisor.phoneEnabled,
                    active: supervisor.phoneEnabled && supervisor.boundPort != nil,
                    symbol: "iphone",
                    detail: supervisor.phoneEnabled
                        ? (supervisor.tokenConfigured
                            ? model.language.text("Protected and discoverable", "已保护，可被发现"): model.language.text("Available without a token", "可用，但未配置 Token"))
                        : model.language.text("Not available", "未开放")
                )
                if supervisor.phoneEnabled && !supervisor.pairingURI.isEmpty {
                    Button(model.language.text("Copy pairing link", "复制配对链接"), systemImage: "qrcode") {
                        supervisor.copyPairingURI()
                    }
                }
            }
            Section(model.language.text("Security", "安全")) {
                Label(
                    model.language.text(
                        supervisor.tokenConfigured ? "A pairing token protects both enabled services." : "No token is configured; anyone on the bound network can connect.",
                        supervisor.tokenConfigured ? "配对 Token 会保护所有已开放的服务。" : "未配置 Token；绑定网络上的设备都可以连接。"
                    ),
                    systemImage: supervisor.tokenConfigured ? "lock.shield" : "exclamationmark.shield"
                )
                .foregroundStyle(supervisor.tokenConfigured ? .green : .orange)
                if !supervisor.tokenConfigured && (supervisor.webEnabled || supervisor.phoneEnabled) {
                    Button(model.language.text("Create a pairing token", "创建配对 Token"), systemImage: "key.fill") {
                        supervisor.refreshConnectionSettings()
                    }
                }
            }
    }

    @ViewBuilder
    private func connectionStatus(title: String, enabled: Bool, active: Bool, symbol: String, detail: String) -> some View {
        LabeledContent {
            VStack(alignment: .trailing, spacing: 3) {
                Label(
                    active ? model.language.text("Listening", "监听中") : enabled ? model.language.text("Enabled", "已启用") : model.language.text("Off", "已关闭"),
                    systemImage: active ? "checkmark.circle.fill" : enabled ? "circle" : "minus.circle"
                )
                .foregroundStyle(
                    active
                        ? Color.green
                        : enabled
                            ? Color.secondary
                            : Color(nsColor: .tertiaryLabelColor)
                )
                Text(detail)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
        } label: {
            Label(title, systemImage: symbol)
        }
    }

    private var serviceLabel: String {
        switch supervisor.state {
        case .running: return model.language.text("Ready", "就绪")
        case .starting: return model.language.text("Starting", "启动中")
        case .waitingForPermission: return model.language.text("Permission", "待授权")
        case .degraded: return model.language.text("Degraded", "受限")
        case .failed: return model.language.text("Attention", "需处理")
        case .stopped: return model.language.text("Stopped", "已停止")
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

    private func relativeTime(_ date: Date) -> String {
        let formatter = RelativeDateTimeFormatter()
        formatter.locale = Locale(identifier: model.language.localeIdentifier)
        return formatter.localizedString(for: date, relativeTo: Date())
    }

    @ViewBuilder
    private var pointAndClick: some View {
            SliderRow(title: "Tracking speed", titleCN: "跟踪速度", value: Binding(get: { model.number("cursor.sensitivity", default: 28) }, set: { model.set("cursor.sensitivity", value: String(format: "%.1f", $0)) }), range: 5...80, language: model.language)
            UnavailableRow(title: "Click", titleCN: "点按力度", description: "Hardware-only setting; unavailable for virtual input.", descriptionCN: "仅适用于实体触控板；虚拟输入不可用。", language: model.language)
            UnavailableRow(title: "Quiet Click", titleCN: "静音点按", description: "Hardware-only setting; unavailable for virtual input.", descriptionCN: "仅适用于实体触控板；虚拟输入不可用。", language: model.language)
            ToggleRow(path: "gestures.tap_to_click", title: "Tap to click", titleCN: "轻点来点按", description: "Tap with one finger to click.", descriptionCN: "用一个手指轻点触控板来点按。", model: model)
            ToggleRow(path: "gestures.secondary_click", title: "Secondary click", titleCN: "辅助点按", description: "Click or tap with two fingers.", descriptionCN: "用两个手指点按来打开辅助菜单。", model: model)
            ToggleRow(path: "gestures.dictionary_lookup", title: "Look up & data detectors", titleCN: "查询与数据检测器", description: "Use a gesture to look up words and detect data.", descriptionCN: "使用手势查询单词并检测数据。", model: model)
            PickerRow(path: "macos.haptic_feedback", title: "Force Click and haptic feedback", titleCN: "用力点按与触觉反馈", options: [("auto", "Automatic", "自动"), ("on", "On", "打开"), ("off", "Off", "关闭")], model: model)
    }

    @ViewBuilder
    private var scrollAndZoom: some View {
            ToggleRow(path: "scroll.natural", title: "Natural scrolling", titleCN: "自然滚动", description: "Move contents in the same direction as your fingers.", descriptionCN: "让窗口内容与手指移动方向一致。", model: model)
            ToggleRow(path: "scroll.enable", title: "Trackpad scrolling", titleCN: "触控板滚动", description: "Emit two-finger scroll events.", descriptionCN: "发送双指滚动事件。", model: model)
            SliderRow(title: "Scroll sensitivity", titleCN: "滚动灵敏度", value: Binding(get: { model.number("scroll.sensitivity", default: 20) }, set: { model.set("scroll.sensitivity", value: String(format: "%.1f", $0)) }), range: 5...80, language: model.language)
            ToggleRow(path: "scroll.momentum", title: "Momentum scrolling", titleCN: "惯性滚动", description: "Continue scrolling after fingers lift.", descriptionCN: "抬指后继续滚动一小段惯性。", model: model)
            ToggleRow(path: "scroll.horizontal", title: "Horizontal scrolling", titleCN: "水平滚动", description: "Preserve horizontal scroll deltas.", descriptionCN: "保留水平滚动分量。", model: model)
            ToggleRow(path: "gestures.pinch.enable", title: "Zoom in or out", titleCN: "放大或缩小", description: "Pinch with two fingers to zoom.", descriptionCN: "用两个手指捏合来缩放。", model: model)
            ToggleRow(path: "gestures.smart_zoom", title: "Smart zoom", titleCN: "智能缩放", description: "Double-tap with two fingers to zoom.", descriptionCN: "用两个手指轻点两下以智能缩放。", model: model)
            ToggleRow(path: "gestures.rotate.enable", title: "Rotate", titleCN: "旋转", description: "Rotate items with two fingers.", descriptionCN: "用两个手指旋转项目。", model: model)
            PickerRow(path: "scroll.modifier_zoom_mask", title: "Zoom modifier", titleCN: "缩放修饰键", options: [("0", "Default (Cmd/Ctrl)", "默认（Command/Control）"), ("262144", "Control", "Control"), ("524288", "Option", "Option"), ("1048576", "Command", "Command")], model: model)
    }

    @ViewBuilder
    private var moreGestures: some View {
            ToggleRow(path: "gestures.swipe.horizontal.enable", title: "Swipe between pages", titleCN: "在页面之间轻扫", description: "Swipe between document pages.", descriptionCN: "在文档页面之间左右轻扫。", model: model)
            ToggleRow(path: "gestures.swipe.vertical.enable", title: "Mission Control", titleCN: "调度中心", description: "Swipe up to open Mission Control.", descriptionCN: "向上轻扫以打开调度中心。", model: model)
            ToggleRow(path: "gestures.right_edge_swipe", title: "Notification Center", titleCN: "通知中心", description: "Swipe from the right edge for notifications.", descriptionCN: "从右边缘向左轻扫以显示通知中心。", model: model)
    }

    @ViewBuilder
    private var companion: some View {
            ToggleRow(path: "gestures.three_finger_drag.enable", title: "Three-finger drag", titleCN: "三指拖移", description: "Hold a virtual click while three fingers move.", descriptionCN: "三指移动时保持虚拟点按。", model: model)
            SliderRow(title: "Drag-lock delay", titleCN: "拖移锁定延迟", value: Binding(get: { model.number("gestures.three_finger_drag.release_delay_ms", default: 500) }, set: { model.set("gestures.three_finger_drag.release_delay_ms", value: String(Int($0))) }), range: 0...2000, language: model.language, unit: "ms")
            ToggleRow(path: "gestures.one_finger_tap_drag.enable", title: "One-finger tap-drag", titleCN: "单指轻点拖移", description: "Double-tap and hold to drag.", descriptionCN: "单指双击后保持按住并拖移。", model: model)
            ToggleRow(path: "gestures.press_and_hold_drag.enable", title: "Press-and-hold drag", titleCN: "按住拖移", description: "Accessibility-style stationary press drag.", descriptionCN: "无移动时按住即可开始拖移。", model: model)
            SliderRow(title: "Acceleration curve", titleCN: "加速度曲线", value: Binding(get: { model.number("cursor.accel_exponent", default: 1.35) }, set: { model.set("cursor.accel_exponent", value: String(format: "%.2f", $0)) }), range: 1...2, language: model.language)
            SliderRow(title: "Acceleration reference", titleCN: "加速度参考速度", value: Binding(get: { model.number("cursor.accel_ref", default: 70) }, set: { model.set("cursor.accel_ref", value: String(format: "%.1f", $0)) }), range: 20...200, language: model.language, unit: "mm/s")
            ToggleRow(path: "scroll.shift_scroll_horizontal", title: "Shift scroll compatibility", titleCN: "Shift 滚动兼容", description: "Optional remap; native mode keeps the original axis.", descriptionCN: "可选兼容转换；原生模式保留原始滚动轴。", model: model)
            ToggleRow(path: "macos.sync_system_settings", title: "Sync macOS settings", titleCN: "同步 macOS 设置", description: "Use available macOS preferences as startup defaults.", descriptionCN: "启动时读取可用的 macOS 偏好作为默认值。", model: model)
            Toggle(isOn: Binding(
                get: { supervisor.launchAtLogin },
                set: { supervisor.setLaunchAtLogin($0) }
            )) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(model.language.text("Launch at login", "登录时启动"))
                    Text(model.language.text(
                        "Start the menu-bar companion when you sign in.",
                        "登录 Mac 后自动启动菜单栏伴侣。"
                    ))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
            }
            if supervisor.launchAtLoginRequiresApproval {
                Text(model.language.text(
                    "Approve this app in System Settings > General > Login Items.",
                    "请在系统设置 > 通用 > 登录项中批准此应用。"
                ))
                .font(.caption)
                .foregroundStyle(.orange)
            }
    }

    private func icon(for section: SettingsSection) -> String {
        switch section {
        case .overview: return "rectangle.3.group"
        case .connections: return "point.3.connected.trianglepath.dotted"
        case .pointAndClick: return "cursorarrow"
        case .scrollAndZoom: return "arrow.up.and.down"
        case .moreGestures: return "hand.tap"
        case .companion: return "slider.horizontal.3"
        }
    }
}
