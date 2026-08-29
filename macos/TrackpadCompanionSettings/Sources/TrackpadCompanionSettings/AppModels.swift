import Foundation

extension Notification.Name {
    static let toggleLanguage = Notification.Name("TrackpadCompanionSettings.toggleLanguage")
    static let configurationDidChange = Notification.Name("TrackpadCompanionSettings.configurationDidChange")
}

enum AppLanguage: String, CaseIterable, Identifiable, Hashable {
    case english = "English"
    case chinese = "中文"

    var id: String { rawValue }

    static var preferred: AppLanguage {
        if let saved = UserDefaults.standard.string(forKey: "TrackpadCompanion.language"),
           let language = AppLanguage(rawValue: saved) {
            return language
        }
        return Locale.current.language.languageCode?.identifier.lowercased().hasPrefix("zh") == true ? .chinese : .english
    }

    func text(_ english: String, _ chinese: String) -> String {
        self == .english ? english : chinese
    }

    /// PermissionFlow resolves its own panel copy through SwiftUI's locale.
    /// Keep this mapping in one place so the app menu and the floating panel
    /// never drift apart when the user switches language.
    var localeIdentifier: String {
        self == .english ? "en" : "zh-Hans"
    }
}

enum SettingsSection: String, CaseIterable, Identifiable, Hashable {
    case overview, connections, pointAndClick, scrollAndZoom, moreGestures, companion

    var id: String { rawValue }

    func title(_ language: AppLanguage) -> String {
        switch self {
        case .overview: return language.text("Overview", "总览")
        case .connections: return language.text("Connections", "连接")
        case .pointAndClick: return language.text("Point & Click", "点按与点击")
        case .scrollAndZoom: return language.text("Scroll & Zoom", "滚动与缩放")
        case .moreGestures: return language.text("More Gestures", "更多手势")
        case .companion: return language.text("Companion", "Companion 扩展")
        }
    }

    func subtitle(_ language: AppLanguage) -> String {
        switch self {
        case .overview: return language.text("Service status, pairing, and permissions", "服务状态、配对与权限")
        case .connections: return language.text("Choose which local services are available", "选择要开放的本地服务")
        case .pointAndClick: return language.text("Pointer tracking, clicking, lookup, and haptic feedback", "指针跟踪、点击、查词与触觉反馈")
        case .scrollAndZoom: return language.text("Natural scrolling, zooming, rotation, and momentum", "自然滚动、缩放、旋转与惯性")
        case .moreGestures: return language.text("Pages, Spaces, Mission Control, and Notification Center", "页面、Space、调度中心与通知中心")
        case .companion: return language.text("Virtual-input controls not present in macOS Trackpad settings", "虚拟输入专属控制，不冒充 macOS 原生选项")
        }
    }
}

enum ServiceState: String {
    case stopped, starting, waitingForPermission, running, degraded, failed

    var symbol: String {
        switch self {
        case .stopped: return "circle"
        case .starting: return "arrow.triangle.2.circlepath"
        case .waitingForPermission: return "hand.raised"
        case .running: return "checkmark.circle.fill"
        case .degraded: return "bolt.horizontal.circle"
        case .failed: return "exclamationmark.triangle.fill"
        }
    }
}

struct ServiceMetrics: Equatable {
    var udpDatagrams = 0
    var websocketFrames = 0
    var decodeErrors = 0
    var engineFrames = 0
    var updatedAt: Date?

    var hasTraffic: Bool {
        udpDatagrams > 0 || websocketFrames > 0 || engineFrames > 0
    }
}

struct RecentConnection: Codable, Equatable {
    let host: String
    let port: Int
    let lastConnectedAt: Date
}

enum ServiceMetricsParser {
    static func updating(_ current: ServiceMetrics, from text: String, now: Date = Date()) -> ServiceMetrics {
        var values: [String: Int] = [:]
        for line in text.split(whereSeparator: \.isNewline) {
            guard let marker = line.range(of: "[net] stats:") else { continue }
            for field in line[marker.upperBound...].split(separator: " ") {
                let pair = field.split(separator: "=", maxSplits: 1)
                guard pair.count == 2, let value = Int(pair[1]) else { continue }
                values[String(pair[0])] = value
            }
        }
        guard values.isEmpty == false else { return current }

        var result = current
        result.udpDatagrams = values["udp_rx"] ?? current.udpDatagrams
        result.websocketFrames = values["ws"] ?? current.websocketFrames
        result.decodeErrors = values["decode_err"] ?? current.decodeErrors
        result.engineFrames = values["engine_in"] ?? current.engineFrames
        result.updatedAt = now
        return result
    }
}
