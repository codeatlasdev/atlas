import 'dart:async';
import 'dart:convert';

import 'package:atlas_mobile/core/transport/transport.dart';

/// NDJSON request/response multiplexer over a [Transport].
///
/// - Outgoing requests: `{"id":N,"cmd":"...","args":{...}}`
/// - Incoming responses: `{"id":N,"ok":bool,"data":...}` or `{"id":N,"ok":false,"error":"..."}`
/// - Incoming push events: `{"ev":"..."}` (no `id` field)
///
/// Lines longer than [maxLineBytes] are silently dropped per contract.
class Multiplexer {
  static const int maxLineBytes = 1024 * 1024; // 1 MiB

  final Transport _transport;
  int _nextId = 1;
  final Map<int, Completer<Map<String, dynamic>>> _pending = {};

  final StreamController<Map<String, dynamic>> _pushController =
      StreamController<Map<String, dynamic>>.broadcast();

  final Completer<void> _doneCompleter = Completer<void>();

  late final StreamSubscription<String> _sub;

  /// Push events from the server (no `id` field, have `ev` field).
  Stream<Map<String, dynamic>> get pushEvents => _pushController.stream;

  /// Completes when the underlying transport closes.
  Future<void> get done => _doneCompleter.future;

  Multiplexer(this._transport) {
    _sub = _transport.incomingLines.listen(
      _onLine,
      onDone: _onDone,
      onError: _onError,
      cancelOnError: false,
    );
  }

  void _onLine(String line) {
    // Drop lines exceeding 1 MiB (wire contract).
    if (line.length > maxLineBytes) return;
    Map<String, dynamic> msg;
    try {
      msg = jsonDecode(line) as Map<String, dynamic>;
    } catch (_) {
      return; // malformed JSON — drop
    }
    if (msg.containsKey('id')) {
      final id = msg['id'] as int?;
      if (id != null) {
        _pending.remove(id)?.complete(msg);
      }
    } else if (msg.containsKey('ev')) {
      _pushController.add(msg);
    }
  }

  void _onDone() {
    for (final c in _pending.values) {
      c.completeError(Exception('Transport closed'));
    }
    _pending.clear();
    if (!_pushController.isClosed) {
      _pushController.close();
    }
    if (!_doneCompleter.isCompleted) {
      _doneCompleter.complete();
    }
  }

  void _onError(Object error, StackTrace stack) {
    // Propagate to pending completers.
    for (final c in _pending.values) {
      if (!c.isCompleted) c.completeError(error, stack);
    }
    _pending.clear();
  }

  /// Send a request and wait for the matching response.
  ///
  /// Returns the full response map (contains `ok`, `data` or `error`).
  Future<Map<String, dynamic>> call(
    String cmd, [
    Map<String, dynamic>? args,
  ]) async {
    final id = _nextId++;
    final request = jsonEncode({'id': id, 'cmd': cmd, 'args': args ?? {}});
    final completer = Completer<Map<String, dynamic>>();
    _pending[id] = completer;
    await _transport.sendLine(request);
    return completer.future;
  }

  /// Close the multiplexer and the underlying transport. The transport is closed first,
  /// so its resources (sockets, timers) go even if cancelling the subscription is slow.
  Future<void> close() async {
    final closing = _transport.close();
    await _sub.cancel();
    await closing;
    _onDone();
  }
}
