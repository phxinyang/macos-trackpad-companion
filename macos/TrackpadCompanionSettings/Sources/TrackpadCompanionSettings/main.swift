import Foundation
import SwiftUI
#if os(macOS)
import AppKit
#endif

@main
struct TrackpadCompanionSettingsApp: App {
    var body: some Scene {
        WindowGroup {
            SettingsView()
                .frame(minWidth: 760, minHeight: 520)
        }
        .windowResizability(.contentSize)
        .commands {
            CommandGroup(after: .appSettings) {
                Button("Toggle Language") { NotificationCenter.default.post(name: .toggleLanguage, object: nil) }
                    .keyboardShortcut("l", modifiers: [.command, .option])
            }
        }
    }
}

extension Notification.Name {
    static let toggleLanguage = Notification.Name("TrackpadCompanionSettings.toggleLanguage")
}

enum AppLanguage: String, CaseIterable, Identifiable, Hashable {
    case english = "English"
    case chinese = "中文"
    var id: String { rawValue }

    func text(_ english: String, _ chinese: String) -> String {
        self == .english ? english : chinese
    }
}

enum SettingsSection: String, CaseIterable, Identifiable, Hashable {
    case overview, pointAndClick, scrollAndZoom, moreGestures, companion
    var id: String { rawValue }

    func title(_ language: AppLanguage) -> String {
        switch self {
        case .overview: return language.text("Overview", "总览")
        case .pointAndClick: return language.text("Point & Click", "点按与点击")
        case .scrollAndZoom: return language.text("Scroll & Zoom", "滚动与缩放")
        case .moreGestures: return language.text("More Gestures", "更多手势")
        case .companion: return language.text("Companion", "Companion 扩展")
        }
    }

    func subtitle(_ language: AppLanguage) -> String {
        switch self {
        case .overview: return language.text("Service status, pairing, and permissions", "服务状态、配对与权限")
        case .pointAndClick: return language.text("Pointer tracking, clicking, lookup, and haptic feedback", "指针跟踪、点击、查词与触觉反馈")
        case .scrollAndZoom: return language.text("Natural scrolling, zooming, rotation, and momentum", "自然滚动、缩放、旋转与惯性")
        case .moreGestures: return language.text("Pages, Spaces, Mission Control, and Notification Center", "页面、Space、调度中心与通知中心")
        case .companion: return language.text("Virtual-input controls not present in macOS Trackpad settings", "虚拟输入专属控制，不冒充 macOS 原生选项")
        }
    }
}

enum ServiceState: String {
    case stopped, starting, running, failed

    var symbol: String {
        switch self { case .stopped: return "circle", .starting: return "arrow.triangle.2.circlepath", .running: return "checkmark.circle.fill", .failed: return "exclamationmark.triangle.fill" }
    }
}

@MainActor
final class ServiceSupervisor: ObservableObject {
    @Published private(set) var state: ServiceState = .stopped
    @Published private(set) var message = ""
    @Published private(set) var endpoint = "http://localhost:4242/"
    private var process: Process?
    private var outputPipe: Pipe?

    deinit {
        outputPipe?.fileHandleForReading.readabilityHandler = nil
        process?.terminate()
    }

