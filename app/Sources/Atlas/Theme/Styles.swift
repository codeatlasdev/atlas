import SwiftUI

// MARK: - Card Style

struct CardStyle: ViewModifier {
    var padding: CGFloat = 16

    func body(content: Content) -> some View {
        content
            .padding(padding)
            .background {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(.ultraThinMaterial)
                    .shadow(color: .black.opacity(0.08), radius: 8, x: 0, y: 2)
            }
            .overlay {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(AtlasColors.border.opacity(0.3), lineWidth: 0.5)
            }
    }
}

// MARK: - Glass Style

struct GlassStyle: ViewModifier {
    var cornerRadius: CGFloat = 12

    func body(content: Content) -> some View {
        content
            .background {
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .fill(.ultraThinMaterial)
            }
            .overlay {
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .strokeBorder(AtlasColors.border.opacity(0.2), lineWidth: 0.5)
            }
    }
}

// MARK: - Status Badge

struct StatusBadge: View {
    let label: String
    let color: Color
    var showDot: Bool = true

    var body: some View {
        HStack(spacing: 5) {
            if showDot {
                Circle()
                    .fill(color)
                    .frame(width: 6, height: 6)
            }
            Text(label)
                .atlasFont(.caption)
                .foregroundStyle(color)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
        .background {
            Capsule(style: .continuous)
                .fill(color.opacity(0.12))
        }
    }
}

// MARK: - Count Badge

struct CountBadge: View {
    let count: Int
    var color: Color = AtlasColors.accentPrimary

    var body: some View {
        if count > 0 {
            Text("\(count)")
                .font(.system(size: 10, weight: .semibold, design: .rounded))
                .foregroundStyle(.white)
                .padding(.horizontal, 5)
                .padding(.vertical, 1)
                .background {
                    Capsule(style: .continuous)
                        .fill(color)
                }
        }
    }
}

// MARK: - View Extensions

extension View {
    func cardStyle(padding: CGFloat = 16) -> some View {
        modifier(CardStyle(padding: padding))
    }

    func glassStyle(cornerRadius: CGFloat = 12) -> some View {
        modifier(GlassStyle(cornerRadius: cornerRadius))
    }
}
