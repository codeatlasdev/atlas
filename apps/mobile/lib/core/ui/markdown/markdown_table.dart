import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:markdown/markdown.dart' as md;

import '../atlas_theme.dart';
import 'bar_chart.dart';
import 'markdown_renderer.dart';
import 'numeric_column.dart';

class MarkdownTable extends StatefulWidget {
  const MarkdownTable({
    super.key,
    required this.table,
    required this.renderer,
    required this.style,
  });

  final md.Element table;
  final MarkdownRenderer renderer;
  final TextStyle style;

  @override
  State<MarkdownTable> createState() => _MarkdownTableState();
}

class _MarkdownTableState extends State<MarkdownTable> {
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
  void didUpdateWidget(MarkdownTable old) {
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
    final style = widget.style.copyWith(fontSize: TextSize.body, height: 1.35);

    Widget cell(md.Element cell, int col, {required bool header}) {
      final s = header
          ? style.copyWith(fontWeight: FontWeight.w600, color: c.text)
          : style;
      return Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: Space.md,
          vertical: Space.sm,
        ),
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
          const SizedBox(height: Space.sm),
          Align(
            alignment: Alignment.centerLeft,
            child: _ChartToggle(
              on: _chart,
              onTap: () => setState(() => _chart = !_chart),
            ),
          ),
          if (_chart) ...[
            const SizedBox(height: Space.sm),
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
          padding: const EdgeInsets.symmetric(
            horizontal: Space.sm,
            vertical: Space.sm,
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(Icons.bar_chart_rounded, size: 16, color: color),
              const SizedBox(width: Space.sm),
              Text(
                on ? 'Hide chart' : 'Chart',
                style: Theme.of(context).textTheme.bodySmall!
                    .copyWith(color: color),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
