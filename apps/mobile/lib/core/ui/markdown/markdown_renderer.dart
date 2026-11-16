import 'package:flutter/gestures.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart' show PlatformException;
import 'package:markdown/markdown.dart' as md;
import 'package:url_launcher/url_launcher.dart';

import '../atlas_theme.dart';
import 'code_block.dart';
import 'markdown_table.dart';

class MarkdownRenderer {
  const MarkdownRenderer({
    required this.context,
    required this.colors,
    required this.base,
    required this.taps,
  });

  /// Where a link that cannot be opened says so (the nearest [ScaffoldMessenger]).
  final BuildContext context;
  final AtlasColors colors;
  final TextStyle base;
  final List<TapGestureRecognizer> taps;

  static const _gap = 10.0;

  MarkdownRenderer withTaps(List<TapGestureRecognizer> t) =>
      MarkdownRenderer(context: context, colors: colors, base: base, taps: t);

  // ---- blocks -------------------------------------------------------------

  Widget blocks(List<md.Node> nodes, {TextStyle? style}) {
    final children = <Widget>[];
    for (final n in nodes) {
      final w = block(n, style ?? base);
      if (w == null) continue;
      if (children.isNotEmpty) children.add(const SizedBox(height: _gap));
      children.add(w);
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: children,
    );
  }

