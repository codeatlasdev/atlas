import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:xterm/xterm.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// One styled run of text inside a line.
typedef FeedRun = ({String text, Color? color, bool bold, bool faint});

/// A logical line: the terminal's soft-wrapped rows joined back together, so the
/// phone wraps it at its own width instead of the Mac's column count.
class FeedLine {
  FeedLine(this.runs);
  final List<FeedRun> runs;
  String get text => runs.map((r) => r.text).join();
}

/// A prompt line and the output under it. [command] is null for output before the
/// first prompt, or for a full-screen program's screen ([screen] true).
class FeedBlock {
  FeedBlock(this.command, this.lines, {this.screen = false});
  final FeedLine? command;
  final List<FeedLine> lines;
  final bool screen;
}

/// A line that starts a new block: a shell or agent prompt.
final _prompt = RegExp(r'^\s*(❯|➜|\$|%|#|>|›)\s');

/// Read [t]'s current buffer into blocks. The alternate screen (Claude, omp, vim…)
/// is one block of its visible rows: it is a picture, not a history.
List<FeedBlock> feedBlocks(Terminal t, TerminalTheme theme) {
  final buf = t.buffer;
  final logical = <FeedLine>[];
  for (var i = 0; i < buf.lines.length; i++) {
    final line = buf.lines[i];
    final runs = _runs(line, theme);
    if (line.isWrapped && logical.isNotEmpty) {
      logical.last.runs.addAll(runs);
    } else {
      logical.add(FeedLine(runs));
    }
  }
  while (logical.isNotEmpty && logical.last.text.trim().isEmpty) {
    logical.removeLast();
  }
  if (t.isUsingAltBuffer) return [FeedBlock(null, logical, screen: true)];
  final blocks = <FeedBlock>[];
  for (final l in logical) {
    if (_prompt.hasMatch(l.text)) {
      blocks.add(FeedBlock(l, []));
    } else if (blocks.isEmpty) {
      if (l.text.trim().isNotEmpty) blocks.add(FeedBlock(null, [l]));
    } else {
      blocks.last.lines.add(l);
    }
  }
  for (final b in blocks) {
    while (b.lines.isNotEmpty && b.lines.last.text.trim().isEmpty) {
      b.lines.removeLast();
    }
  }
  return blocks;
}

List<FeedRun> _runs(BufferLine line, TerminalTheme theme) {
  final out = <FeedRun>[];
  final n = line.getTrimmedLength();
  final sb = StringBuffer();
  int? fg;
  var attrs = 0;
  void flush() {
    if (sb.isEmpty) return;
    out.add((
      text: sb.toString(),
      color: _color(fg ?? 0, theme),
      bold: attrs & CellAttr.bold != 0,
      faint: attrs & CellAttr.faint != 0,
    ));
    sb.clear();
  }

  for (var x = 0; x < n; x++) {
    if (line.getWidth(x) == 0) continue;
    final f = line.getForeground(x), a = line.getAttributes(x);
    if (f != fg || a != attrs) {
      flush();
      fg = f;
      attrs = a;
    }
    final cp = line.getCodePoint(x);
    sb.writeCharCode(cp == 0 ? 0x20 : cp);
  }
  flush();
  return out;
}

Color? _color(int cell, TerminalTheme th) {
  final v = cell & CellColor.valueMask;
  switch (cell & CellColor.typeMask) {
    case CellColor.normal:
      return null;
    case CellColor.rgb:
      return Color(v | 0xFF000000);
  }
  const cube = [0, 95, 135, 175, 215, 255];
  final named = [
    th.black,
    th.red,
    th.green,
    th.yellow,
    th.blue,
    th.magenta,
    th.cyan,
    th.white,
    th.brightBlack,
    th.brightRed,
    th.brightGreen,
    th.brightYellow,
    th.brightBlue,
    th.brightMagenta,
    th.brightCyan,
    th.brightWhite,
  ];
  if (v < 16) return named[v];
  if (v < 232) {
    final i = v - 16;
    return Color.fromARGB(255, cube[i ~/ 36], cube[i ~/ 6 % 6], cube[i % 6]);
  }
  final g = 8 + (v - 232) * 10;
  return Color.fromARGB(255, g, g, g);
}

/// The terminal as a readable feed: newest at the bottom, each prompt and its
/// output a card, lines wrapped at the phone's width, long output folded.
class TerminalFeed extends StatefulWidget {
  const TerminalFeed({super.key, required this.terminal, required this.theme});
  final Terminal terminal;
  final TerminalTheme theme;

  @override
  State<TerminalFeed> createState() => _TerminalFeedState();
}

class _TerminalFeedState extends State<TerminalFeed> {
  List<FeedBlock> _blocks = const [];
  bool _scheduled = false;

  @override
  void initState() {
    super.initState();
    widget.terminal.addListener(_changed);
    _blocks = feedBlocks(widget.terminal, widget.theme);
  }

  @override
  void dispose() {
    widget.terminal.removeListener(_changed);
    super.dispose();
  }

