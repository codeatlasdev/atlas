import 'dart:async';
import 'dart:convert';

import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/transport/transport.dart';

/// In-memory [Transport] for unit tests.
///
/// Inject lines with [injectLine]; inspect sent lines via [sentLines].
final class FakeTransport implements Transport {
  /// Answers the first `info` request like a current hostd (`proto` = the app's), as the connection
  /// check on connect needs; every other request stays for the test to answer with [injectLine],
  /// and a later `info` (the foreground probe) too.
  FakeTransport({this.answerInfo = false});
  bool answerInfo;

  final StreamController<String> _controller =
      StreamController<String>.broadcast();

  bool _open = true;

  /// Lines that were passed to [sendLine].
  final List<String> sentLines = [];

  @override
  Stream<String> get incomingLines => _controller.stream;

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
    if (answerInfo && line.contains('"cmd":"info"')) {
      answerInfo = false;
      final id = jsonDecode(line)['id'];
      scheduleMicrotask(() {
        if (_open) {
          _controller.add('{"id":$id,"ok":true,"data":{"proto":$kAppProto}}');
        }
      });
    }
  }

  @override
  Future<void> close() async {
    if (!_open) return;
    _open = false;
    await _controller.close();
  }
}
