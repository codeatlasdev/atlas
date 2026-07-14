import SwiftUI

enum AtlasTypography {
    case display
    case title
    case headline
    case body
    case caption
    case mono

    var font: Font {
        switch self {
        case .display: .system(size: 32, weight: .bold, design: .default)
        case .title: .system(size: 20, weight: .semibold, design: .default)
        case .headline: .system(size: 15, weight: .semibold, design: .default)
        case .body: .system(size: 13, weight: .regular, design: .default)
        case .caption: .system(size: 11, weight: .regular, design: .default)
        case .mono: .system(size: 12, weight: .regular, design: .monospaced)
        }
    }

    var lineSpacing: CGFloat {
        switch self {
        case .display: 4
        case .title: 3
        case .headline: 2
        case .body: 2
        case .caption: 1
        case .mono: 2
        }
    }
}

extension View {
    func atlasFont(_ style: AtlasTypography) -> some View {
        self
            .font(style.font)
            .lineSpacing(style.lineSpacing)
    }
}
