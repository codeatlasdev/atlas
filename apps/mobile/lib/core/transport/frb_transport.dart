import 'dart:async';

import 'package:atlas_mobile/src/rust/api.dart' as native;

import 'transport.dart';

/// [Transport] backed by the native iroh crate (`apps/mobile/native`).
final class FrbTransport implements Transport {
  FrbTransport._(this._endpoint, this._conn) {
    _readLoop();
  }

  /// Held for the connection's lifetime: dropping the endpoint closes it.
  final native.NativeEndpoint _endpoint;
  final native.NativeConn _conn;
  final _lines = StreamController<String>.broadcast();
  bool _open = true;

  /// Tracks whether [_endpoint] has been disposed so [close] is idempotent
  /// even when [_readLoop] sets [_open] to false on a remote close.
  bool _disposed = false;

  @override
  Stream<String> get incomingLines => _lines.stream;

  /// Dial the Mac [nodeId] as the phone identity [secretKeyHex] (32 bytes, hex).
  /// [addrs] and [relay] are the optional hints from the pairing QR code.
  static Future<FrbTransport> connect({
    required String secretKeyHex,
    required String nodeId,
    required List<String> addrs,
    String? relay,
  }) async {
    final ep = await native.open(secretKeyHex: secretKeyHex);
    try {
      final conn = await native.connect(
        ep: ep,
        nodeId: nodeId,
        addrs: addrs,
        relay: relay,
      );
      return FrbTransport._(ep, conn);
    } catch (e) {
      // Dispose the endpoint so its sockets and relay session are released
      // immediately, rather than waiting for GC on every failed retry.
      ep.dispose();
      rethrow;
    }
  }

  Future<void> _readLoop() async {
    try {
      while (_open) {
        final line = await native.readLine(conn: _conn);
        if (line == null) break;
        _lines.add(line);
      }
    } catch (e) {
      if (_open) _lines.addError(e);
    }
    _open = false;
    await _lines.close();
  }

  @override
  Future<void> sendLine(String line) async {
    if (!_open) throw StateError('connection closed');
    await native.sendLine(conn: _conn, line: line);
  }

  @override
  Future<void> close() async {
    if (_disposed) return;
    _disposed = true;
    _open = false;
    try {
      await native.closeConn(conn: _conn);
    } catch (_) {
      // Best-effort: the stream may already be gone after a remote close.
    } finally {
      _endpoint.dispose();
    }
  }
}
