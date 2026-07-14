import SwiftUI

enum AtlasColor {
    case background
    case surface
    case surfaceElevated
    case text
    case textSecondary
    case accent
    case border
    case statusOnline
    case statusOffline
    case statusError

    var color: Color {
        switch self {
        case .background: Color(nsColor: .windowBackgroundColor)
        case .surface: Color(nsColor: .controlBackgroundColor)
        case .surfaceElevated: Color(nsColor: .underPageBackgroundColor)
        case .text: Color(nsColor: .labelColor)
        case .textSecondary: Color(nsColor: .secondaryLabelColor)
        case .accent: .accentColor
        case .border: Color(nsColor: .separatorColor)
        case .statusOnline: .green
        case .statusOffline: Color(nsColor: .tertiaryLabelColor)
        case .statusError: .red
        }
    }
}

extension View {
    func foregroundStyle(_ token: AtlasColor) -> some View {
        foregroundStyle(token.color)
    }
}

extension ShapeStyle where Self == Color {
    static func atlas(_ token: AtlasColor) -> Color {
        token.color
    }
}
