// Regression: the Crew tab showed "No crew yet" (and a Recruit-less empty screen) for as long as the
// computers' crew lists were still loading, because it read `value ?? []`. A slow link looked like a
// crew that did not exist. While anything is loading the tab shows a loader; "No crew yet" is for a
// settled, empty answer.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/computer_state.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/features/crew/chat/crewmate_chat_screen.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/list/crew_screen.dart';

import 'support/demo_harness.dart';

const _mac = MacDevice(nodeId: 'mac-1', name: 'Mac');

CrewListResult _empty() =>
    (isCrewServer: true, hostName: 'Mac', crewmates: const [], paused: false);

class _Pending extends CrewNotifier {
  _Pending(super.device, this.gate);
  final Completer<CrewListResult> gate;
  @override
  Future<CrewListResult> build() => gate.future;
}

void main() {
  testWidgets(
    'crew tab shows a loader, not "No crew yet", while the list loads',
    (t) async {
      await t.binding.setSurfaceSize(const Size(393, 852));
      final gate = Completer<CrewListResult>();
      await t.pumpWidget(
        demoWrap(
          const CrewScreen(),
          [_mac],
          overrides: [
            crewDevicesProvider.overrideWith((_) async => [_mac]),
            crewProvider.overrideWith2((_) => _Pending(_mac, gate)),
          ],
        ),
      );
      await t.pump();
      await t.pump(const Duration(milliseconds: 100));

      expect(find.text('No crew yet'), findsNothing);
      expect(find.byType(CircularProgressIndicator), findsOneWidget);

      // The answer arrives and it really is empty: now the empty state is true.
      gate.complete(_empty());
      await t.pump();
      await t.pump(const Duration(milliseconds: 100));
      expect(find.byType(CircularProgressIndicator), findsNothing);
      expect(find.text('No crew yet'), findsOneWidget);

      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'crew tab shows a loader while the computers themselves are listed',
    (t) async {
      await t.binding.setSurfaceSize(const Size(393, 852));
      final devices = Completer<List<MacDevice>>();
      await t.pumpWidget(
        demoWrap(
          const CrewScreen(),
          [_mac],
          overrides: [crewDevicesProvider.overrideWith((_) => devices.future)],
        ),
      );
      await t.pump();

      expect(find.text('No crew yet'), findsNothing);
      expect(find.byType(CircularProgressIndicator), findsOneWidget);

      devices.complete(const []);
      await t.pump();
      await t.pump(const Duration(milliseconds: 100));
      expect(find.text('No crew yet'), findsOneWidget);

      await t.pumpWidget(const SizedBox());
    },
  );

  // Same class as the tab: the thread read `value ?? []`, so "Say hi" showed while the tasks loaded.
  testWidgets(
    'crewmate chat shows a loader, not "Say hi", while its tasks load',
    (t) async {
      await t.binding.setSurfaceSize(const Size(393, 852));
      final gate = Completer<List<CrewmateTask>>();
      await t.pumpWidget(
        demoWrap(
          const CrewmateChatScreen(device: _mac, crewmateId: 'c1'),
          [_mac],
          overrides: [
            crewmateTasksProvider.overrideWith((_, _) => gate.future),
          ],
        ),
      );
      await t.pump();
      await t.pump(const Duration(milliseconds: 100));

      expect(find.textContaining('Say hi'), findsNothing);
      expect(find.byType(CircularProgressIndicator), findsOneWidget);

      gate.complete(const []);
      await t.pump();
      await t.pump(const Duration(milliseconds: 100));
      expect(find.byType(CircularProgressIndicator), findsNothing);

      await t.pumpWidget(const SizedBox());
    },
  );

  // One strip, one sentence: the chat says its computer is offline with the shared message.
  testWidgets('offline strip shows the one sentence, only while offline', (
    t,
  ) async {
    final c = ProviderContainer(
      overrides: [
        macStorageProvider.overrideWithValue(DemoStorage([_mac])),
        transportFactoryProvider.overrideWithValue(
          (_, _) async => throw Exception('refused'),
        ),
      ],
    );
    await t.pumpWidget(
      UncontrolledProviderScope(
        container: c,
        child: MaterialApp(
          theme: atlasTheme(),
          home: Scaffold(
            body: OfflineStrip(device: _mac, suffix: ' · history shown'),
          ),
        ),
      ),
    );
    await t.pump();
    await t.pump(const Duration(milliseconds: 100));
    expect(find.text('Mac is offline · history shown'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
    // The reconnect backoff timer lives with the container: dispose it inside the test.
    c.dispose();
  });
}
