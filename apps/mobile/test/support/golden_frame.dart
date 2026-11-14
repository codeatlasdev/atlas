/// The frame every crew golden is drawn in: the app theme, a fake storage and no transport.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';

class _FakeStorage extends MacStorage {
  _FakeStorage() : super(storage: null);
  @override
  Future<List<MacDevice>> loadDevices() async => const [];
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

Widget goldenFrame(Widget child) => ProviderScope(
  overrides: [
    macStorageProvider.overrideWithValue(_FakeStorage()),
    transportFactoryProvider.overrideWith((_) => throw UnimplementedError()),
  ],
  child: MaterialApp(
    theme: atlasTheme(),
    home: Scaffold(body: child),
    debugShowCheckedModeBanner: false,
  ),
);
