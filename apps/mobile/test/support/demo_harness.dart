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
Widget demoWrap(
  Widget child,
  List<MacDevice> devices, {
  List<Override> overrides = const [],
  Transport Function()? transport,
}) => ProviderScope(
  overrides: [
    macStorageProvider.overrideWithValue(DemoStorage(devices)),
    transportFactoryProvider.overrideWithValue(
      (_, _) async => (transport ?? demoTransport)(),
    ),
    ...overrides,
  ],
  child: MaterialApp(theme: atlasTheme(), builder: atlasGround, home: child),
);

/// Lets timers and demo pushes run for three seconds of fake time.
Future<void> settle(WidgetTester t) async {
  for (var i = 0; i < 30; i++) {
    await t.pump(const Duration(milliseconds: 100));
  }
}
