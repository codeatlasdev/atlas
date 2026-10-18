import 'dart:async';

import 'transport.dart';

/// In-memory [Transport] for unit tests.
///
/// Inject lines with [injectLine]; inspect sent lines via [sentLines].
final class FakeTransport implements Transport {
  final StreamController<String> _controller =
      StreamController<String>.broadcast();

  bool _open = true;

  /// Lines that were passed to [sendLine].
  final List<String> sentLines = [];

  @override
  Stream<String> get incomingLines => _controller.stream;

  @override
  bool get isOpen => _open;

  /// Push a line as if it arrived from the remote end.
  void injectLine(String line) {
    if (!_open) throw StateError('FakeTransport is closed');
    _controller.add(line);
  }

  /// Simulate the remote closing the connection.
  Future<void> simulateRemoteClose() async {
    _open = false;
    await _controller.close();
  }

  @override
  Future<void> sendLine(String line) async {
    if (!_open) throw StateError('FakeTransport is closed');
    sentLines.add(line);
  }

  @override
  Future<void> close() async {
    if (!_open) return;
    _open = false;
    await _controller.close();
  }
}
