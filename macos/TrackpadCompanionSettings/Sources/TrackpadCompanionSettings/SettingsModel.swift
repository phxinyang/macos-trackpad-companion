import Foundation
import SwiftUI

@MainActor
final class SettingsModel: ObservableObject {
    @Published var language: AppLanguage = .preferred {
        didSet { UserDefaults.standard.set(language.rawValue, forKey: "TrackpadCompanion.language") }
    }
    @Published var selectedSection: SettingsSection = .overview {
        didSet { UserDefaults.standard.set(selectedSection.rawValue, forKey: "TrackpadCompanion.selectedSection") }
    }
    @Published var selectedPath: String?
    @Published var error: String?
    @Published var isSaving = false
    @Published private(set) var values: [String: Any] = [:]
    var configPath: String = ""
    private var languageObserver: NSObjectProtocol?
    private var configurationObserver: NSObjectProtocol?

    init() {
        if let savedSection = UserDefaults.standard.string(forKey: "TrackpadCompanion.selectedSection"),
           let section = SettingsSection(rawValue: savedSection) {
            selectedSection = section
        }
        languageObserver = NotificationCenter.default.addObserver(forName: .toggleLanguage, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                self.language = self.language == .english ? .chinese : .english
            }
        }
        configurationObserver = NotificationCenter.default.addObserver(forName: .configurationDidChange, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in
                self?.reload()
            }
        }
    }

    deinit {
        if let languageObserver { NotificationCenter.default.removeObserver(languageObserver) }
        if let configurationObserver { NotificationCenter.default.removeObserver(configurationObserver) }
    }

    func reload() {
        do {
            let json = try runHelper(["dump"])
            guard let root = try JSONSerialization.jsonObject(with: json) as? [String: Any],
                  let config = root["config"] as? [String: Any] else { throw HelperError.invalidOutput }
            values = flatten(config)
            let rawPath = root["path"] as? String ?? ""
            let home = NSHomeDirectory()
            if rawPath.hasPrefix(home) {
                configPath = "~" + rawPath.dropFirst(home.count)
            } else {
                configPath = rawPath
            }
            error = nil
        } catch { self.error = error.localizedDescription }
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

    @discardableResult
    func set(_ path: String, value: String) -> Bool {
        isSaving = true
        do {
            _ = try runHelper(["set", "--path", path, "--value", value])
            values[path] = scalar(value)
            NotificationCenter.default.post(name: .configurationDidChange, object: nil)
            error = nil
            isSaving = false
            return true
        } catch { self.error = error.localizedDescription }
        isSaving = false
        return false
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
