import SwiftUI
import AppKit

// MARK: - Design System Colors

enum AtlasColors {

    // MARK: Backgrounds (3 levels)

    static let backgroundBase = Color(nsColor: .windowBackgroundColor)
    static let backgroundSurface = Color(nsColor: .controlBackgroundColor)
    static let backgroundElevated = Color(nsColor: .underPageBackgroundColor)

    // MARK: Text

    static let textPrimary = Color(nsColor: .labelColor)
    static let textSecondary = Color(nsColor: .secondaryLabelColor)
    static let textTertiary = Color(nsColor: .tertiaryLabelColor)

    // MARK: Accent

    static let accentPrimary = Color(red: 0.35, green: 0.34, blue: 0.84) // Indigo/violet
    static let accentSecondary = Color(red: 0.25, green: 0.52, blue: 0.96) // Blue

    // MARK: Status

    static let statusSuccess = Color(red: 0.20, green: 0.78, blue: 0.35)
    static let statusWarning = Color(red: 0.95, green: 0.65, blue: 0.12)
    static let statusError = Color(red: 0.92, green: 0.26, blue: 0.24)
    static let statusInfo = Color(red: 0.25, green: 0.52, blue: 0.96)

    // MARK: Borders & Separators

    static let border = Color(nsColor: .separatorColor)
    static let borderSubtle = Color(nsColor: .quaternaryLabelColor)

    // MARK: Sidebar

    static let sidebarBackground = Color(nsColor: .controlBackgroundColor).opacity(0.5)
    static let sidebarHover = Color(nsColor: .labelColor).opacity(0.06)
    static let sidebarSelected = Color(red: 0.35, green: 0.34, blue: 0.84).opacity(0.15)
}

// MARK: - View Extensions

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
        case .base: AtlasColors.backgroundBase
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