  Widget? block(md.Node node, TextStyle style) {
    if (node is md.Text) {
      final t = node.text.trim();
      return t.isEmpty ? null : richText(inline([node], style), style);
    }
    final e = node as md.Element;
    switch (e.tag) {
      case 'p':
        return richText(inline(e.children ?? const [], style), style);
      case 'h1' || 'h2' || 'h3' || 'h4' || 'h5' || 'h6':
        // Headings step up the scale from the body: h1 headline, h2 title, the rest body weight.
        final (size, weight) = switch (e.tag) {
          'h1' => (TextSize.headline, FontWeight.w700),
          'h2' => (TextSize.title, FontWeight.w700),
          _ => (style.fontSize!, FontWeight.w600),
        };
        final hs = style.copyWith(
          fontSize: size,
          fontWeight: weight,
          height: 1.3,
        );
        final heading = richText(inline(e.children ?? const [], hs), hs);
        return Padding(
          padding: const EdgeInsets.only(top: Space.xs),
          child: e.tag == 'h1' || e.tag == 'h2'
              ? Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    heading,
                    const SizedBox(height: Space.xs),
                    Divider(height: 1, color: colors.borderSubtle),
                  ],
                )
              : heading,
        );
      case 'pre':
        return codeBlock(e);
      case 'blockquote':
        return Container(
          padding: const EdgeInsets.only(left: Space.md),
          decoration: BoxDecoration(
            border: Border(left: BorderSide(color: colors.accent, width: 3)),
          ),
          child: blocks(
            e.children ?? const [],
            style: style.copyWith(color: colors.muted),
          ),
        );
      case 'hr':
        return Divider(height: 1, color: colors.border);
      case 'ul' || 'ol':
        return list(e, style, 0);
      case 'table':
        return MarkdownTable(table: e, renderer: this, style: style);
      default:
        // Raw HTML and anything unknown: show the text, never drop content.
        final t = e.textContent.trim();
        return t.isEmpty
            ? null
            : richText(TextSpan(text: t, style: style), style);
    }
  }

  Widget codeBlock(md.Element pre) {
    final code = pre.children!.first as md.Element;
    final cls = code.attributes['class'] ?? '';
    final lang = cls.startsWith('language-') ? cls.substring(9) : '';
    var text = code.textContent;
    if (text.endsWith('\n')) text = text.substring(0, text.length - 1);
    return AtlasCodeBlock(code: text, language: lang);
  }

  Widget list(md.Element list, TextStyle style, int depth) {
    final ordered = list.tag == 'ol';
    var n = int.tryParse(list.attributes['start'] ?? '') ?? 1;
    final items = <Widget>[];
    for (final item in list.children ?? const <md.Node>[]) {
      if (item is! md.Element || item.tag != 'li') continue;
      final task = _taskState(item);
      final Widget marker = task != null
          ? Padding(
              padding: const EdgeInsets.only(top: 2),
              child: Icon(
                task ? Icons.check_box : Icons.check_box_outline_blank,
                size: TextSize.title,
                color: task ? colors.success : colors.faint,
              ),
            )
          : Text(
              ordered ? '${n++}.' : (depth.isEven ? '•' : '◦'),
              style: style.copyWith(
                color: ordered ? colors.muted : colors.accent,
                fontFeatures: const [FontFeature.tabularFigures()],
              ),
            );
      items.add(
        Padding(
          padding: const EdgeInsets.only(bottom: Space.xs),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              SizedBox(
                width: ordered ? 26 : 20,
                child: Align(alignment: Alignment.topLeft, child: marker),
              ),
              Expanded(child: listItem(item, style, depth)),
            ],
          ),
        ),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: items,
    );
  }

  /// `true`/`false` for a task item's checkbox, null for a plain item.
  bool? _taskState(md.Element li) {
    final first = li.children?.firstOrNull;
    final input = switch (first) {
      md.Element(tag: 'input') => first,
      md.Element(tag: 'p') => first.children?.firstOrNull as md.Element?,
      _ => null,
    };
    if (input is! md.Element || input.tag != 'input') return null;
    return input.attributes.containsKey('checked');
  }

  /// A list item holds inline runs (tight) and/or blocks (loose, nested lists).
  Widget listItem(md.Element li, TextStyle style, int depth) {
    final parts = <Widget>[];
    var run = <md.Node>[];
    void flush() {
      if (run.isEmpty) return;
      parts.add(richText(inline(run, style), style));
      run = [];
    }

    void add(Widget w) {
      if (parts.isNotEmpty) parts.add(const SizedBox(height: Space.xs));
      parts.add(w);
    }

    for (final child in li.children ?? const <md.Node>[]) {
      if (child is md.Element && child.tag == 'input') continue;
      if (child is md.Element && (child.tag == 'ul' || child.tag == 'ol')) {
        final pending = run.isNotEmpty;
        flush();
        final nested = list(child, style, depth + 1);
        if (pending) parts.add(const SizedBox(height: Space.xs));
        add(nested);
      } else if (child is md.Element && child.tag == 'p') {
        flush();
        add(richText(inline(child.children ?? const [], style), style));
      } else if (child is md.Element && !_isInline(child.tag)) {
        flush();
        final w = block(child, style);
        if (w != null) add(w);
      } else {
        run.add(child);
      }
    }
    flush();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: parts,
    );
  }

  static bool _isInline(String tag) => const {
    'strong',
    'em',
    'del',
    'code',
    'a',
    'br',
    'img',
    'span',
  }.contains(tag);

  // ---- inline -------------------------------------------------------------

  Widget richText(InlineSpan span, TextStyle style) =>
      Text.rich(TextSpan(style: style, children: [span]));

  /// Inline nodes → one [TextSpan] tree. Link recognizers are registered in
  /// [taps] and disposed by the owning widget state.
  InlineSpan inline(List<md.Node> nodes, TextStyle style) =>
      TextSpan(children: [for (final n in nodes) _inline(n, style)]);

  InlineSpan _inline(md.Node node, TextStyle style) {
    if (node is md.Text) return TextSpan(text: node.text);
    final e = node as md.Element;
    final kids = e.children ?? const <md.Node>[];
    List<InlineSpan> sub(TextStyle s) => [for (final k in kids) _inline(k, s)];
    switch (e.tag) {
      case 'strong':
        return TextSpan(
          style: const TextStyle(fontWeight: FontWeight.w700),
          children: sub(style),
        );
      case 'em':
        return TextSpan(
          style: const TextStyle(fontStyle: FontStyle.italic),
          children: sub(style),
        );
      case 'del':
        return TextSpan(
          style: TextStyle(
            decoration: TextDecoration.lineThrough,
            color: colors.muted,
          ),
          children: sub(style),
        );
      case 'code':
        return WidgetSpan(
          alignment: PlaceholderAlignment.baseline,
          baseline: TextBaseline.alphabetic,
          child: Container(
            padding: const EdgeInsets.symmetric(
              horizontal: Space.xs,
              vertical: 1,
            ),
            decoration: BoxDecoration(
              color: colors.raised,
              borderRadius: BorderRadius.circular(5),
              border: Border.all(color: colors.borderSubtle),
            ),
            child: Text(
              e.textContent,
              style: style.copyWith(
                fontFamily: monoFamily,
                fontFamilyFallback: monoFallback,
                fontSize: TextSize.small,
                height: 1.2,
                color: colors.text,
              ),
            ),
          ),
        );
      case 'a':
        final href = e.attributes['href'] ?? '';
        final tap = TapGestureRecognizer()..onTap = () => _open(href);
        taps.add(tap);
        return TextSpan(
          style: TextStyle(
            color: colors.accent,
            decoration: TextDecoration.underline,
            decorationColor: colors.accent.withValues(alpha: .4),
          ),
          recognizer: tap,
          children: sub(style),
        );
      case 'br':
        return const TextSpan(text: '\n');
      case 'img':
        final alt = e.attributes['alt'] ?? e.attributes['src'] ?? '';
        return TextSpan(
          text: alt,
          style: TextStyle(color: colors.muted),
        );
      default:
        return TextSpan(text: e.textContent);
    }
  }

  /// Opens [href] outside the app. A link with no scheme is not a link (nothing to open); one the
  /// system cannot or will not open is reported, never swallowed.
  Future<void> _open(String href) async {
    final uri = Uri.tryParse(href);
    if (uri == null || !uri.hasScheme) return;
    final messenger = ScaffoldMessenger.maybeOf(context);
    String? why;
    try {
      if (!await launchUrl(uri, mode: LaunchMode.externalApplication)) {
        why = 'no app can open it';
      }
    } catch (e) {
      why = e is PlatformException && e.message != null ? e.message! : '$e';
    }
    if (why == null || !context.mounted) return;
    messenger?.showSnackBar(
      SnackBar(content: Text("Couldn't open $href: $why")),
    );
  }
}

// ---- tables -----------------------------------------------------------------
