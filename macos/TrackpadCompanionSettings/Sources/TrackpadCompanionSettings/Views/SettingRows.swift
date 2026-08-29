import SwiftUI

struct ToggleRow: View {
    let path: String
    let title: String
    let titleCN: String
    let description: String
    let descriptionCN: String
    @ObservedObject var model: SettingsModel

    var body: some View {
        Toggle(isOn: Binding(
            get: { model.toggle(path) },
            set: { model.set(path, value: isBooleanPath ? ($0 ? "true" : "false") : ($0 ? "on" : "off")) }
        )) {
            VStack(alignment: .leading, spacing: 3) {
                Text(model.language.text(title, titleCN))
                Text(model.language.text(description, descriptionCN))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var isBooleanPath: Bool { path.hasPrefix("scroll.") || path == "macos.sync_system_settings" }
}

struct UnavailableRow: View {
    let title: String
    let titleCN: String
    let description: String
    let descriptionCN: String
    let language: AppLanguage

    var body: some View {
        LabeledContent {
            Text(language.text("Unavailable", "不可用"))
                .foregroundStyle(.secondary)
        } label: {
            VStack(alignment: .leading, spacing: 3) {
                Text(language.text(title, titleCN))
                Text(language.text(description, descriptionCN))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

struct SliderRow: View {
    let title: String
    let titleCN: String
    @Binding var value: Double
    let range: ClosedRange<Double>
    let language: AppLanguage
    var unit = ""
    @State private var draft: Double = 0

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(language.text(title, titleCN))
                Spacer()
                Text(String(format: "%.1f", draft) + (unit.isEmpty ? "" : " \(unit)"))
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            Slider(value: $draft, in: range, onEditingChanged: { editing in
                if !editing { value = draft }
            })
            .accessibilityValue(Text(String(format: "%.1f", draft) + (unit.isEmpty ? "" : " \(unit)")))
        }
        .onAppear { draft = value }
        .onChange(of: value) { newValue in draft = newValue }
    }
}

struct PickerRow: View {
    let path: String
    let title: String
    let titleCN: String
    let options: [(String, String, String)]
    @ObservedObject var model: SettingsModel

    var body: some View {
        Picker(model.language.text(title, titleCN), selection: Binding(
            get: { model.string(path, default: "0") },
            set: { model.set(path, value: $0) }
        )) {
            ForEach(Array(options.enumerated()), id: \.offset) { item in
                let option = item.element
                Text(model.language.text(option.1, option.2)).tag(option.0)
            }
        }
    }
}