  // Output arrives in bursts; rebuild at most once a frame.
  void _changed() {
    if (_scheduled) return;
    _scheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _scheduled = false;
      if (mounted) {
        setState(() => _blocks = feedBlocks(widget.terminal, widget.theme));
      }
    });
    WidgetsBinding.instance.scheduleFrame();
  }

  @override
  Widget build(BuildContext context) {
    if (_blocks.isEmpty) {
      return Center(
        child: Text(
          'Nothing on this terminal yet',
          style: TextStyle(color: AtlasColors.of(context).faint),
        ),
      );
    }
    // Reversed: the newest block sits on the bottom edge and stays there when
    // the keyboard comes up and the view shrinks.
    return ListView.builder(
      reverse: true,
      padding: const EdgeInsets.fromLTRB(
        Space.sm,
        Space.sm,
        Space.sm,
        Space.md,
      ),
      itemCount: _blocks.length,
      itemBuilder: (_, i) {
        final b = _blocks[_blocks.length - 1 - i];
        return _BlockCard(
          key: ValueKey(
            b.command?.text.trim() ??
                b.lines.firstOrNull?.text.trim() ??
                'block:$i',
          ),
          block: b,
          latest: i == 0,
          theme: widget.theme,
        );
      },
    );
  }
}

const _fold = 14;

class _BlockCard extends StatefulWidget {
  const _BlockCard({
    super.key,
    required this.block,
    required this.latest,
    required this.theme,
  });
  final FeedBlock block;
  final bool latest;
  final TerminalTheme theme;

  @override
  State<_BlockCard> createState() => _BlockCardState();
}

class _BlockCardState extends State<_BlockCard> {
  bool _open = false;

  TextSpan _span(FeedLine l, {double size = 13.5}) => TextSpan(
    children: [
      for (final r in l.runs)
        TextSpan(
          text: r.text,
          style: TextStyle(
            color: (r.color ?? widget.theme.foreground).withValues(
              alpha: r.faint ? .6 : 1,
            ),
            fontWeight: r.bold ? FontWeight.w700 : null,
          ),
        ),
    ],
    style: TextStyle(
      fontFamily: monoFamily,
      fontFamilyFallback: monoFallback,
      fontSize: size,
      height: 1.45,
    ),
  );

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final b = widget.block;
    // The latest block shows its tail (what just happened); older ones their head.
    final folded = !_open && b.lines.length > _fold;
    final shown = !folded
        ? b.lines
        : widget.latest || b.screen
        ? b.lines.sublist(b.lines.length - _fold)
        : b.lines.sublist(0, _fold);
    final more = TextButton(
      onPressed: () => setState(() => _open = true),
      child: Text('Show ${b.lines.length - _fold} more lines'),
    );
    void copy() {
      Clipboard.setData(
        ClipboardData(
          text: [
            if (b.command != null) b.command!.text.trim(),
            ...b.lines.map((l) => l.text.trimRight()),
          ].join('\n'),
        ),
      );
      HapticFeedback.mediumImpact();
      ScaffoldMessenger.of(context)
          .showSnackBar(const SnackBar(content: Text('Block copied')));
    }

    // Long-press belongs to text selection (SelectableText), so copying the
    // whole block is a visible button instead.
    final copyBtn = IconButton(
      icon: const Icon(Icons.copy_rounded, size: 16),
      tooltip: 'Copy block',
      color: c.faint,
      visualDensity: VisualDensity.compact,
      onPressed: copy,
    );
    return Padding(
      padding: const EdgeInsets.only(bottom: Space.sm),
      child: Container(
        decoration: BoxDecoration(
          color: c.raised.withValues(alpha: .55),
          borderRadius: BorderRadius.circular(12),
          border: Border.all(
            color: widget.latest
                ? c.accent.withValues(alpha: .35)
                : c.borderSubtle,
          ),
        ),
        clipBehavior: Clip.antiAlias,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Container(
              color: b.command != null ? c.raised : null,
              padding: const EdgeInsets.only(left: 12),
              child: Row(
                children: [
                  Expanded(
                    child: b.command != null
                        ? Text.rich(_span(b.command!, size: 14))
                        : Text(
                            b.screen ? 'Full-screen app · live view' : 'Output',
                            style: TextStyle(color: c.faint, fontSize: 11),
                          ),
                  ),
                  copyBtn,
                ],
              ),
            ),
            if (folded && !(widget.latest || b.screen)) ...[
              _body(shown),
              more,
            ] else ...[
              if (folded) more,
              if (shown.isNotEmpty) _body(shown),
            ],
          ],
        ),
      ),
    );
  }

  Widget _body(List<FeedLine> lines) => Padding(
    padding: const EdgeInsets.fromLTRB(12, 8, 12, 10),
    child: SelectableText.rich(
      TextSpan(
        children: [
          for (var i = 0; i < lines.length; i++) ...[
            if (i > 0) const TextSpan(text: '\n'),
            _span(lines[i]),
          ],
        ],
      ),
    ),
  );
}
