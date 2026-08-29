import Foundation

final class BonjourAdvertiser: NSObject, NetServiceDelegate {
    private var service: NetService?

    func publish(port: Int, serviceID: String, authenticated: Bool, webEnabled: Bool, phoneEnabled: Bool) {
        stop()
        let name = "Trackpad Companion - \(Host.current().localizedName ?? ProcessInfo.processInfo.hostName)"
        let service = NetService(domain: "local.", type: "_mtc-trackpad._tcp.", name: name, port: Int32(port))
        service.delegate = self
        service.setTXTRecord(NetService.data(fromTXTRecord: [
            "v": Data("1".utf8),
            "proto": Data("atp1".utf8),
            "auth": Data((authenticated ? "token" : "none").utf8),
            "id": Data(serviceID.utf8),
            "web": Data((webEnabled ? "1" : "0").utf8),
            "phone": Data((phoneEnabled ? "1" : "0").utf8),
        ]))
        service.publish(options: [.listenForConnections])
        self.service = service
    }

    func stop() {
        service?.stop()
        service = nil
    }

    func netService(_ sender: NetService, didNotPublish errorDict: [String : NSNumber]) {
        NSLog("Bonjour publish failed: %@", errorDict)
    }
}
