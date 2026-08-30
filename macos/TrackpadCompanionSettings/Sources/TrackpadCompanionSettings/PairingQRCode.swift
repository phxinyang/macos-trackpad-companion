import AppKit
import CoreImage
import CoreImage.CIFilterBuiltins
import SwiftUI

/// QR presentation for the phone pairing payload. The payload remains a local
/// `mtc://` URI; no network request or cloud hand-off is involved.
struct PairingQRCodeView: View {
    let payload: String
    let language: AppLanguage

    @State private var image: NSImage?

    var body: some View {
        HStack(alignment: .center, spacing: 14) {
            Group {
                if let image {
                    Image(nsImage: image)
                        .interpolation(.none)
                        .resizable()
                        .scaledToFit()
                        .padding(10)
                        .background(.white, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                } else {
                    ProgressView()
                        .frame(width: 176, height: 176)
                }
            }
            .frame(width: 196, height: 196)
            .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 12, style: .continuous))

            VStack(alignment: .leading, spacing: 7) {
                Label(
                    language.text("Scan with the phone app", "用手机应用扫描"),
                    systemImage: "qrcode.viewfinder"
                )
                .font(.headline)
                Text(language.text(
                    "The code includes the local address, port, service capabilities, and pairing token.",
                    "二维码包含局域网地址、端口、服务能力和配对 Token。"
                ))
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                Text(language.text(
                    "Keep both devices on the same Wi-Fi. The token stays on your local network.",
                    "请让两台设备连接同一 Wi-Fi。Token 只在局域网内使用。"
                ))
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .task(id: payload) {
            image = QRCodeRenderer.image(from: payload, size: 512)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(language.text("Phone pairing QR code", "手机配对二维码"))
    }
}

private enum QRCodeRenderer {
    static func image(from value: String, size: Int) -> NSImage? {
        guard !value.isEmpty else { return nil }
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(value.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }

        let scale = max(1, CGFloat(size) / output.extent.width)
        let scaled = output.transformed(by: CGAffineTransform(scaleX: scale, y: scale))
        let context = CIContext()
        guard let cgImage = context.createCGImage(scaled, from: scaled.extent) else { return nil }
        return NSImage(cgImage: cgImage, size: NSSize(width: size, height: size))
    }
}
