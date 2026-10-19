import 'dart:async';

import 'package:flutter/foundation.dart';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/demo/demo_transport.dart';
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
) async => isDemo(device)
    ? demoTransport()
    : await FrbTransport.connect(
        secretKeyHex: secretKeyHex,
        nodeId: device.nodeId,
        addrs: device.addrs,
        relay: device.relay,
      );

/// Overridable transport factory provider (override in tests).
final transportFactoryProvider = Provider<TransportFactory>(
  (ref) => _defaultTransportFactory,
);

/// Provides the [MacStorage] singleton.
final macStorageProvider = Provider<MacStorage>((ref) => MacStorage());

/// Manages the [Multiplexer] lifecycle for a single paired Mac.
///
/// - Reconnects with exponential backoff (1 s → 30 s) while the app is foregrounded.
/// - Disconnects 30 s after the app backgrounds.
/// - Family key: [MacDevice] (freezed equality keeps the provider stable).
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

  /// Why the last connection attempt failed (shown by the pairing screen).
  Object? lastError;
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
      // The demo computer lives in the app: it needs no identity, so no Keychain read.
      final secretKey = isDemo(_device)
          ? ''
          : await ref.read(macStorageProvider).loadSecretKey() ?? '';
      final transport = await ref.read(transportFactoryProvider)(
        _device,
        secretKey,
      );
      final mux = Multiplexer(transport);
      _mux = mux;
      _backoffSeconds = 1;

      // Reconnect when transport closes.
      mux.done.then((_) {
        if (_foregrounded && ref.mounted) {
          _scheduleReconnect();
        }
      });

      return mux;
    } catch (e) {
      lastError = e;
      debugPrint('atlas-mobile: connect to ${_device.nodeId} failed: $e');
      _scheduleReconnect();
      return null;
    }
  }

  void _scheduleReconnect() {
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

  /// Call when the app comes to the foreground.
  void onForegrounded() {
    _foregrounded = true;
    _backgroundTimer?.cancel();
    _backgroundTimer = null;
    // Reconnect if currently disconnected.
    if (state.value == null && !state.isLoading) {
      state = const AsyncValue.loading();
      ref.invalidateSelf();
    }
  }

  /// Call when the app goes to the background.
  void onBackgrounded() {
    _foregrounded = false;
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
    _backgroundTimer?.cancel();
    _mux?.close();
    _mux = null;
  }
}
