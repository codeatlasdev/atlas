import SwiftUI

enum AtlasTypography {
    case title
    case heading
    case body
    case caption
    case mono

    var font: Font {
        switch self {
        case .title: .system(size: 20, weight: .semibold, design: .default)
        case .heading: .system(size: 14, weight: .semibold, design: .default)
        case .body: .system(size: 13, weight: .regular, design: .default)
        case .caption: .system(size: 11, weight: .regular, design: .default)
        case .mono: .system(size: 12, weight: .regular, design: .monospaced)
        }
    }
}

extension View {
    func atlasFont(_ style: AtlasTypography) -> some View {
        font(style.font)
    }
}
