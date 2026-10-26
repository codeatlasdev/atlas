import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:markdown/markdown.dart' as md;
import 'package:url_launcher/url_launcher.dart';

import '../atlas_theme.dart';
import 'bar_chart.dart';
import 'code_block.dart';
import 'numeric_column.dart';

/// GitHub-flavoured Markdown rendered with Atlas's palette. The text is parsed
/// once per [data] (in `State`, not per build); partial/streaming input is fine:
/// an unclosed fence is a code block, a half-typed table is plain text.
class AtlasMarkdown extends StatefulWidget {
  const AtlasMarkdown(
    this.data, {
    super.key,
    this.style,
    this.selectable = true,
  });

  final String data;
  final TextStyle? style;
  final bool selectable;

  @override
  State<AtlasMarkdown> createState() => _AtlasMarkdownState();
}

class _AtlasMarkdownState extends State<AtlasMarkdown> {
  late List<md.Node> _nodes = _parse(widget.data);
  final _taps = <TapGestureRecognizer>[];

  static List<md.Node> _parse(String data) => md.Document(
    extensionSet: md.ExtensionSet.gitHubWeb,
    encodeHtml: false,
  ).parseLines(data.replaceAll('\r\n', '\n').split('\n'));

  @override
  void didUpdateWidget(AtlasMarkdown old) {
    super.didUpdateWidget(old);
    if (old.data != widget.data) _nodes = _parse(widget.data);
  }

  void _disposeTaps() {
    for (final t in _taps) {
      t.dispose();
    }
    _taps.clear();
  }

  @override
  void dispose() {
    _disposeTaps();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    // Link recognizers belong to this build's spans; the previous build's are
    // released here (the old spans are discarded with the old widget tree).
    _disposeTaps();
    final c = AtlasColors.of(context);
    final base = (widget.style ?? const TextStyle()).copyWith(
      color: widget.style?.color ?? c.text,
      fontSize: widget.style?.fontSize ?? 14.5,
      height: widget.style?.height ?? 1.5,
    );
    final body = _Renderer(colors: c, base: base, taps: _taps).blocks(_nodes);
    // One selection host for the whole message (per-block SelectableText is an
    // EditableText each: ~17x slower on long answers).
    return widget.selectable ? SelectionArea(child: body) : body;
  }
}

class _Renderer {
  const _Renderer({
    required this.colors,
    required this.base,
    required this.taps,
  });

  final AtlasColors colors;
  final TextStyle base;
  final List<TapGestureRecognizer> taps;

  static const _gap = 10.0;

  _Renderer withTaps(List<TapGestureRecognizer> t) =>
      _Renderer(colors: colors, base: base, taps: t);

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
        final (size, weight) = switch (e.tag) {
          'h1' => (style.fontSize! + 6, FontWeight.w700),
          'h2' => (style.fontSize! + 3.5, FontWeight.w700),
          'h3' => (style.fontSize! + 1.5, FontWeight.w600),
          _ => (style.fontSize!, FontWeight.w600),
        };
        final hs = style.copyWith(
          fontSize: size,
          fontWeight: weight,
          height: 1.3,
        );
        final heading = richText(inline(e.children ?? const [], hs), hs);
        return Padding(
          padding: const EdgeInsets.only(top: 4),
          child: e.tag == 'h1' || e.tag == 'h2'
              ? Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    heading,
                    const SizedBox(height: 4),
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
        return _MarkdownTable(table: e, renderer: this, style: style);
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
    return AtlasCodeBlock(
      code: text,
      language: lang,
      fontSize: (base.fontSize ?? 14.5) - 2,
    );
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
                size: (style.fontSize ?? 14.5) + 2,
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
          padding: const EdgeInsets.only(bottom: 4),
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
      if (parts.isNotEmpty) parts.add(const SizedBox(height: 4));
      parts.add(w);
    }

    for (final child in li.children ?? const <md.Node>[]) {
      if (child is md.Element && child.tag == 'input') continue;
      if (child is md.Element && (child.tag == 'ul' || child.tag == 'ol')) {
        final pending = run.isNotEmpty;
        flush();
        final nested = list(child, style, depth + 1);
        if (pending) parts.add(const SizedBox(height: 4));
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
            padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1),
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
                fontSize: (style.fontSize ?? 14.5) - 1.5,
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

  static Future<void> _open(String href) async {
    final uri = Uri.tryParse(href);
    if (uri == null || !uri.hasScheme) return;
    try {
      await launchUrl(uri, mode: LaunchMode.externalApplication);
    } catch (_) {
      // No handler for the scheme: the tap is simply a no-op.
    }
  }
}

// ---- tables -----------------------------------------------------------------

class _MarkdownTable extends StatefulWidget {
  const _MarkdownTable({
    required this.table,
    required this.renderer,
    required this.style,
  });

  final md.Element table;
  final _Renderer renderer;
  final TextStyle style;

