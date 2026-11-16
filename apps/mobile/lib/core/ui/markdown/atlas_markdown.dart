import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:markdown/markdown.dart' as md;

import '../atlas_theme.dart';
import 'markdown_renderer.dart';

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
    final base = (widget.style ?? Theme.of(context).textTheme.bodyLarge!)
        .copyWith(
          color: widget.style?.color ?? c.text,
          height: widget.style?.height ?? 1.5,
        );
    final body = MarkdownRenderer(
      context: context,
      colors: c,
      base: base,
      taps: _taps,
    ).blocks(_nodes);
    // One selection host for the whole message (per-block SelectableText is an
    // EditableText each: ~17x slower on long answers).
    return widget.selectable ? SelectionArea(child: body) : body;
  }
}
