import SwiftUI

extension Font {
    static let atlasDisplay = Font.system(size: 28, weight: .bold, design: .default)
    static let atlasTitle = Font.system(size: 20, weight: .semibold, design: .default)
    static let atlasHeadline = Font.system(size: 15, weight: .semibold, design: .default)
    static let atlasBody = Font.system(size: 13, weight: .regular, design: .default)
    static let atlasCaption = Font.system(size: 11, weight: .regular, design: .default)
    static let atlasMono = Font.system(size: 12, weight: .regular, design: .monospaced)
}

enum AtlasTypography {
    case display, title, headline, body, caption, mono

    var font: Font {
        switch self {
        case .display: .atlasDisplay
        case .title: .atlasTitle
        case .headline: .atlasHeadline
        case .body: .atlasBody
        case .caption: .atlasCaption
        case .mono: .atlasMono
        }
    }

    var lineSpacing: CGFloat {
        switch self {
        case .display: 6
        case .title: 4
        case .headline: 3
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
