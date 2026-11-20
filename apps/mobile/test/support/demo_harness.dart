/// Pumps a screen against the demo computer, with the given paired devices in storage.
library;

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/misc.dart' show Override;
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/backdrop.dart';
import 'package:atlas_mobile/core/voice/platform_speech.dart';
import 'package:atlas_mobile/core/voice/read_aloud.dart';

import 'fake_speech.dart';

class DemoStorage extends MacStorage {
  DemoStorage(this.devices) : super(storage: null);
  final List<MacDevice> devices;
  @override
  Future<List<MacDevice>> loadDevices() async => devices;
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

/// [child] in the app theme; every device connects to [demoTransport], or to [transport] when given (a wrapper
/// of it that misbehaves on purpose). [overrides] join the one `ProviderScope`: a second, outer scope would not
/// reach the providers this one overrides.
///
/// [readAloudStoreProvider] and [speechSynthesizerProvider] are injected as in-memory fakes so tests never touch
/// SharedPreferences or the platform channel — unless the caller already overrides them.
Widget demoWrap(
  Widget child,
  List<MacDevice> devices, {
  List<Override> overrides = const [],
  Transport Function()? transport,
}) {
  final readAloudOverride = readAloudStoreProvider.overrideWithValue(
    MemoryReadAloudStore(),
  );
  final speechOverride = speechSynthesizerProvider.overrideWithValue(
    FakeSpeech(),
  );
  // A test that passes its own read-aloud or speech override must win: if it already provides one,
  // skip our default to avoid "tried to override a provider twice" (Riverpod 3 forbids it).
  final overrideSet = overrides.map(_overrideKey).toSet();
  return ProviderScope(
    overrides: [
      macStorageProvider.overrideWithValue(DemoStorage(devices)),
      transportFactoryProvider.overrideWithValue(
        (_, _) async => (transport ?? demoTransport)(),
      ),
      if (!overrideSet.contains(_overrideKey(readAloudOverride))) readAloudOverride,
      if (!overrideSet.contains(_overrideKey(speechOverride))) speechOverride,
      ...overrides,
    ],
    child: MaterialApp(theme: atlasTheme(), builder: atlasGround, home: child),
  );
}

/// A stable key that identifies which provider an [Override] targets. Two overrides targeting
/// the same provider share the same key; this never inspects values.
String _overrideKey(Override o) {
  // `Override.toString()` on Riverpod 3 includes the provider's debug name and id,
  // which is unique per provider singleton. Same call site → same string.
  return o.toString().split('(').first;
}

/// Lets timers and demo pushes run for three seconds of fake time.
Future<void> settle(WidgetTester t) async {
  for (var i = 0; i < 30; i++) {
    await t.pump(const Duration(milliseconds: 100));
  }
}
