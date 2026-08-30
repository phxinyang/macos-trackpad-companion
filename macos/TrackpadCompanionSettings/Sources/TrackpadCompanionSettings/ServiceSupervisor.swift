import Foundation
import SwiftUI
import AppKit
import ApplicationServices
import ServiceManagement
import Network
import Darwin

@MainActor
final class ServiceSupervisor: ObservableObject {
    @Published private(set) var state: ServiceState = .stopped
    @Published private(set) var message = ""
    @Published private(set) var endpoint = ""
    @Published private(set) var pairingURI = ""
    @Published private(set) var localAddress = ""
    @Published private(set) var webEnabled = true
    @Published private(set) var phoneEnabled = true
    @Published private(set) var boundPort: Int?
    @Published private(set) var accessibilityGranted = false
    @Published private(set) var tokenConfigured = false
    @Published private(set) var diagnostics = ""
    @Published private(set) var metrics = ServiceMetrics()
    @Published private(set) var launchAtLogin = false
    @Published private(set) var launchAtLoginRequiresApproval = false
    @Published private(set) var networkAvailable = true
    @Published private(set) var networkInterface = ""
    @Published private(set) var recentConnection: RecentConnection?
    private var process: Process?
    private var outputPipe: Pipe?
    private var lastServiceOutput = ""
    private let bonjour = BonjourAdvertiser()
    private let serviceID: String
    private var terminationObserver: NSObjectProtocol?
    private var wakeObserver: NSObjectProtocol?
    private var pathMonitor: NWPathMonitor?
    private var pathMonitorQueue: DispatchQueue?
    private var networkPathInitialized = false
    private var desiredRunning = false
    private var automaticRestartCount = 0
    private var processStartedAt: Date?
    private var restartAfterTermination = false
    private var restartWorkItem: DispatchWorkItem?

