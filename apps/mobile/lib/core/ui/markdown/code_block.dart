import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../atlas_theme.dart';

/// Fenced code: language label, copy button, horizontal scroll, soft tinting.
class AtlasCodeBlock extends StatefulWidget {
  const AtlasCodeBlock({
    super.key,
    required this.code,
    this.language = '',
    this.fontSize = 12.5,
  });

  final String code;
  final String language;
  final double fontSize;

  @override
  State<AtlasCodeBlock> createState() => _AtlasCodeBlockState();
}

class _AtlasCodeBlockState extends State<AtlasCodeBlock> {
  bool _copied = false;

  Future<void> _copy() async {
    await Clipboard.setData(ClipboardData(text: widget.code));
    if (!mounted) return;
    setState(() => _copied = true);
    await Future<void>.delayed(const Duration(seconds: 2));
    if (mounted) setState(() => _copied = false);
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final base = TextStyle(
      fontFamily: monoFamily,
      fontFamilyFallback: monoFallback,
      fontSize: widget.fontSize,
      height: 1.45,
      color: c.text,
    );
    final span = highlight(widget.code, widget.language, base, c);
    final body = Text.rich(span, softWrap: false);
    return Container(
      width: double.infinity,
      decoration: BoxDecoration(
        color: c.ground,
        borderRadius: BorderRadius.circular(Space.radiusSm),
        border: Border.all(color: c.borderSubtle),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            padding: const EdgeInsets.only(left: Space.md),
            decoration: BoxDecoration(
              border: Border(bottom: BorderSide(color: c.borderSubtle)),
            ),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    widget.language.isEmpty ? 'text' : widget.language,
                    style: TextStyle(
                      color: c.faint,
                      fontSize: 11,
                      letterSpacing: .3,
                    ),
                  ),
                ),
                IconButton(
                  key: const ValueKey('atlas-md-copy'),
                  tooltip: _copied ? 'Copied' : 'Copy',
                  visualDensity: VisualDensity.compact,
                  iconSize: 16,
                  color: _copied ? c.success : c.muted,
                  onPressed: _copy,
                  icon: Icon(_copied ? Icons.check : Icons.content_copy),
                ),
              ],
            ),
          ),
          SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            padding: const EdgeInsets.all(Space.md),
            child: body,
          ),
        ],
      ),
    );
  }
}

final _tokens = RegExp(
  r'''(?<comment>//[^\n]*|#[^\n]*|/\*[\s\S]*?\*/)'''
  r'''|(?<string>"(?:\\.|[^"\\\n])*"?|'(?:\\.|[^'\\\n])*'?|`[^`\n]*`?)'''
  r'''|(?<number>\b\d[\d_]*(?:\.\d+)?\b)'''
  r'''|(?<word>[A-Za-z_][A-Za-z0-9_]*)''',
);

const _keywords = {
  'if',
  'else',
  'for',
  'while',
  'return',
  'fn',
  'func',
  'function',
  'def',
  'class',
  'struct',
  'enum',
  'impl',
  'trait',
  'let',
  'const',
  'var',
  'final',
  'import',
  'from',
  'use',
  'pub',
  'mod',
  'async',
  'await',
  'match',
  'switch',
  'case',
  'break',
  'continue',
  'in',
  'of',
  'new',
  'try',
  'catch',
  'throw',
  'true',
  'false',
  'null',
  'None',
  'True',
  'False',
  'self',
  'this',
  'mut',
  'static',
  'void',
  'int',
  'String',
  'bool',
  'type',
  'interface',
  'export',
  'default',
  'extends',
  'implements',
  'with',
  'as',
  'is',
  'not',
  'and',
  'or',
};

/// Soft syntax tinting: comments, strings, numbers and keywords. Plain text and
/// diffs/logs are left mostly untouched so output blocks stay readable.
@visibleForTesting
TextSpan highlight(
  String code,
  String language,
  TextStyle base,
  AtlasColors c,
) {
  final lang = language.toLowerCase();
  if (lang == 'mermaid' ||
      lang == 'text' ||
      lang == 'txt' ||
      lang == 'log' ||
      lang == 'output' ||
      code.length > 20000) {
    return TextSpan(text: code, style: base);
  }
  if (lang == 'diff' || lang == 'patch') {
    final lines = code.split('\n');
    return TextSpan(
      style: base,
      children: [
        for (final (i, line) in lines.indexed)
          TextSpan(
            text: i == lines.length - 1 ? line : '$line\n',
            style: line.startsWith('+') && !line.startsWith('+++')
                ? base.copyWith(color: c.success)
                : line.startsWith('-') && !line.startsWith('---')
                ? base.copyWith(color: c.error)
                : line.startsWith('@@')
                ? base.copyWith(color: c.info)
                : null,
          ),
      ],
    );
  }
  final spans = <TextSpan>[];
  var last = 0;
  for (final m in _tokens.allMatches(code)) {
    Color? color;
    if (m.namedGroup('comment') != null) {
      // `#` only starts a comment in hash-comment languages, `//` elsewhere.
      final t = m.group(0)!;
      final hash = t.startsWith('#');
      const hashLangs = {
        'python',
        'py',
        'sh',
        'bash',
        'zsh',
        'shell',
        'yaml',
        'yml',
        'toml',
        'ruby',
        'rb',
        'dockerfile',
        'makefile',
        'conf',
        'ini',
        '',
      };
      if (hash && !hashLangs.contains(lang)) continue;
      color = c.faint;
    } else if (m.namedGroup('string') != null) {
      color = c.success;
    } else if (m.namedGroup('number') != null) {
      color = c.warning;
    } else if (_keywords.contains(m.group(0))) {
      color = c.series[1];
    } else {
      continue;
    }
    if (m.start > last) {
      spans.add(TextSpan(text: code.substring(last, m.start)));
    }
    spans.add(
      TextSpan(
        text: m.group(0),
        style: TextStyle(color: color),
      ),
    );
    last = m.end;
  }
  if (last < code.length) spans.add(TextSpan(text: code.substring(last)));
  return TextSpan(style: base, children: spans);
}