  @override
  State<_MarkdownTable> createState() => _MarkdownTableState();
}

class _MarkdownTableState extends State<_MarkdownTable> {
  bool _chart = false;

  late List<md.Element> _head;
  late List<List<md.Element>> _body;
  late List<TextAlign> _align;
  late List<List<String>> _bodyText;
  late NumericColumn? _numeric;
  final _taps = <TapGestureRecognizer>[];

  @override
  void dispose() {
    for (final t in _taps) {
      t.dispose();
    }
    super.dispose();
  }

  @override
  void initState() {
    super.initState();
    _index();
  }

  @override
  void didUpdateWidget(_MarkdownTable old) {
    super.didUpdateWidget(old);
    if (!identical(old.table, widget.table)) _index();
  }

  List<md.Element> _rows(String section) => [
    for (final s in widget.table.children!.whereType<md.Element>())
      if (s.tag == section)
        ...s.children!.whereType<md.Element>().where((r) => r.tag == 'tr'),
  ];

  void _index() {
    List<md.Element> cells(md.Element row) =>
        row.children!.whereType<md.Element>().toList();
    _head = _rows('thead').isEmpty ? [] : cells(_rows('thead').first);
    if (_head.isEmpty) {
      // Cannot happen with GFM output, but never crash on odd input.
      _body = [];
      _align = [];
      _bodyText = [];
      _numeric = null;
      return;
    }
    _body = [for (final r in _rows('tbody')) cells(r)];
    final cols = _head.length;
    // Rows may be ragged (streaming): pad/trim to the header width.
    _body = [
      for (final r in _body)
        [
          ...r.take(cols),
          for (var i = r.length; i < cols; i++) md.Element.empty('td'),
        ],
    ];
    _align = [
      for (final h in _head)
        switch (h.attributes['align']) {
          'center' => TextAlign.center,
          'right' => TextAlign.right,
          _ => TextAlign.left,
        },
    ];
    _bodyText = [
      for (final r in _body) [for (final c in r) c.textContent.trim()],
    ];
    _numeric = NumericColumn.find(_bodyText);
  }

  @override
  Widget build(BuildContext context) {
    for (final t in _taps) {
      t.dispose();
    }
    _taps.clear();
    final c = widget.renderer.colors;
    final r = widget.renderer.withTaps(_taps);
    final style = widget.style.copyWith(
      fontSize: (widget.style.fontSize ?? 14.5) - 1,
      height: 1.35,
    );

    Widget cell(md.Element cell, int col, {required bool header}) {
      final s = header
          ? style.copyWith(fontWeight: FontWeight.w600, color: c.text)
          : style;
      return Padding(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
        child: Text.rich(
          TextSpan(
            style: s,
            children: [r.inline(cell.children ?? const [], s)],
          ),
          textAlign: _align[col],
        ),
      );
    }

    final border = TableBorder(
      horizontalInside: BorderSide(color: c.borderSubtle),
      verticalInside: BorderSide(color: c.borderSubtle),
    );
    final table = Table(
      defaultColumnWidth: const IntrinsicColumnWidth(),
      defaultVerticalAlignment: TableCellVerticalAlignment.middle,
      border: border,
      children: [
        TableRow(
          decoration: BoxDecoration(color: c.raised),
          children: [
            for (var i = 0; i < _head.length; i++)
              cell(_head[i], i, header: true),
          ],
        ),
        for (final row in _body)
          TableRow(
            children: [
              for (var i = 0; i < row.length; i++)
                cell(row[i], i, header: false),
            ],
          ),
      ],
    );

    final numeric = _numeric;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          width: double.infinity,
          clipBehavior: Clip.antiAlias,
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(Space.radiusSm),
            border: Border.all(color: c.border),
          ),
          child: SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: table,
          ),
        ),
        if (numeric != null) ...[
          const SizedBox(height: 6),
          Align(
            alignment: Alignment.centerLeft,
            child: _ChartToggle(
              on: _chart,
              onTap: () => setState(() => _chart = !_chart),
            ),
          ),
          if (_chart) ...[
            const SizedBox(height: 6),
            AtlasBarChart(
              title: _head[numeric.index].textContent.trim(),
              labels: [for (final row in _bodyText) row.first],
              values: numeric.values,
              unit: numeric.unit,
            ),
          ],
        ],
      ],
    );
  }
}

class _ChartToggle extends StatelessWidget {
  const _ChartToggle({required this.on, required this.onTap});

  final bool on;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final color = on ? c.accent : c.muted;
    return Semantics(
      button: true,
      selected: on,
      label: on ? 'Hide chart' : 'Show chart',
      child: InkWell(
        key: const ValueKey('atlas-md-chart-toggle'),
        borderRadius: BorderRadius.circular(Space.radiusSm),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(Icons.bar_chart_rounded, size: 16, color: color),
              const SizedBox(width: 6),
              Text(
                on ? 'Hide chart' : 'Chart',
                style: TextStyle(color: color, fontSize: 12.5),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