    init() {
        let defaults = UserDefaults.standard
        serviceID = defaults.string(forKey: "TrackpadCompanion.serviceID") ?? {
            let value = UUID().uuidString.lowercased()
            defaults.set(value, forKey: "TrackpadCompanion.serviceID")
            return value
        }()
        terminationObserver = NotificationCenter.default.addObserver(
            forName: NSApplication.willTerminateNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            // `willTerminate` is the last synchronous cleanup point. A Task
            // scheduled here can be abandoned while NSApplication exits,
            // leaving companion-net alive and holding the instance flock.
            MainActor.assumeIsolated {
                self?.stopForApplicationTermination()
            }
        }
        wakeObserver = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didWakeNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.handleWake()
            }
        }
        startPathMonitor()
        refreshLaunchAtLogin()
        loadRecentConnection()
        // MenuBarExtra can launch without ever creating the settings window.
        // Defer startup until the object is installed in SwiftUI so the menu
        // bar and the settings scene share one lifecycle.
        Task { @MainActor [weak self] in
            await Task.yield()
            self?.start()
        }
    }

    deinit {
        outputPipe?.fileHandleForReading.readabilityHandler = nil
        if let process, process.isRunning {
            process.terminate()
            Self.terminateAndWait(process)
        }
        bonjour.stop()
        restartWorkItem?.cancel()
        pathMonitor?.cancel()
        if let terminationObserver {
            NotificationCenter.default.removeObserver(terminationObserver)
        }
        if let wakeObserver {
            NSWorkspace.shared.notificationCenter.removeObserver(wakeObserver)
        }
    }

    func start() {
        desiredRunning = true
        restartWorkItem?.cancel()
        if process?.isRunning == true {
            // A previous helper may still be winding down after Restart.
            // Let its termination callback start the requested replacement.
            restartAfterTermination = true
            return
        }
        restartAfterTermination = false
        preparePairing()
        guard webEnabled || phoneEnabled else {
            bonjour.stop()
            endpoint = ""
            pairingURI = ""
            state = .stopped
            message = "No connections are enabled."
            return
        }
        refreshPermissions()
        guard accessibilityGranted else {
            state = .waitingForPermission
            message = "Accessibility permission is required."
            return
        }
        guard let executable = locate("COMPANION_NET_BIN", bundledName: "companion-net") else {
            state = .failed
            message = "companion-net was not found. Build the Rust daemon or set COMPANION_NET_BIN."
            return
        }
        let child = Process()
        child.executableURL = URL(fileURLWithPath: executable)
        // The host owns the TCC prompt. Keep the embedded helper silent so a
        // stale child answer cannot create a repeating system dialog.
        child.environment = ProcessInfo.processInfo.environment.merging([
            "MTC_ACCESSIBILITY_PROMPT": "0",
            "MTC_ACCESSIBILITY_HOST_TRUSTED": accessibilityGranted ? "1" : "0",
        ]) { _, new in new }
        let pipe = Pipe()
        child.standardOutput = pipe
        child.standardError = pipe
        outputPipe = pipe
        pipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty else {
                // A closed child pipe can repeatedly report readability.
                // Remove the handler at EOF or the host app spins forever.
                handle.readabilityHandler = nil
                return
            }
            let text = String(data: data, encoding: .utf8) ?? ""
            Task { @MainActor in
                guard let self else { return }
                let status = text.trimmingCharacters(in: .whitespacesAndNewlines)
                if !status.isEmpty {
                    self.lastServiceOutput = String(status.suffix(2000))
                    self.message = status
                }
                self.metrics = ServiceMetricsParser.updating(self.metrics, from: text)
                if let port = self.port(from: status) {
                    let isNewPort = self.boundPort != port
                    self.boundPort = port
                    self.updateEndpoint(port: port)
                    if isNewPort {
                        self.rememberConnection(port: port)
                        if self.phoneEnabled {
                            self.publishBonjour(port: port)
                        } else {
                            self.bonjour.stop()
                            self.pairingURI = ""
                        }
                    }
                }
                if text.localizedCaseInsensitiveContains("accessibility permission required") {
                    self.state = .waitingForPermission
                } else if text.contains("listening on") || text.contains("[net] ready web=") {
                    self.state = .running
                }
            }
        }
        child.terminationHandler = { [weak self] process in
            Task { @MainActor in
                guard let self else { return }
                self.process = nil
                self.outputPipe?.fileHandleForReading.readabilityHandler = nil
                self.outputPipe = nil
                if let startedAt = self.processStartedAt,
                   Date().timeIntervalSince(startedAt) >= 10 {
                    self.automaticRestartCount = 0
                }
                self.processStartedAt = nil
                self.boundPort = nil
                self.endpoint = ""
                self.pairingURI = ""
                self.bonjour.stop()
                if process.terminationStatus == 0 || self.state == .stopped || !self.desiredRunning {
                    self.state = .stopped
                    if self.restartAfterTermination && self.desiredRunning {
                        self.restartAfterTermination = false
                        self.scheduleStart(after: 0.05)
                    }
                } else {
                    let detail = self.lastServiceOutput.trimmingCharacters(in: .whitespacesAndNewlines)
                    if detail.localizedCaseInsensitiveContains("Accessibility permission") {
                        self.state = .waitingForPermission
                    } else {
                        self.state = self.automaticRestartCount == 0 ? .degraded : .failed
                    }
                    self.message = detail.isEmpty
                        ? "companion-net exited with status \(process.terminationStatus)."
                        : detail
                    if self.desiredRunning && self.state != .waitingForPermission && self.automaticRestartCount == 0 {
                        self.automaticRestartCount = 1
                        self.message = "Service stopped unexpectedly. Retrying once…"
                        self.scheduleStart(after: 0.6)
                    }
                }
            }
        }
        do {
            lastServiceOutput = ""
            metrics = ServiceMetrics()
            try child.run()
            process = child
            processStartedAt = Date()
            state = .starting
            message = "Starting companion-net…"
        } catch {
            state = .failed
            message = error.localizedDescription
        }
    }

    func stop() {
        desiredRunning = false
        automaticRestartCount = 0
        restartAfterTermination = false
        restartWorkItem?.cancel()
        stopProcess()
    }

    /// Stop synchronously during application termination so a relaunch cannot
    /// race the helper's instance lock. The normal user stop remains
    /// asynchronous; only the final app teardown waits for the child.
    func stopForApplicationTermination() {
        desiredRunning = false
        automaticRestartCount = 0
        restartAfterTermination = false
        restartWorkItem?.cancel()
        stopProcess(waitForExit: true)
    }

    private func stopProcess(waitForExit: Bool = false) {
        outputPipe?.fileHandleForReading.readabilityHandler = nil
        outputPipe = nil
        guard let process, process.isRunning else {
            self.process = nil
            bonjour.stop()
            boundPort = nil
            endpoint = ""
            pairingURI = ""
            state = .stopped
            return
        }
        state = .stopped
        message = "Service stopped"
        bonjour.stop()
        boundPort = nil
        endpoint = ""
        pairingURI = ""
        process.terminate()
        guard waitForExit else { return }
        Self.terminateAndWait(process)
    }

    nonisolated private static func terminateAndWait(_ process: Process) {
        // SIGTERM normally exits immediately. Keep this final wait bounded,
        // then force-kill a wedged helper so the lock is released before
        // NSApplication finishes terminating.
        let deadline = Date().addingTimeInterval(2)
        while process.isRunning && Date() < deadline {
            Thread.sleep(forTimeInterval: 0.02)
        }
        if process.isRunning {
            _ = kill(process.processIdentifier, SIGKILL)
            process.waitUntilExit()
        }
    }

    /// Retry the service from a user-facing recovery action. This keeps the
    /// permission gate and helper lifecycle in one place instead of making
    /// menu-bar and settings views duplicate startup rules.
    func retry() {
        refreshPermissions()
        if accessibilityGranted {
            start()
        } else {
            state = .waitingForPermission
            message = "Accessibility permission is required."
        }
    }

    func refreshLaunchAtLogin() {
        let status = SMAppService.mainApp.status
        launchAtLogin = status == .enabled
        launchAtLoginRequiresApproval = status == .requiresApproval
    }

    func setLaunchAtLogin(_ enabled: Bool) {
        do {
            if enabled {
                try SMAppService.mainApp.register()
            } else {
                try SMAppService.mainApp.unregister()
            }
            refreshLaunchAtLogin()
        } catch {
            refreshLaunchAtLogin()
            message = enabled
                ? "Could not enable launch at login: \(error.localizedDescription)"
                : "Could not disable launch at login: \(error.localizedDescription)"
        }
    }

    func restart() {
        restartAfterTermination = true
        desiredRunning = false
        automaticRestartCount = 0
        restartWorkItem?.cancel()
        stopProcess()
        desiredRunning = true
        if process == nil {
            restartAfterTermination = false
            scheduleStart(after: 0.18)
        }
    }

    func openAccessibilitySettings() {
        let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
        _ = AXIsProcessTrustedWithOptions(options)
        let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!
        NSWorkspace.shared.open(url)
    }

    func refreshPermissions() {
        accessibilityGranted = AXIsProcessTrusted()
    }

    func copyPairingURI() {
        guard !pairingURI.isEmpty else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(pairingURI, forType: .string)
    }

    func copyWebURL() {
        guard !endpoint.isEmpty else { return }
        var value = endpoint
        if let token = pairingToken, tokenConfigured,
           var components = URLComponents(string: endpoint) {
            components.queryItems = [URLQueryItem(name: "token", value: token)]
            value = components.string ?? endpoint
        }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(value, forType: .string)
    }

    func copyLocalAddress() {
        guard !localAddress.isEmpty else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(localAddress, forType: .string)
        message = "Local IP address copied."
    }

    /// Apply a changed net.* switch without starting a service that the user
    /// had stopped. A running helper is restarted so the socket boundary is
    /// updated immediately.
    func applyConnectionConfiguration() {
        let wasRunning = process?.isRunning == true
        if wasRunning || desiredRunning {
            restart()
        } else {
            refreshConnectionSettings()
        }
    }

    func refreshConnectionSettings() {
        refreshLocalAddress()
        preparePairing()
    }

    /// Update one of the two transport switches from a quick-control surface.
    /// The write still goes through companion-config, preserving one config
    /// authority for the menu bar and the full settings window.
    func setConnectionEnabled(path: String, enabled: Bool) {
        guard path == "net.web_enabled" || path == "net.phone_enabled" else { return }
        let previousWebEnabled = webEnabled
        let previousPhoneEnabled = phoneEnabled
        if path == "net.web_enabled" {
            webEnabled = enabled
        } else {
            phoneEnabled = enabled
        }
        guard let executable = locate("COMPANION_CONFIG_BIN", bundledName: "companion-config") else {
            webEnabled = previousWebEnabled
            phoneEnabled = previousPhoneEnabled
            message = "companion-config was not found."
            return
        }
        do {
            _ = try run(executable: executable, arguments: ["set", "--path", path, "--value", enabled ? "true" : "false"])
            NotificationCenter.default.post(name: .configurationDidChange, object: nil)
            applyConnectionConfiguration()
        } catch {
            webEnabled = previousWebEnabled
            phoneEnabled = previousPhoneEnabled
            message = error.localizedDescription
        }
    }

    private func handleWake() {
        refreshPermissions()
        refreshLaunchAtLogin()
        guard desiredRunning, state != .stopped else { return }
        if state == .waitingForPermission && accessibilityGranted {
            start()
        } else if state == .degraded && networkAvailable {
            restart()
        } else if state == .failed && process?.isRunning != true {
            retry()
        }
    }

    func handlePairingURL(_ url: URL) {
        guard url.scheme == "mtc", url.host == "pair" else { return }
        message = "Pairing link received. Use it in the Android client on the same LAN."
    }

    func refreshDiagnostics() {
        guard let executable = locate("COMPANION_CONFIG_BIN", bundledName: "companion-config") else {
            diagnostics = "companion-config was not found."
            return
        }
        do {
            diagnostics = String(data: try run(executable: executable, arguments: ["doctor"]), encoding: .utf8) ?? "No diagnostics returned."
        } catch {
            diagnostics = error.localizedDescription
        }
    }

    func copyDiagnostics() {
        if diagnostics.isEmpty {
            refreshDiagnostics()
        }
        guard !diagnostics.isEmpty else { return }
        NSPasteboard.general.clearContents()
        let sanitized = diagnostics.replacingOccurrences(of: NSHomeDirectory(), with: "~")
        NSPasteboard.general.setString(sanitized, forType: .string)
        message = "Diagnostics copied to the clipboard."
    }

    func openLogs() {
        let url = URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent("Library/Logs/macos-trackpad-companion", isDirectory: true)
        NSWorkspace.shared.open(url)
    }

    private func publishBonjour(port: Int) {
        guard phoneEnabled else {
            bonjour.stop()
            pairingURI = ""
            return
        }
        bonjour.publish(port: port, serviceID: serviceID, authenticated: tokenConfigured, webEnabled: webEnabled, phoneEnabled: phoneEnabled)
        var components = URLComponents()
        components.scheme = "mtc"
        components.host = "pair"
        components.queryItems = [
            URLQueryItem(name: "host", value: advertisedHost),
            URLQueryItem(name: "port", value: String(port)),
            URLQueryItem(name: "web", value: webEnabled ? "1" : "0"),
            URLQueryItem(name: "phone", value: phoneEnabled ? "1" : "0"),
        ]
        if let token = pairingToken {
            components.queryItems?.append(URLQueryItem(name: "token", value: token))
        }
        pairingURI = components.string ?? ""
    }

    private var pairingToken: String?

    private var localHostName: String {
        let candidate = Host.current().name ?? ProcessInfo.processInfo.hostName
        return candidate.isEmpty ? "localhost" : candidate
    }

    private var advertisedHost: String {
        localAddress.isEmpty ? localHostName : localAddress
    }

    private func refreshLocalAddress() {
        var addresses: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&addresses) == 0, let first = addresses else {
            localAddress = ""
            return
        }
        defer { freeifaddrs(addresses) }

        var preferred = ""
        var fallback = ""
        var cursor: UnsafeMutablePointer<ifaddrs>? = first
        while let current = cursor {
            let entry = current.pointee
            cursor = entry.ifa_next
            guard let rawAddress = entry.ifa_addr,
                  rawAddress.pointee.sa_family == sa_family_t(AF_INET) else { continue }
            let name = String(cString: entry.ifa_name)
            let address = rawAddress.withMemoryRebound(to: sockaddr_in.self, capacity: 1) { pointer in
                var value = pointer.pointee.sin_addr
                var buffer = [CChar](repeating: 0, count: Int(INET_ADDRSTRLEN))
                guard inet_ntop(AF_INET, &value, &buffer, socklen_t(INET_ADDRSTRLEN)) != nil else { return "" }
                return String(cString: buffer)
            }
            guard !address.isEmpty, address != "127.0.0.1" else { continue }
            if name == "en0" || name == "en1" {
                preferred = address
                break
            }
            if fallback.isEmpty { fallback = address }
        }
        localAddress = preferred.isEmpty ? fallback : preferred
    }

    private func updateEndpoint(port: Int) {
        guard webEnabled else {
            endpoint = ""
            return
        }
        endpoint = "http://\(advertisedHost):\(port)/"
    }

    private func preparePairing() {
        refreshLocalAddress()
        guard let executable = locate("COMPANION_CONFIG_BIN", bundledName: "companion-config") else {
            webEnabled = true
            phoneEnabled = true
            tokenConfigured = false
            pairingToken = nil
            return
        }
        do {
            let data = try run(executable: executable, arguments: ["dump"])
            guard let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let config = root["config"] as? [String: Any],
                  let net = config["net"] as? [String: Any] else {
                webEnabled = true
                phoneEnabled = true
                tokenConfigured = false
                pairingToken = nil
                return
            }
            webEnabled = net["web_enabled"] as? Bool ?? true
            phoneEnabled = net["phone_enabled"] as? Bool ?? true
            guard webEnabled || phoneEnabled else {
                tokenConfigured = false
                pairingToken = nil
                return
            }
            _ = try run(executable: executable, arguments: ["ensure-token"])
            let refreshed = try run(executable: executable, arguments: ["dump"])
            let refreshedRoot = try JSONSerialization.jsonObject(with: refreshed) as? [String: Any]
            let refreshedConfig = refreshedRoot?["config"] as? [String: Any]
            let refreshedNet = refreshedConfig?["net"] as? [String: Any]
            guard let token = refreshedNet?["token"] as? String, !token.isEmpty else {
                tokenConfigured = false
                pairingToken = nil
                return
            }
            pairingToken = token
            tokenConfigured = true
        } catch {
            message = "Pairing setup unavailable: \(error.localizedDescription)"
            tokenConfigured = false
            pairingToken = nil
        }
    }

    private func port(from status: String) -> Int? {
        if let marker = status.range(of: "port=") {
            let digits = status[marker.upperBound...].prefix { $0.isNumber }
            if let value = Int(digits), value > 0 { return value }
        }
        let marker = status.contains("phone input at ") ? "phone input at " : "touchpad page at "
        guard let start = status.range(of: marker)?.upperBound else { return nil }
        let suffix = status[start...]
        guard let colon = suffix.lastIndex(of: ":") else { return nil }
        let digits = suffix[suffix.index(after: colon)...].prefix { $0.isNumber }
        return Int(digits)
    }

    private func run(executable: String, arguments: [String]) throws -> Data {
        let process = Process()
        let pipe = Pipe()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        process.standardOutput = pipe
        process.standardError = pipe
        try process.run()
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw NSError(domain: "TrackpadCompanionSettings", code: Int(process.terminationStatus), userInfo: [NSLocalizedDescriptionKey: String(data: data, encoding: .utf8) ?? "helper failed"])
        }
        return data
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

    private func scheduleStart(after delay: TimeInterval) {
        restartWorkItem?.cancel()
        let work = DispatchWorkItem { [weak self] in
            Task { @MainActor in
                guard let self, self.desiredRunning else { return }
                self.start()
            }
        }
        restartWorkItem = work
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work)
    }

    private func startPathMonitor() {
        let monitor = NWPathMonitor()
        let queue = DispatchQueue(label: "com.mtc.trackpad-companion.network", qos: .utility)
        monitor.pathUpdateHandler = { [weak self] path in
            let available = path.status == .satisfied
            let interface: String
            if let type = path.availableInterfaces.first?.type {
                switch type {
                case .wifi: interface = "Wi-Fi"
                case .wiredEthernet: interface = "Ethernet"
                case .cellular: interface = "Cellular"
                case .loopback: interface = "Loopback"
                case .other: interface = "Other"
                @unknown default: interface = ""
                }
            } else {
                interface = ""
            }
            Task { @MainActor in
                self?.handleNetworkPath(available: available, interface: interface)
            }
        }
        pathMonitor = monitor
        pathMonitorQueue = queue
        monitor.start(queue: queue)
    }

    private func handleNetworkPath(available: Bool, interface: String) {
        let oldAvailability = networkAvailable
        let oldInterface = networkInterface
        networkAvailable = available
        networkInterface = interface
        guard networkPathInitialized else {
            networkPathInitialized = true
            return
        }
        guard desiredRunning else { return }
        if !available {
            if state == .running || state == .starting {
                state = .degraded
                message = "Network unavailable. Waiting to reconnect…"
            }
        } else if !oldAvailability || oldInterface != interface {
            state = .degraded
            message = "Network changed. Reconnecting…"
            restart()
        }
    }

    private func loadRecentConnection() {
        guard let data = UserDefaults.standard.data(forKey: "TrackpadCompanion.recentConnection") else { return }
        recentConnection = try? JSONDecoder().decode(RecentConnection.self, from: data)
    }

    private func rememberConnection(port: Int) {
        let connection = RecentConnection(host: advertisedHost, port: port, lastConnectedAt: Date())
        recentConnection = connection
        if let data = try? JSONEncoder().encode(connection) {
            UserDefaults.standard.set(data, forKey: "TrackpadCompanion.recentConnection")
        }
    }
}