    func start() {
        guard process?.isRunning != true else { return }
        guard let executable = locate("COMPANION_NET_BIN", bundledName: "companion-net") else {
            state = .failed
            message = "companion-net was not found. Build the Rust daemon or set COMPANION_NET_BIN."
            return
        }
        let child = Process()
        child.executableURL = URL(fileURLWithPath: executable)
        let pipe = Pipe()
        child.standardOutput = pipe
        child.standardError = pipe
        outputPipe = pipe
        pipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty else { return }
            let text = String(data: data, encoding: .utf8) ?? ""
            Task { @MainActor in
                self?.message = text.trimmingCharacters(in: .whitespacesAndNewlines)
                if text.contains("listening on") { self?.state = .running }
            }
        }
        child.terminationHandler = { [weak self] process in
            Task { @MainActor in
                guard let self else { return }
                self.process = nil
                if process.terminationStatus == 0 || self.state == .stopped { self.state = .stopped }
                else { self.state = .failed; self.message = "companion-net exited with status \(process.terminationStatus)." }
            }
        }
        do {
            try child.run()
            process = child
            state = .starting
            message = "Starting companion-net…"
        } catch {
            state = .failed
            message = error.localizedDescription
        }
    }

    func stop() {
        guard let process, process.isRunning else { state = .stopped; return }
        state = .stopped
        message = "Service stopped"
        process.terminate()
        self.process = nil
    }

    func restart() { stop(); start() }

    func openAccessibilitySettings() {
        let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!
        NSWorkspace.shared.open(url)
    }

    private func locate(_ environmentKey: String, bundledName: String) -> String? {
        let candidates = [
            ProcessInfo.processInfo.environment[environmentKey],
            Bundle.main.url(forResource: bundledName, withExtension: nil)?.path,
            "/opt/homebrew/bin/\(bundledName)",
            "/usr/local/bin/\(bundledName)",
            NSHomeDirectory() + "/.cargo/bin/\(bundledName)",
        ].compactMap { $0 }
        return candidates.first(where: { FileManager.default.isExecutableFile(atPath: $0) })
    }
}

@MainActor
final class SettingsModel: ObservableObject {
    @Published var language: AppLanguage = .chinese
    @Published var selectedSection: SettingsSection = .pointAndClick
    @Published var selectedPath: String?
    @Published var error: String?
    @Published var isSaving = false
    @Published private(set) var values: [String: Any] = [:]
    var configPath: String = ""
    private var languageObserver: NSObjectProtocol?

    init() {
        languageObserver = NotificationCenter.default.addObserver(forName: .toggleLanguage, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                self.language = self.language == .english ? .chinese : .english
            }
        }
        reload()
    }

    deinit { if let languageObserver { NotificationCenter.default.removeObserver(languageObserver) } }

    func reload() {
        do {
            let json = try runHelper(["dump"])
            guard let root = try JSONSerialization.jsonObject(with: json) as? [String: Any],
                  let config = root["config"] as? [String: Any] else { throw HelperError.invalidOutput }
            values = flatten(config)
            configPath = root["path"] as? String ?? ""
            error = nil
        } catch { error = error.localizedDescription }
    }

    func bool(_ path: String, default fallback: Bool = false) -> Bool { values[path] as? Bool ?? fallback }
    func toggle(_ path: String, default fallback: Bool = false) -> Bool {
        if let value = values[path] as? Bool { return value }
        if let value = values[path] as? String { return value == "on" || value == "true" }
        return fallback
    }
    func number(_ path: String, default fallback: Double = 0) -> Double {
        if let value = values[path] as? NSNumber { return value.doubleValue }
        return fallback
    }
    func string(_ path: String, default fallback: String = "") -> String {
        if let value = values[path] as? String { return value }
        if let value = values[path] as? NSNumber { return String(value.intValue) }
        return fallback
    }

    func set(_ path: String, value: String) {
        isSaving = true
        do {
            _ = try runHelper(["set", "--path", path, "--value", value])
            values[path] = scalar(value)
            error = nil
        } catch { error = error.localizedDescription }
        isSaving = false
    }

    private func scalar(_ value: String) -> Any {
        if value == "true" { return true }
        if value == "false" { return false }
        if let number = Double(value) { return number }
        return value
    }

    private func flatten(_ value: [String: Any], prefix: String = "") -> [String: Any] {
        var result: [String: Any] = [:]
        for (key, child) in value {
            let path = prefix.isEmpty ? key : "\(prefix).\(key)"
            if let table = child as? [String: Any] { result.merge(flatten(table, prefix: path)) { _, new in new } }
            else { result[path] = child }
        }
        return result
    }

    private enum HelperError: LocalizedError {
        case unavailable, invalidOutput, failed(String)
        var errorDescription: String? {
            switch self {
            case .unavailable: return "companion-config was not found. Build the Rust helper or set COMPANION_CONFIG_BIN."
            case .invalidOutput: return "The companion-config response was invalid."
            case .failed(let message): return message
            }
        }
    }

    private func runHelper(_ arguments: [String]) throws -> Data {
        let process = Process()
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        process.arguments = arguments
        let candidates = [
            ProcessInfo.processInfo.environment["COMPANION_CONFIG_BIN"],
            Bundle.main.url(forResource: "companion-config", withExtension: nil)?.path,
            "/opt/homebrew/bin/companion-config",
            "/usr/local/bin/companion-config",
            NSHomeDirectory() + "/.cargo/bin/companion-config",
        ].compactMap { $0 }
        guard let executable = candidates.first(where: { FileManager.default.isExecutableFile(atPath: $0) }) else { throw HelperError.unavailable }
        process.executableURL = URL(fileURLWithPath: executable)
        try process.run()
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw HelperError.failed(String(data: data, encoding: .utf8) ?? "companion-config failed")
        }
        return data
    }
}

