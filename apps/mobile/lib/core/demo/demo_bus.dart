import 'dart:async';
import 'dart:convert';

int demoNow() => DateTime.now().millisecondsSinceEpoch;

/// A reply to a command: its data, or the refusal. A refusal hostd codes (`protocol::REPLY_CODES`) carries its
/// stable `code` and `args` as well as the English `error`.
class DemoReply {
  const DemoReply({this.data, this.error, this.code, this.args = const {}});
  final Object? data;
  final String? error;
  final String? code;
  final Map<String, Object?> args;
}

/// What every part of the demo computer shares: the line to the app, timers that stop
/// with it, and the counter that names new things (terminals, tasks).
class DemoBus {
  final _lines = StreamController<String>.broadcast();
  bool _open = true;
  int seq = 0;

  Stream<String> get lines => _lines.stream;
  bool get isOpen => _open;

  void emit(Map<String, Object?> v) {
    if (_open) _lines.add(jsonEncode(v));
  }

  void later(Duration d, void Function() f) => Timer(d, () {
    if (_open) f();
  });

  Future<void> close() async {
    if (!_open) return;
    _open = false;
    await _lines.close();
  }
}
