import 'dart:async';
import 'dart:convert';

import 'package:atlas_mobile/core/transport/transport.dart';

/// NDJSON request/response multiplexer over a [Transport].
///
/// - Outgoing requests: `{"id":N,"cmd":"...","args":{...}}`
/// - Incoming responses: `{"id":N,"ok":bool,"data":...}` or `{"id":N,"ok":false,"error":"..."}`
/// - Incoming push events: `{"ev":"..."}` (no `id` field)
///
/// A line longer than [maxLineBytes], or one that is not JSON, is never routed. When it was the
/// REPLY to a call, that call fails at once with the real cause ([_dropped] finds the id in the
/// line's own envelope); a line with no id to recover is counted in [droppedLines].
class Multiplexer {
  static const int maxLineBytes = 1024 * 1024; // 1 M UTF-16 code units (Dart String.length; not bytes for multi-byte chars)

  final Transport _transport;
  int _nextId = 1;
  final Map<int, Completer<Map<String, dynamic>>> _pending = {};

  /// One deadline per pending call; cancelled when its reply or the end arrives.
  final Map<int, Timer> _deadlines = {};
  bool _closed = false;

  final StreamController<Map<String, dynamic>> _pushController =
      StreamController<Map<String, dynamic>>.broadcast();

  final Completer<void> _doneCompleter = Completer<void>();

  late final StreamSubscription<String> _sub;

  /// Lines dropped (too big or malformed) that no pending call could be blamed for: a push event,
  /// or a reply with no readable id. For the connection screen to show; it never resets.
  int get droppedLines => _droppedLines;
  int _droppedLines = 0;

  /// Push events from the server (no `id` field, have `ev` field).
  Stream<Map<String, dynamic>> get pushEvents => _pushController.stream;

  /// Completes when the underlying transport closes.
  Future<void> get done => _doneCompleter.future;

  /// How long a call waits for its reply. A connection can die without closing
  /// (the Mac sleeps, a network drops); without a limit the caller waited forever.
  final Duration timeout;

  Multiplexer(this._transport, {this.timeout = const Duration(seconds: 20)}) {
    _sub = _transport.incomingLines.listen(
      _onLine,
      onDone: _onDone,
      onError: _onError,
      cancelOnError: false,
    );
  }

  /// hostd builds a reply with serde_json's `json!`: `{"id":N,"ok":..,"data"|"error":..}` or, without
  /// the `preserve_order` feature, keys sorted: `{"data"|"error":..,"id":N,"ok":..}`. Only the
  /// envelope's own ends are read, never the middle: nested objects carry an `id` too.
  static final _idFirst = RegExp(r'^\{"id":(\d+),');
  static final _idLast = RegExp(r'"id":(\d+),"ok":(?:true|false)\}$');

  /// An id is at most a few digits; looking at a short head and tail keeps a 1 MiB line cheap.
  static const _envelope = 64;

  /// The id of the call a line replies to, read from its envelope; null when it has none.
  static int? _replyId(String line) {
    final head = line.length > _envelope ? line.substring(0, _envelope) : line;
    final first = _idFirst.firstMatch(head);
    if (first != null) return int.tryParse(first.group(1)!);
    final tail = line.length > _envelope
        ? line.substring(line.length - _envelope)
        : line;
    final last = _idLast.firstMatch(tail);
    return last == null ? null : int.tryParse(last.group(1)!);
  }

  /// A line that cannot be used. If it answers a call, that call fails now with [why] instead of
  /// 20 s later with "did not answer"; a reply for a call that already ended is just late. Anything
  /// else is counted.
  void _dropped(String line, String Function() why) {
    final id = _replyId(line);
    if (id == null) {
      _droppedLines++;
      return;
    }
    _deadlines.remove(id)?.cancel();
    _pending.remove(id)?.complete({'id': id, 'ok': false, 'error': why()});
  }

  void _onLine(String line) {
    if (line.length > maxLineBytes) {
      _dropped(
        line,
        () =>
            'reply too large: ${(line.length / (1024 * 1024)).toStringAsFixed(1)} MB '
            '(the limit is ${maxLineBytes ~/ (1024 * 1024)} MB)',
      );
      return;
    }
    Map<String, dynamic> msg;
    try {
      msg = jsonDecode(line) as Map<String, dynamic>;
    } catch (_) {
      _dropped(line, () => 'malformed reply from the computer');
      return;
    }
    if (msg.containsKey('id')) {
      final id = msg['id'] as int?;
      if (id != null) {
        _deadlines.remove(id)?.cancel();
        _pending.remove(id)?.complete(msg);
      }
    } else if (msg.containsKey('ev')) {
      _pushController.add(msg);
    }
  }

  void _cancelDeadlines() {
    for (final t in _deadlines.values) {
      t.cancel();
    }
    _deadlines.clear();
  }

  void _onDone() {
    _closed = true;
    _cancelDeadlines();
    for (final entry in _pending.entries) {
      entry.value.complete({
        'id': entry.key,
        'ok': false,
        'error': 'connection closed',
      });
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
    // Propagate to pending completers as error maps, not thrown exceptions, so
    // every caller can do `resp['ok'] == false` instead of try/catch.
    _cancelDeadlines();
    for (final entry in _pending.entries) {
      if (!entry.value.isCompleted) {
        entry.value.complete({
          'id': entry.key,
          'ok': false,
          'error': error.toString(),
        });
      }
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
    // The deadline runs from the send, not after it: a reply that arrives while the line is still
    // being written cancels this timer, where one started afterwards would outlive the call.
    _deadlines[id] = Timer(timeout, () {
      _deadlines.remove(id);
      _pending.remove(id)?.complete({
        'id': id,
        'ok': false,
        'error': 'Your Mac did not answer in ${timeout.inSeconds}s',
      });
    });
    try {
      await _transport.sendLine(request);
    } catch (e) {
      _deadlines.remove(id)?.cancel();
      _pending.remove(id);
      return {'id': id, 'ok': false, 'error': e.toString()};
    }
    // Closed while sending: nothing will answer ([_onDone] already ended the call).
    if (_closed) {
      _deadlines.remove(id)?.cancel();
      _pending.remove(id);
      return {'id': id, 'ok': false, 'error': 'connection closed'};
    }
    return completer.future;
  }

  /// Close the multiplexer and the underlying transport. The transport is closed first,
  /// so its resources (sockets, timers) go even if cancelling the subscription is slow.
  Future<void> close() async {
    _closed = true;
    _cancelDeadlines();
    final closing = _transport.close();
    await _sub.cancel();
    await closing;
    _onDone();
  }
}
