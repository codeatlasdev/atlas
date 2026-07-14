import SwiftUI

// MARK: - Surface

struct Surface<Content: View>: View {
    let content: Content
    var elevation: Elevation

    enum Elevation { case base, elevated, glass }

    init(elevation: Elevation = .elevated, @ViewBuilder content: () -> Content) {
        self.elevation = elevation
        self.content = content()
    }

    var body: some View {
        content
            .background(background)
            .clipShape(RoundedRectangle(cornerRadius: DS.radius.lg, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: DS.radius.lg, style: .continuous)
                    .strokeBorder(DS.border.subtle, lineWidth: 0.5)
            )
    }

    @ViewBuilder
    private var background: some View {
        switch elevation {
        case .base: DS.bg.base
        case .elevated: DS.bg.elevated
        case .glass: Rectangle().fill(.ultraThinMaterial)
        }
    }
}

// MARK: - AtlasButton

struct AtlasButton: View {
    let title: String
    let icon: String?
    let style: Style
    let action: () -> Void

    enum Style { case primary, secondary, ghost }

    init(_ title: String, icon: String? = nil, style: Style = .primary, action: @escaping () -> Void) {
        self.title = title
        self.icon = icon
        self.style = style
        self.action = action
    }

    var body: some View {
        Button(action: action) {
            HStack(spacing: DS.spacing.sm) {
                if let icon {
                    Image(systemName: icon)
                        .font(.system(size: 12, weight: .medium))
                }
                Text(title)
                    .font(.atlasBody)
                    .fontWeight(.medium)
            }
            .padding(.horizontal, DS.spacing.lg)
            .padding(.vertical, DS.spacing.sm)
            .background(buttonBackground)
            .foregroundStyle(buttonForeground)
            .clipShape(RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous)
                    .strokeBorder(buttonBorder, lineWidth: 0.5)
            )
        }
        .buttonStyle(.plain)
    }

    private var buttonBackground: Color {
        switch style {
        case .primary: DS.accent.primary
        case .secondary: DS.bg.elevated2
        case .ghost: .clear
        }
    }

    private var buttonForeground: Color {
        switch style {
        case .primary: .white
        case .secondary: DS.text.primary
        case .ghost: DS.text.secondary
        }
    }

    private var buttonBorder: Color {
        switch style {
        case .primary: .clear
        case .secondary: DS.border.medium
        case .ghost: .clear
        }
    }
}

// MARK: - Badge

struct Badge: View {
    let text: String
    var color: Color = DS.accent.primary
    var size: Size = .medium

    enum Size { case small, medium }

    var body: some View {
        Text(text)
            .font(size == .small ? .atlasCaption : .atlasBody)
            .fontWeight(.medium)
            .foregroundStyle(color)
            .padding(.horizontal, size == .small ? 6 : 8)
            .padding(.vertical, size == .small ? 2 : 3)
            .background(color.opacity(0.12))
            .clipShape(Capsule())
    }
}

// MARK: - InputField

struct InputField: View {
    let placeholder: String
    @Binding var text: String
    var onSubmit: (() -> Void)?

    var body: some View {
        TextField(placeholder, text: $text)
            .textFieldStyle(.plain)
            .font(.atlasBody)
            .foregroundStyle(DS.text.primary)
            .padding(.horizontal, DS.spacing.md)
            .padding(.vertical, DS.spacing.sm)
            .background(DS.bg.base)
            .clipShape(RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous)
                    .strokeBorder(DS.border.medium, lineWidth: 0.5)
            )
            .onSubmit { onSubmit?() }
    }
}

// MARK: - Divider

struct SoftDivider: View {
    var body: some View {
        Rectangle()
            .fill(DS.border.subtle)
            .frame(height: 0.5)
    }
}

// MARK: - View extensions

extension View {
    func surfaceStyle() -> some View {
        self
            .background(DS.bg.elevated)
            .clipShape(RoundedRectangle(cornerRadius: DS.radius.lg, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: DS.radius.lg, style: .continuous)
                    .strokeBorder(DS.border.subtle, lineWidth: 0.5)
            )
    }
}

// MARK: - Legacy compat

struct StatusBadge: View {
    let label: String
    let color: Color
    var showDot: Bool = true

    init(label: String, color: Color, showDot: Bool = true) {
        self.label = label
        self.color = color
        self.showDot = showDot
    }

    init(text: String, color: Color) {
        self.label = text
        self.color = color
        self.showDot = false
    }

    var body: some View {
        HStack(spacing: 5) {
            if showDot {
                Circle()
                    .fill(color)
                    .frame(width: 6, height: 6)
            }
            Text(label)
                .font(.atlasCaption)
                .fontWeight(.medium)
                .foregroundStyle(color)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
        .background(color.opacity(0.12))
        .clipShape(Capsule())
    }
}

struct CountBadge: View {
    let count: Int
    var color: Color = DS.text.secondary

    var body: some View {
        if count > 0 {
            Text("\(count)")
                .font(.system(size: 10, weight: .medium, design: .rounded))
                .foregroundStyle(color)
                .padding(.horizontal, 5)
                .padding(.vertical, 1)
                .background(color.opacity(0.12))
                .clipShape(Capsule())
        }
    }
}

// MARK: - Legacy button styles (backward compat for views not yet rewritten)

struct NeonButtonStyle: ButtonStyle {
    var color: Color = DS.accent.primary

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: .medium))
            .foregroundStyle(.white)
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .background(
                RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous)
                    .fill(color)
            )
            .opacity(configuration.isPressed ? 0.8 : 1.0)
    }
}

struct GradientButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: .medium))
            .foregroundStyle(.white)
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .background(
                RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous)
                    .fill(DS.accent.primary)
            )
            .opacity(configuration.isPressed ? 0.8 : 1.0)
    }
}

struct SidebarItemStyle: ButtonStyle {
    var isSelected: Bool = false
    var isHovered: Bool = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .background(
                RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous)
                    .fill(
                        isSelected
                            ? DS.accent.subtle
                            : (isHovered ? DS.bg.hover : .clear)
                    )
            )
    }
}

// MARK: - Legacy card modifier

struct GlassCard: ViewModifier {
    var cornerRadius: CGFloat = 12

    func body(content: Content) -> some View {
        content
            .background(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .fill(.ultraThinMaterial)
            )
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .strokeBorder(DS.border.subtle, lineWidth: 0.5)
            )
            .clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
    }
}

struct CardStyle: ViewModifier {
    var padding: CGFloat = 16

    func body(content: Content) -> some View {
        content
            .padding(padding)
            .background(DS.bg.elevated)
            .clipShape(RoundedRectangle(cornerRadius: DS.radius.lg, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: DS.radius.lg, style: .continuous)
                    .strokeBorder(DS.border.subtle, lineWidth: 0.5)
            )
    }
}

extension View {
    func glassCard(cornerRadius: CGFloat = 12) -> some View {
        modifier(GlassCard(cornerRadius: cornerRadius))
    }

    func neonGlow(_ color: Color = DS.accent.primary, radius: CGFloat = 10) -> some View {
        self // No-op: glow removed
    }

    func cardStyle(padding: CGFloat = 16) -> some View {
        modifier(CardStyle(padding: padding))
    }

    func glassStyle(cornerRadius: CGFloat = 12) -> some View {
        modifier(GlassCard(cornerRadius: cornerRadius))
    }
}
