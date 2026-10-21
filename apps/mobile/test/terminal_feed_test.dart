import 'package:flutter_test/flutter_test.dart';
import 'package:xterm/xterm.dart';

import 'package:atlas_mobile/features/terminal/terminal_feed.dart';
import 'package:atlas_mobile/features/terminal/terminal_screen.dart';

List<FeedBlock> _feed(String out, {int cols = 20}) {
  final t = Terminal()..resize(cols, 10);
  t.write(out);
  return feedBlocks(t, atlasTerminalTheme);
}

void main() {
  test('a line the Mac wrapped at its width comes back as one line', () {
    final b = _feed('${'a' * 25}\r\nnext\r\n').single;
    expect(b.lines.map((l) => l.text), ['a' * 25, 'next']);
  });

  test('each prompt starts a block holding the output under it', () {
    final b = _feed('boot\r\n❯ ls\r\napi\r\nweb\r\n\$ pwd\r\n/x\r\n');
    expect(b.map((x) => x.command?.text), [null, '❯ ls', '\$ pwd']);
    expect(b[1].lines.map((l) => l.text), ['api', 'web']);
    expect(b[2].lines.map((l) => l.text), ['/x']);
  });

  test('a full-screen app is one live block, not split on its prompts', () {
    final b = _feed('history\r\n\x1b[?1049h\x1b[H❯ typed in a TUI\r\nbody');
    expect(b.single.screen, isTrue);
    expect(b.single.lines.map((l) => l.text), ['❯ typed in a TUI', 'body']);
  });

  test('colour and bold survive as styled runs', () {
    final l = _feed('\x1b[1;31merr\x1b[0m ok\r\n').single.lines.single;
    expect(l.runs.first.text, 'err');
    expect(l.runs.first.bold, isTrue);
    expect(l.runs.first.color, atlasTerminalTheme.red);
    expect(l.runs.last.color, isNull);
  });
}
