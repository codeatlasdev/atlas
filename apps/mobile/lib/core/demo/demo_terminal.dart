/// The terminals of the demo computer: what an agent's pty shows when the phone attaches.
/// Nothing runs; the screen is a replay.
library;

import 'dart:convert';

import 'package:atlas_mobile/core/demo/demo_agents.dart';
import 'package:atlas_mobile/core/demo/demo_bus.dart';

class DemoTerminal {
  final DemoBus bus;
  final DemoAgents agents;
  DemoTerminal(this.bus, this.agents);

  /// Claude runs full screen in a wide Mac terminal, like the real one.
  Map<String, int> _size(String pty) => pty == 'pty-demo-claude'
      ? {'cols': 110, 'rows': 34}
      : {'cols': 80, 'rows': 30};

  void _replay(String pty) {
    if (pty == 'pty-demo-claude') return _replayTui(pty);
    final a = agents[pty.replaceFirst('pty-', '')];
    final buf = StringBuffer()
      ..write(
        '\x1b[38;2;122;162;247m${a?.agent ?? 'shell'}\x1b[0m  ~/Projetos/orbit-shop\r\n\r\n',
      );
    for (final m in a?.history ?? const <Map<String, String>>[]) {
      final who = m['role'] == 'user'
          ? '\x1b[38;2;158;206;106m❯\x1b[0m '
          : '  ';
      buf.write('$who${m['text']!.replaceAll('\n', '\r\n  ')}\r\n\r\n');
    }
    buf.write(
      '\x1b[2m(demo terminal: unlock to type, nothing runs)\x1b[0m\r\n',
    );
    bus.emit({
      'ev': 'pty_replay',
      'pty': pty,
      'data': base64.encode(utf8.encode(buf.toString())),
    });
  }

  /// A full-screen agent's screen: alternate buffer, laid out for 110 columns.
  void _replayTui(String pty) {
    const w = 110;
    String row(String text, [String sgr = '']) =>
        '\x1b[38;2;65;72;104m│\x1b[0m $sgr${text.padRight(w - 4)}\x1b[0m \x1b[38;2;65;72;104m│\x1b[0m\r\n';
    final buf = StringBuffer('\x1b[?1049h\x1b[H')
      ..write('\x1b[38;2;65;72;104m╭${'─' * (w - 2)}╮\x1b[0m\r\n')
      ..write(row('✻ Claude Code', '\x1b[1;38;2;224;175;104m'))
      ..write(row('~/Projetos/orbit-shop', '\x1b[2m'))
      ..write('\x1b[38;2;65;72;104m╰${'─' * (w - 2)}╯\x1b[0m\r\n\r\n')
      ..write(
        '\x1b[38;2;158;206;106m>\x1b[0m Add an index on orders.created_at and run the migration\r\n\r\n',
      )
      ..write('\x1b[38;2;192;202;245m⏺\x1b[0m Read(db/schema.sql)\r\n')
      ..write('  ⎿  Read 214 lines\r\n\r\n')
      ..write(
        '\x1b[38;2;192;202;245m⏺\x1b[0m Write(db/migrations/0042_orders_created_at.sql)\r\n',
      )
      ..write(
        '  ⎿  \x1b[38;2;158;206;106m+ CREATE INDEX CONCURRENTLY orders_created_at_idx ON orders (created_at);\x1b[0m\r\n\r\n',
      )
      ..write('\x1b[38;2;192;202;245m⏺\x1b[0m Bash(bun run db:migrate)\r\n\r\n')
      ..write('\x1b[38;2;224;175;104m╭${'─' * (w - 2)}╮\x1b[0m\r\n')
      ..write(row('Bash command', '\x1b[1m'))
      ..write(row('  bun run db:migrate'))
      ..write(
        row('  Run the pending migration 0042 against the local database'),
      )
      ..write(row(''))
      ..write(row('Do you want to proceed?'))
      ..write(row('❯ 1. Yes', '\x1b[38;2;122;162;247m'))
      ..write(
        row('  2. Yes, and don\'t ask again for bun run db:migrate commands'),
      )
      ..write(row('  3. No, and tell Claude what to do differently (esc)'))
      ..write('\x1b[38;2;224;175;104m╰${'─' * (w - 2)}╯\x1b[0m\r\n');
    bus.emit({
      'ev': 'pty_replay',
      'pty': pty,
      'data': base64.encode(utf8.encode(buf.toString())),
    });
  }

  /// The reply to `cmd`, or null when it is not one of this part's commands.
  DemoReply? handle(String cmd, Map<String, dynamic> args) {
    Object? data;
    String? error;
    switch (cmd) {
      case 'pty_list':
        data = {
          'ptys': [
            for (final a in agents.all)
              {
                'pty': 'pty-${a.pane}',
                'cwd': '~/Projetos/orbit-shop',
                'argv': [a.agent],
                'scope': {'pane_key': a.pane},
              },
          ],
        };
      case 'pty_info':
        data = {'pty': args['pty'], ..._size(args['pty'] as String)};
      case 'pty_attach':
        // Same order as hostd (mobile/session.rs): the replay goes out before the
        // reply, so a screen that listens only after attach returns sees nothing.
        _replay(args['pty'] as String);
        data = {'pty': args['pty']};
      case 'pty_write':
        final text = utf8.decode(base64.decode(args['data'] as String? ?? ''));
        bus.emit({
          'ev': 'pty_output',
          'pty': args['pty'],
          'data': base64.encode(utf8.encode(text.replaceAll('\r', '\r\n'))),
        });
        data = <String, Object?>{};
      default:
        return null;
    }
    return DemoReply(data: data, error: error);
  }
}
