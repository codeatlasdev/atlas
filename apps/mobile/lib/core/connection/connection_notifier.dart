import 'dart:async';

import 'package:flutter/foundation.dart';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/frb_transport.dart';
import 'package:atlas_mobile/core/transport/transport.dart';

/// Factory that creates a [Transport] for a given [MacDevice].
typedef TransportFactory = Future<Transport> Function(
  MacDevice device,
  String secretKeyHex,
);

Future<Transport> _defaultTransportFactory(
  MacDevice device,
  String secretKeyHex,
) async => FrbTransport.connect(
  secretKeyHex: secretKeyHex,
  nodeId: device.nodeId,
  addrs: device.addrs,
  relay: device.relay,
);

final transportFactoryProvider = Provider<TransportFactory>(
  (ref) => _defaultTransportFactory,
);

/// Provides the [MacStorage] singleton.
final macStorageProvider = Provider<MacStorage>((ref) => MacStorage());

/// Manages the [Multiplexer] lifecycle for a single paired Mac.
///
/// - Reconnects with exponential backoff (1 s → 30 s) while the app is foregrounded.
/// - Disconnects 30 s after the app backgrounds.
/// - Family key: [MacDevice] (equality on nodeId keeps the provider stable).
final connectionNotifierProvider = AsyncNotifierProvider.autoDispose
    .family<ConnectionNotifier, Multiplexer?, MacDevice>(
      ConnectionNotifier.new,
    );

class ConnectionNotifier extends AsyncNotifier<Multiplexer?> {
  ConnectionNotifier(this._device);
  final MacDevice _device;
  static const Duration _backgroundGrace = Duration(seconds: 30);
  static const int _maxBackoff = 30;

  Multiplexer? _mux;

  /// How long the liveness probe waits. A healthy Mac answers `info` in well under
  /// a second; the request deadline (20 s) is far too long to find a dead link.
  @visibleForTesting
  Duration probeTimeout = const Duration(seconds: 2);

  Timer? _reconnectTimer;
  Timer? _backgroundTimer;
  int _backoffSeconds = 1;
  bool _foregrounded = true;

  @override
  FutureOr<Multiplexer?> build() {
    ref.onDispose(_cleanup);
    return _connect();
  }

  Future<Multiplexer?> _connect() async {
    try {
      final secretKey =
          await ref.read(macStorageProvider).loadSecretKey() ?? '';
      final transport = await ref.read(transportFactoryProvider)(
        _device,
        secretKey,
      );
      final mux = Multiplexer(transport);
      _mux = mux;
      _backoffSeconds = 1;

      // Reconnect when transport closes (only while foregrounded).
      mux.done.then((_) {
        if (identical(mux, _mux) && _foregrounded && ref.mounted) {
          _scheduleReconnect();
        }
      });

      return mux;
    } catch (e) {
      debugPrint('atlas-mobile: connect to ${_device.nodeId} failed: $e');
      _scheduleReconnect();
      return null;
    }
  }

  void _scheduleReconnect() {
    // Never reconnect while backgrounded: the background timer handles that path.
    if (!_foregrounded) return;
    _reconnectTimer?.cancel();
    final delay = Duration(seconds: _backoffSeconds);
    _backoffSeconds = (_backoffSeconds * 2).clamp(1, _maxBackoff);
    _reconnectTimer = Timer(delay, () {
      if (ref.mounted) {
        state = const AsyncValue.loading();
        ref.invalidateSelf();
      }
    });
  }

  /// Call when the app comes to the foreground. A connection can die silently
  /// while the app sits in the background (the phone suspends the socket, the
  /// network changes); probe it and reconnect at once if it does not answer,
  /// instead of letting the next screen's request wait out its deadline.
  Future<void> onForegrounded() async {
    _foregrounded = true;
    _backgroundTimer?.cancel();
    _backgroundTimer = null;
    final mux = _mux;
    if (mux == null) {
      if (state.value == null && !state.isLoading) _reconnectNow();
      return;
    }
    // Count any real reply (even 'forbidden') as proof the link is alive;
    // only a timeout or thrown error means the connection is dead.
    final alive = await mux
        .call('info')
        .timeout(probeTimeout, onTimeout: () => const {})
        .then((r) => r.containsKey('id'), onError: (_) => false);
    if (!alive && identical(mux, _mux) && ref.mounted) {
      _mux = null;
      mux.close().ignore();
      _reconnectNow();
    }
  }

  void _reconnectNow() {
    _reconnectTimer?.cancel();
    _backoffSeconds = 1;
    state = const AsyncValue.loading();
    ref.invalidateSelf();
  }

  void onBackgrounded() {
    _foregrounded = false;
    // Cancel any pending reconnect so a backoff timer that fires while
    // backgrounded cannot open a new connection (battery + hostd routing).
    _reconnectTimer?.cancel();
    _backgroundTimer?.cancel();
    _backgroundTimer = Timer(_backgroundGrace, () {
      _mux?.close();
      _mux = null;
      if (ref.mounted) {
        state = const AsyncValue.data(null);
      }
    });
  }

  void _cleanup() {
    _reconnectTimer?.cancel();
    // Do NOT cancel _backgroundTimer here: _cleanup runs on every invalidateSelf
    // rebuild. Cancelling it would leave an in-background mux open forever.
    // The timer fires harmlessly after _mux is set to null by _cleanup.
    _mux?.close();
    _mux = null;
  }
}
