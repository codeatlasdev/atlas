import 'dart:async';
import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/features/pair/pair_screen.dart';

class _Storage extends MacStorage {
  _Storage() : super(storage: null);
  final saved = <MacDevice>[];
  @override
  Future<String?> loadSecretKey() async => 'ab' * 32;
  @override
  Future<void> upsertDevice(MacDevice d) async => saved.add(d);
}

/// A computer that accepts the pairing code, after a network round trip.
class _Accepts implements Transport {
  final _lines = StreamController<String>.broadcast();
  @override
  Stream<String> get incomingLines => _lines.stream;
  @override
  Future<void> close() async {}
  @override
  Future<void> sendLine(String line) async {
    final id = (jsonDecode(line) as Map)['id'];
    // Answers a moment later, like a real computer across the network.
    Timer(
      const Duration(milliseconds: 50),
      () => _lines.add(jsonEncode({'id': id, 'ok': true, 'data': {}})),
    );
  }
}

void main() {
  // Guards a pairing lost by leaving the screen: the notifier is auto-disposed with
  // the screen, and writing its state after the awaits threw before the paired
  // computer was saved (found by the on-simulator end-to-end test).
  test(
    'a pairing the computer accepted is saved even with nobody watching',
    () async {
      final storage = _Storage();
      final c = ProviderContainer(
        overrides: [
          macStorageProvider.overrideWithValue(storage),
          transportFactoryProvider.overrideWithValue(
            (_, _) async => _Accepts(),
          ),
        ],
      );
      final pairing = c
          .read(pairNotifierProvider.notifier)
          .pair('atlas-mobile://pair?node=n1&code=123456&name=Mac');
      // Everything that held the notifier goes away while the computer is answering.
      await Future<void>.delayed(Duration.zero);
      c.dispose();
      await pairing;
      expect(storage.saved.map((d) => d.nodeId), ['n1']);
    },
  );
}
