import SwiftUI

/// Renders markdown text with proper formatting.
/// Uses native AttributedString for inline formatting (bold, italic, code, links)
/// and custom views for code blocks.
struct MarkdownText: View {
    let text: String

    var body: some View {
        VStack(alignment: .leading, spacing: DS.spacing.sm) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                switch block {
                case .text(let content):
                    inlineMarkdown(content)
                case .code(let language, let content):
                    codeBlock(language: language, code: content)
                }
            }
        }
    }

    // MARK: - Inline Markdown

    private func inlineMarkdown(_ content: String) -> some View {
        Group {
            if let attributed = try? AttributedString(
                markdown: content,
                options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)
            ) {
                Text(attributed)
                    .font(.atlasBody)
                    .foregroundStyle(DS.text.primary)
                    .textSelection(.enabled)
                    .tint(DS.accent.primary)
            } else {
                Text(content)
                    .font(.atlasBody)
                    .foregroundStyle(DS.text.primary)
                    .textSelection(.enabled)
            }
        }
    }

    // MARK: - Code Block

    private func codeBlock(language: String?, code: String) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            if let lang = language, !lang.isEmpty {
                HStack {
                    Text(lang)
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(DS.text.tertiary)
                    Spacer()
                    Button {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(code, forType: .string)
                    } label: {
                        Image(systemName: "doc.on.doc")
                            .font(.system(size: 10))
                            .foregroundStyle(DS.text.tertiary)
                    }
                    .buttonStyle(.plain)
                }
                .padding(.horizontal, DS.spacing.md)
                .padding(.vertical, DS.spacing.xs)
                .background(DS.bg.base.opacity(0.5))
            }

            ScrollView(.horizontal, showsIndicators: false) {
                Text(code)
                    .font(.system(size: 12, design: .monospaced))
                    .foregroundStyle(DS.text.primary)
                    .textSelection(.enabled)
                    .padding(DS.spacing.md)
            }
            .frame(maxHeight: 400)
        }
        .background(DS.bg.base)
        .clipShape(RoundedRectangle(cornerRadius: DS.radius.md))
        .overlay(
            RoundedRectangle(cornerRadius: DS.radius.md)
                .stroke(DS.border.subtle, lineWidth: 1)
        )
    }

    // MARK: - Block Parsing

    private enum Block {
        case text(String)
        case code(language: String?, content: String)
    }

    private var blocks: [Block] {
        var result: [Block] = []
        var currentText = ""
        var inCodeBlock = false
        var codeLanguage: String?
        var codeContent = ""

        for line in text.components(separatedBy: "\n") {
            if line.hasPrefix("```") {
                if inCodeBlock {
                    // End code block
                    result.append(.code(language: codeLanguage, content: codeContent.trimmingCharacters(in: .newlines)))
                    codeContent = ""
                    codeLanguage = nil
                    inCodeBlock = false
                } else {
                    // Start code block
                    if !currentText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        result.append(.text(currentText.trimmingCharacters(in: .newlines)))
                        currentText = ""
                    }
                    let lang = String(line.dropFirst(3)).trimmingCharacters(in: .whitespaces)
                    codeLanguage = lang.isEmpty ? nil : lang
                    inCodeBlock = true
                }
            } else if inCodeBlock {
                if !codeContent.isEmpty { codeContent += "\n" }
                codeContent += line
            } else {
                if !currentText.isEmpty { currentText += "\n" }
                currentText += line
            }
        }

        // Remaining text
        if inCodeBlock {
            // Unclosed code block — treat as code anyway
            result.append(.code(language: codeLanguage, content: codeContent))
        }
        if !currentText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            result.append(.text(currentText.trimmingCharacters(in: .newlines)))
        }

        return result
    }
}
