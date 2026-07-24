import SwiftUI

/// Atlas Design Tokens — Single source of truth.
/// Blue monochromatic. Dark mode only. CleanMyMac-inspired.
enum DS {
    // MARK: - Backgrounds
    enum bg {
        static let base = Color(hex: "1C1C1E")
        static let elevated = Color(hex: "2C2C2E")
        static let elevated2 = Color(hex: "3A3A3C")
        static let hover = Color.white.opacity(0.05)
        static let selected = Color(hex: "0A84FF").opacity(0.12)
    }

    // MARK: - Text
    enum text {
        static let primary = Color.white
        static let secondary = Color.white.opacity(0.6)
        static let tertiary = Color.white.opacity(0.3)
        static let disabled = Color.white.opacity(0.15)
    }

    // MARK: - Accent (Blue only)
    enum accent {
        static let primary = Color(hex: "0A84FF")
        static let hover = Color(hex: "409CFF")
        static let subtle = Color(hex: "0A84FF").opacity(0.12)
    }

    // MARK: - Status
    enum status {
        static let success = Color(hex: "30D158")
        static let warning = Color(hex: "FFD60A")
        static let error = Color(hex: "FF453A")
        static let info = Color(hex: "0A84FF")
    }

    // MARK: - Border
    enum border {
        static let subtle = Color.white.opacity(0.08)
        static let medium = Color.white.opacity(0.12)
        static let focus = Color(hex: "0A84FF").opacity(0.5)
    }

    // MARK: - Spacing
    enum spacing {
        static let xs: CGFloat = 4
        static let sm: CGFloat = 8
        static let md: CGFloat = 12
        static let lg: CGFloat = 16
        static let xl: CGFloat = 20
        static let xxl: CGFloat = 24
        static let xxxl: CGFloat = 32
    }

    // MARK: - Radius
    enum radius {
        static let sm: CGFloat = 6
        static let md: CGFloat = 8
        static let lg: CGFloat = 12
        static let xl: CGFloat = 16
        static let xxl: CGFloat = 20
    }
}

// MARK: - Color hex init

extension Color {
    init(hex: String) {
        let hex = hex.trimmingCharacters(in: .init(charactersIn: "#"))
        var int: UInt64 = 0
        Scanner(string: hex).scanHexInt64(&int)
        let r = Double((int >> 16) & 0xFF) / 255.0
        let g = Double((int >> 8) & 0xFF) / 255.0
        let b = Double(int & 0xFF) / 255.0
        self.init(red: r, green: g, blue: b)
    }
}

// MARK: - Backward compat aliases (remove later)

enum AtlasColors {
    static let backgroundDeep = DS.bg.base
    static let backgroundSurface = DS.bg.elevated
    static let backgroundElevated = DS.bg.elevated2
    static let backgroundGlass = DS.bg.hover
    static let neonCyan = DS.accent.primary
    static let neonPurple = DS.accent.primary
    static let neonPink = DS.status.error
    static let neonGreen = DS.status.success
    static let neonAmber = DS.status.warning
    static let neonRed = DS.status.error
    static let textPrimary = DS.text.primary
    static let textSecondary = DS.text.secondary
    static let textTertiary = DS.text.tertiary
    static let border = DS.border.subtle
    static let borderSubtle = DS.border.subtle
    static let sidebarBackground = DS.bg.elevated
    static let sidebarHover = DS.bg.hover
    static let sidebarSelected = DS.accent.subtle
    static let accentPrimary = DS.accent.primary
    static let accentSecondary = DS.accent.primary
    static let statusSuccess = DS.status.success
    static let statusWarning = DS.status.warning
    static let statusError = DS.status.error
    static let statusInfo = DS.accent.primary

    static let gradientPrimary = LinearGradient(
        colors: [DS.accent.primary, DS.accent.primary],
        startPoint: .leading,
        endPoint: .trailing
    )

    static let gradientAccent = LinearGradient(
        colors: [DS.accent.primary, DS.accent.primary],
        startPoint: .leading,
        endPoint: .trailing
    )

    static let gradientFull = LinearGradient(
        colors: [DS.accent.primary, DS.accent.primary],
        startPoint: .leading,
        endPoint: .trailing
    )

    static let gradientRadialBackground = RadialGradient(
        colors: [DS.bg.base, DS.bg.base],
        center: .center,
        startRadius: 0,
        endRadius: 500
    )
}

// MARK: - View Extensions (backwards compat)

extension View {
    func atlasBackground(_ level: AtlasBackgroundLevel = .base) -> some View {
        background(level.color)
    }

    func atlasForeground(_ style: AtlasTextStyle = .primary) -> some View {
        foregroundStyle(style.color)
    }
}

enum AtlasBackgroundLevel {
    case base, surface, elevated

    var color: Color {
        switch self {
        case .base: DS.bg.base
        case .surface: DS.bg.elevated
        case .elevated: DS.bg.elevated2
        }
    }
}

enum AtlasTextStyle {
    case primary, secondary, tertiary

    var color: Color {
        switch self {
        case .primary: DS.text.primary
        case .secondary: DS.text.secondary
        case .tertiary: DS.text.tertiary
        }
    }
}
