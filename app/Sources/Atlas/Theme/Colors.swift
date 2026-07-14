import SwiftUI

enum AtlasColors {

    // MARK: - Backgrounds

    static let backgroundDeep = Color(hex: "0A0A0F")
    static let backgroundSurface = Color(hex: "12121A")
    static let backgroundElevated = Color(hex: "1A1A2E")
    static let backgroundGlass = Color.white.opacity(0.05)

    // MARK: - Neon Accents

    static let neonCyan = Color(hex: "00D4FF")
    static let neonPurple = Color(hex: "A855F7")
    static let neonPink = Color(hex: "EC4899")
    static let neonGreen = Color(hex: "10B981")
    static let neonAmber = Color(hex: "F59E0B")
    static let neonRed = Color(hex: "EF4444")

    // MARK: - Text

    static let textPrimary = Color.white
    static let textSecondary = Color.white.opacity(0.7)
    static let textTertiary = Color.white.opacity(0.4)

    // MARK: - Status (backwards compat)

    static let statusSuccess = neonGreen
    static let statusWarning = neonAmber
    static let statusError = neonRed
    static let statusInfo = neonCyan

    // MARK: - Accent (backwards compat)

    static let accentPrimary = neonPurple
    static let accentSecondary = neonCyan

    // MARK: - Borders

    static let border = Color.white.opacity(0.1)
    static let borderSubtle = Color.white.opacity(0.05)

    // MARK: - Sidebar

    static let sidebarBackground = backgroundSurface
    static let sidebarHover = Color.white.opacity(0.06)
    static let sidebarSelected = neonPurple.opacity(0.15)

    // MARK: - Gradients

    static let gradientPrimary = LinearGradient(
        colors: [neonCyan, neonPurple],
        startPoint: .leading,
        endPoint: .trailing
    )

    static let gradientAccent = LinearGradient(
        colors: [neonPurple, neonPink],
        startPoint: .leading,
        endPoint: .trailing
    )

    static let gradientFull = LinearGradient(
        colors: [neonCyan, neonPurple, neonPink],
        startPoint: .leading,
        endPoint: .trailing
    )

    static let gradientRadialBackground = RadialGradient(
        colors: [neonPurple.opacity(0.15), backgroundDeep],
        center: .center,
        startRadius: 0,
        endRadius: 500
    )
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
        case .base: AtlasColors.backgroundDeep
        case .surface: AtlasColors.backgroundSurface
        case .elevated: AtlasColors.backgroundElevated
        }
    }
}

enum AtlasTextStyle {
    case primary, secondary, tertiary

    var color: Color {
        switch self {
        case .primary: AtlasColors.textPrimary
        case .secondary: AtlasColors.textSecondary
        case .tertiary: AtlasColors.textTertiary
        }
    }
}
