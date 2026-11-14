/// Pumps a screen against the demo computer, with the given paired devices in storage.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';

class DemoStorage extends MacStorage {
  DemoStorage(this.devices) : super(storage: null);
  final List<MacDevice> devices;
  @override
  Future<List<MacDevice>> loadDevices() async => devices;
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

/// [child] in the app theme; every device connects to [demoTransport].
Widget demoWrap(Widget child, List<MacDevice> devices) => ProviderScope(
  overrides: [
    macStorageProvider.overrideWithValue(DemoStorage(devices)),
    transportFactoryProvider.overrideWithValue((_, _) async => demoTransport()),
  ],
  child: MaterialApp(theme: atlasTheme(), home: child),
);

/// Lets timers and demo pushes run for three seconds of fake time.
Future<void> settle(WidgetTester t) async {
  for (var i = 0; i < 30; i++) {
    await t.pump(const Duration(milliseconds: 100));
  }
}
