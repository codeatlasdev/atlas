import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/push/push_provider.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/settings/settings_screen.dart';

// Settings shows, in one line, why notifications work or do not: the user could never tell before
// (every failure on the way was a silent null/false).

class _Storage extends MacStorage {
  _Storage() : super(storage: null);
  @override
  Future<List<MacDevice>> loadDevices() async => const [];
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

Future<void> _pump(
  WidgetTester tester,
  Future<String?> Function() status,
) async {
  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage()),
        pushRegistrationProvider.overrideWith((ref) => status()),
      ],
      child: MaterialApp(theme: atlasTheme(), home: const SettingsScreen()),
    ),
  );
  await tester.pump();
  await tester.pump();
}

void main() {
  testWidgets('shows the reason notifications are not working', (tester) async {
    await _pump(
      tester,
      () async => 'Notifications are off: the push relay is down (HTTP 503)',
    );
    expect(find.text('Notifications'), findsOneWidget);
    expect(
      find.text('Notifications are off: the push relay is down (HTTP 503)'),
      findsOneWidget,
    );
  });

  testWidgets('shows that they are on', (tester) async {
    await _pump(tester, () async => 'Notifications are on');
    expect(find.text('Notifications are on'), findsOneWidget);
  });

  // No Firebase config (a local build): nothing to report, and no empty row either.
  testWidgets('shows no row when there is nothing to say', (tester) async {
    await _pump(tester, () async => null);
    expect(find.text('Notifications'), findsNothing);
  });
}
