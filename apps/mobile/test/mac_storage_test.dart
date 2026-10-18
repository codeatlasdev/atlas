import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  // The first pairing saved into an empty store and threw "Cannot add to an
  // unmodifiable list" after the Mac had already accepted the phone.
  test(
    'first paired Mac is saved into an empty store, then updated in place',
    () async {
      FlutterSecureStorage.setMockInitialValues({});
      final s = MacStorage();
      await s.upsertDevice(const MacDevice(nodeId: 'a', name: 'Mac'));
      await s.upsertDevice(const MacDevice(nodeId: 'a', name: 'Mac 2'));
      await s.upsertDevice(const MacDevice(nodeId: 'b', name: 'Other'));
      final all = await s.loadDevices();
      expect(all.map((d) => (d.nodeId, d.name)), [
        ('a', 'Mac 2'),
        ('b', 'Other'),
      ]);
      await s.removeDevice('a');
      expect((await s.loadDevices()).map((d) => d.nodeId), ['b']);
    },
  );
}
