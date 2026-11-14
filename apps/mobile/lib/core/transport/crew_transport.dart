/// A crew server reached through a paired Mac: the phone only talks to its Macs, and the
/// Mac's hostd relays to the crew servers its desktop knows (`crew_call`, `crew_event`).
library;

import 'dart:async';
import 'dart:convert';

import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/transport/transport.dart';

/// Lines to and from crew server [server], carried by the Mac's [parent] connection.
///
/// A request `{id, cmd, args}` becomes `crew_call {server, cmd, args}` on the Mac, and its
/// answer comes back under the original id; the Mac's `crew_event` pushes for [server] come
/// out unwrapped. A [Multiplexer] on top behaves exactly as if the phone talked to [server].
class CrewTransport implements Transport {
  CrewTransport(this.parent, this.server) {
    _events = parent.pushEvents.listen((e) {
      if (e['ev'] != 'crew_event' || e['server'] != server) return;
      final inner = e['event'];
      if (inner is Map) _lines.add(jsonEncode(inner));
    }, onDone: _end);
    parent.done.then((_) => _end());
  }

  final Multiplexer parent;
  final String server;
  final _lines = StreamController<String>.broadcast();
  late final StreamSubscription<Map<String, dynamic>> _events;

  @override
  Stream<String> get incomingLines => _lines.stream;

  @override
  Future<void> sendLine(String line) async {
    if (_lines.isClosed) throw StateError('closed');
    final req = jsonDecode(line) as Map<String, dynamic>;
    final id = req['id'];
    unawaited(
      parent
          .call('crew_call', {
            'server': server,
            'cmd': req['cmd'],
            'args': req['args'] ?? const {},
          })
          .then((resp) {
            if (_lines.isClosed) return;
            _lines.add(jsonEncode({...resp, 'id': id}));
          }),
    );
  }

  void _end() {
    if (!_lines.isClosed) _lines.close();
  }

  @override
  Future<void> close() async {
    await _events.cancel();
    _end();
  }
}