struct SettingsView: View {
    @StateObject private var model = SettingsModel()
    @StateObject private var supervisor = ServiceSupervisor()

    var body: some View {
        NavigationSplitView {
            List(SettingsSection.allCases, selection: $model.selectedSection) { section in
                Label(section.title(model.language), systemImage: icon(for: section))
                    .tag(section)
            }
            .navigationTitle(model.language.text("Trackpad", "触控板"))
            .safeAreaInset(edge: .bottom) {
                HStack {
                    Picker(model.language.text("Language", "语言"), selection: $model.language) {
                        ForEach(AppLanguage.allCases) { language in Text(language.rawValue).tag(language) }
                    }
                    .pickerStyle(.menu)
                    Spacer()
                    Button(model.language.text("Reload", "重载"), systemImage: "arrow.clockwise") { model.reload() }
                        .help(model.language.text("Reload configuration", "从磁盘重载配置"))
                }
                .padding(10)
            }
        } detail: {
            Form {
                Section {
                    Text(model.selectedSection.subtitle(model.language))
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                if model.selectedSection == .overview { overview } else { sectionForm(model.selectedSection) }
                if let error = model.error {
                    Section { Label(error, systemImage: "exclamationmark.triangle").foregroundStyle(.red) }
                }
                Section {
                    Text(model.language.text("Changes are written to", "修改将写入"))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(model.configPath)
                        .font(.caption.monospaced())
                        .textSelection(.enabled)
                }
            }
            .formStyle(.grouped)
            .navigationTitle(model.selectedSection.title(model.language))
            .toolbar {
                ToolbarItem(placement: .automatic) {
                    Button(model.language.text("Reload", "重载"), systemImage: "arrow.clockwise") { model.reload() }
                        .disabled(model.isSaving)
                }
            }
        }
        .onAppear { supervisor.start() }
    }

    @ViewBuilder
    private func sectionForm(_ section: SettingsSection) -> some View {
        switch section {
        case .overview: overview
        case .pointAndClick: pointAndClick
        case .scrollAndZoom: scrollAndZoom
        case .moreGestures: moreGestures
        case .companion: companion
        }
    }

    private var overview: some View {
        Group {
            Section {
                LabeledContent(model.language.text("Service", "服务")) {
                    Label(model.language.text(supervisor.state == .running ? "Running" : supervisor.state.rawValue.capitalized, supervisor.state == .running ? "运行中" : supervisor.state.rawValue), systemImage: supervisor.state.symbol)
                        .foregroundStyle(supervisor.state == .failed ? .red : supervisor.state == .running ? .green : .secondary)
                }
                LabeledContent(model.language.text("Endpoint", "连接地址")) {
                    Text(supervisor.endpoint).font(.caption.monospaced()).textSelection(.enabled)
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
                Label(model.language.text("Accessibility is required for synthetic cursor, click, scroll, and gesture events.", "合成光标、点击、滚动和手势事件需要辅助功能权限。"), systemImage: "hand.raised")
                    .font(.callout)
                Button(model.language.text("Open Accessibility Settings", "打开辅助功能设置"), systemImage: "arrow.up.forward.app") { supervisor.openAccessibilitySettings() }
            }
            if !supervisor.message.isEmpty {
                Section(model.language.text("Latest status", "最新状态")) {
                    Text(supervisor.message).font(.caption.monospaced()).textSelection(.enabled)
                }
            }
        }
    }

    private var pointAndClick: some View {
        Group {
            SliderRow(title: "Tracking speed", titleCN: "跟踪速度", value: Binding(get: { model.number("cursor.sensitivity", default: 28) }, set: { model.set("cursor.sensitivity", value: String(format: "%.1f", $0)) }), range: 5...80, language: model.language)
            UnavailableRow(title: "Click", titleCN: "点按力度", description: "Hardware-only setting; unavailable for virtual input.", descriptionCN: "仅适用于实体触控板；虚拟输入不可用。", language: model.language)
            UnavailableRow(title: "Quiet Click", titleCN: "静音点按", description: "Hardware-only setting; unavailable for virtual input.", descriptionCN: "仅适用于实体触控板；虚拟输入不可用。", language: model.language)
            ToggleRow(path: "gestures.tap_to_click", title: "Tap to click", titleCN: "轻点来点按", description: "Tap with one finger to click.", descriptionCN: "用一个手指轻点触控板来点按。", model: model)
            ToggleRow(path: "gestures.secondary_click", title: "Secondary click", titleCN: "辅助点按", description: "Click or tap with two fingers.", descriptionCN: "用两个手指点按来打开辅助菜单。", model: model)
            ToggleRow(path: "gestures.dictionary_lookup", title: "Look up & data detectors", titleCN: "查询与数据检测器", description: "Use a gesture to look up words and detect data.", descriptionCN: "使用手势查询单词并检测数据。", model: model)
            PickerRow(path: "macos.haptic_feedback", title: "Force Click and haptic feedback", titleCN: "用力点按与触觉反馈", options: [("auto", "Automatic", "自动"), ("on", "On", "打开"), ("off", "Off", "关闭")], model: model)
        }
    }

    private var scrollAndZoom: some View {
        Group {
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
    }

    private var moreGestures: some View {
        Group {
            ToggleRow(path: "gestures.swipe.horizontal.enable", title: "Swipe between pages", titleCN: "在页面之间轻扫", description: "Swipe between document pages.", descriptionCN: "在文档页面之间左右轻扫。", model: model)
            ToggleRow(path: "gestures.swipe.vertical.enable", title: "Mission Control", titleCN: "调度中心", description: "Swipe up to open Mission Control.", descriptionCN: "向上轻扫以打开调度中心。", model: model)
            ToggleRow(path: "gestures.right_edge_swipe", title: "Notification Center", titleCN: "通知中心", description: "Swipe from the right edge for notifications.", descriptionCN: "从右边缘向左轻扫以显示通知中心。", model: model)
        }
    }

    private var companion: some View {
        Group {
            ToggleRow(path: "gestures.three_finger_drag.enable", title: "Three-finger drag", titleCN: "三指拖移", description: "Hold a virtual click while three fingers move.", descriptionCN: "三指移动时保持虚拟点按。", model: model)
            SliderRow(title: "Drag-lock delay", titleCN: "拖移锁定延迟", value: Binding(get: { model.number("gestures.three_finger_drag.release_delay_ms", default: 500) }, set: { model.set("gestures.three_finger_drag.release_delay_ms", value: String(Int($0))) }), range: 0...2000, language: model.language, unit: "ms")
            ToggleRow(path: "gestures.one_finger_tap_drag.enable", title: "One-finger tap-drag", titleCN: "单指轻点拖移", description: "Double-tap and hold to drag.", descriptionCN: "单指双击后保持按住并拖移。", model: model)
            ToggleRow(path: "gestures.press_and_hold_drag.enable", title: "Press-and-hold drag", titleCN: "按住拖移", description: "Accessibility-style stationary press drag.", descriptionCN: "无移动时按住即可开始拖移。", model: model)
            SliderRow(title: "Acceleration curve", titleCN: "加速度曲线", value: Binding(get: { model.number("cursor.accel_exponent", default: 1.35) }, set: { model.set("cursor.accel_exponent", value: String(format: "%.2f", $0)) }), range: 1...2, language: model.language)
            SliderRow(title: "Acceleration reference", titleCN: "加速度参考速度", value: Binding(get: { model.number("cursor.accel_ref", default: 70) }, set: { model.set("cursor.accel_ref", value: String(format: "%.1f", $0)) }), range: 20...200, language: model.language, unit: "mm/s")
            ToggleRow(path: "scroll.shift_scroll_horizontal", title: "Shift scroll compatibility", titleCN: "Shift 滚动兼容", description: "Optional remap; native mode keeps the original axis.", descriptionCN: "可选兼容转换；原生模式保留原始滚动轴。", model: model)
            ToggleRow(path: "macos.sync_system_settings", title: "Sync macOS settings", titleCN: "同步 macOS 设置", description: "Use available macOS preferences as startup defaults.", descriptionCN: "启动时读取可用的 macOS 偏好作为默认值。", model: model)
        }
    }

    private func icon(for section: SettingsSection) -> String {
        switch section {
        case .overview: return "rectangle.3.group"
        case .pointAndClick: return "cursorarrow"
        case .scrollAndZoom: return "arrow.up.and.down"
        case .moreGestures: return "hand.tap"
        case .companion: return "slider.horizontal.3"
        }
    }
}

struct ToggleRow: View {
    let path: String; let title: String; let titleCN: String; let description: String; let descriptionCN: String; @ObservedObject var model: SettingsModel
    var body: some View {
        Toggle(isOn: Binding(get: { model.toggle(path) }, set: { model.set(path, value: isBooleanPath ? ($0 ? "true" : "false") : ($0 ? "on" : "off")) })) {
            VStack(alignment: .leading, spacing: 3) { Text(model.language.text(title, titleCN)); Text(model.language.text(description, descriptionCN)).font(.caption).foregroundStyle(.secondary) }
        }
    }

    private var isBooleanPath: Bool { path.hasPrefix("scroll.") || path == "macos.sync_system_settings" }
}

struct UnavailableRow: View {
    let title: String; let titleCN: String; let description: String; let descriptionCN: String; let language: AppLanguage
    var body: some View {
        LabeledContent {
            Text(language.text("Unavailable", "不可用"))
                .foregroundStyle(.secondary)
        } label: {
            VStack(alignment: .leading, spacing: 3) {
                Text(language.text(title, titleCN))
                Text(language.text(description, descriptionCN)).font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}

struct SliderRow: View {
    let title: String; let titleCN: String; @Binding var value: Double; let range: ClosedRange<Double>; let language: AppLanguage; var unit = ""
    @State private var draft: Double = 0
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(language.text(title, titleCN)); Spacer()
                Text(String(format: "%.1f", draft) + (unit.isEmpty ? "" : " \(unit)")).foregroundStyle(.secondary).monospacedDigit()
            }
            Slider(value: $draft, in: range, onEditingChanged: { editing in if !editing { value = draft } })
        }
        .onAppear { draft = value }
        .onChange(of: value) { newValue in draft = newValue }
    }
}

struct PickerRow: View {
    let path: String; let title: String; let titleCN: String; let options: [(String, String, String)]; @ObservedObject var model: SettingsModel
    var body: some View {
        Picker(model.language.text(title, titleCN), selection: Binding(get: { model.string(path, default: "0") }, set: { model.set(path, value: $0) })) {
            ForEach(Array(options.enumerated()), id: \.offset) { item in
                let option = item.element
                Text(model.language.text(option.1, option.2)).tag(option.0)
            }
        }
    }
}
