import SwiftUI

struct SidebarHeader: View {
    let language: AppLanguage
    let state: ServiceState

    var body: some View {
        HStack(spacing: 10) {
            ZStack {
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(LinearGradient(
                        colors: [Color.accentColor, Color.accentColor.opacity(0.72)],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    ))
                Image(systemName: "rectangle.inset.filled")
                    .font(.system(size: 20, weight: .semibold))
                    .foregroundStyle(.white)
            }
            .frame(width: 38, height: 38)
            VStack(alignment: .leading, spacing: 2) {
                Text("Trackpad Companion")
                    .font(.headline)
                    .lineLimit(1)
                HStack(spacing: 4) {
                    Circle()
                        .fill(state == .running ? .green : state == .failed ? .red : .secondary)
                        .frame(width: 6, height: 6)
                    Text(stateLabel)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .padding(.top, 18)
        .padding(.bottom, 14)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Trackpad Companion")
    }

    private var stateLabel: String {
        switch state {
        case .running: return language.text("Ready", "就绪")
        case .starting: return language.text("Starting", "启动中")
        case .waitingForPermission: return language.text("Permission needed", "需要授权")
        case .degraded: return language.text("Degraded", "受限")
        case .failed: return language.text("Needs attention", "需处理")
        case .stopped: return language.text("Stopped", "已停止")
        }
    }
}

struct StatusBadge: View {
    let state: ServiceState
    let language: AppLanguage

    var body: some View {
        Label(label, systemImage: state.symbol)
            .font(.caption.weight(.medium))
            .foregroundStyle(color)
            .padding(.horizontal, 9)
            .padding(.vertical, 5)
            .background(color.opacity(0.12), in: Capsule())
            .accessibilityLabel(label)
    }

    private var label: String {
        switch state {
        case .running: return language.text("Ready", "就绪")
        case .starting: return language.text("Starting", "启动中")
        case .waitingForPermission: return language.text("Permission", "待授权")
        case .degraded: return language.text("Degraded", "受限")
        case .failed: return language.text("Attention", "需处理")
        case .stopped: return language.text("Stopped", "已停止")
        }
    }

    private var color: Color {
        switch state {
        case .running: return .green
        case .failed: return .red
        case .waitingForPermission: return .orange
        default: return .secondary
        }
    }
}

struct OverviewHero: View {
    let state: ServiceState
    let language: AppLanguage

    var body: some View {
        HStack(spacing: 14) {
            Image(systemName: "rectangle.inset.filled")
                .font(.system(size: 30, weight: .medium))
                .foregroundStyle(.tint)
                .frame(width: 48, height: 48)
                .background(Color.accentColor.opacity(0.12), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            VStack(alignment: .leading, spacing: 4) {
                Text(language.text("Your Mac, touch-ready", "让 Mac 拥有触控板体验"))
                    .font(.title3.weight(.semibold))
                Text(language.text("A native-feeling bridge for pointer, scroll, zoom, and desktop gestures.", "为指针、滚动、缩放和桌面手势提供接近原生的桥接。"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            StatusBadge(state: state, language: language)
        }
        .padding(.vertical, 4)
    }
}

struct MetricTile: View {
    let title: String
    let value: String
    let symbol: String
    let tint: Color

    var body: some View {
        HStack(spacing: 9) {
            Image(systemName: symbol)
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(tint)
                .frame(width: 30, height: 30)
                .background(tint.opacity(0.12), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(value)
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 2)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
