import 'dart:async';
import 'dart:convert';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/features/crew/automation_editor_sheet.dart';
import 'package:atlas_mobile/features/crew/list/crew_screen.dart';
import 'package:atlas_mobile/features/crew/profile/crewmate_profile_screen.dart';
import 'package:atlas_mobile/features/inbox/inbox.dart';

import 'support/demo_harness.dart' show DemoStorage;
import 'support/layout_harness.dart';

// The tap-target and label guidelines (`expectNoLayoutProblems`, tapTargets on) for the screens
// layout_test.dart does not reach: the Crew tab (crewmates, and a computer that failed next to one that
// loaded), a crewmate's profile, the Inbox with items, and the automation editor sheet. Every phone size
// at text scale 1.0 and 2.0, Android 48 dp, iOS 44 pt and a label on everything tappable.

const _studio = MacDevice(nodeId: 'studio', name: 'Studio');

/// A computer whose `crewmate_list` always fails with `forbidden`: its card on the Crew tab is the
/// per-computer error card (#393), next to the demo computer's crewmates.
class _Forbidden implements Transport {
  final _in = StreamController<String>.broadcast();
  @override
  Stream<String> get incomingLines => _in.stream;
  @override
  Future<void> close() => _in.close();
  @override
  Future<void> sendLine(String line) async {
    final m = jsonDecode(line) as Map<String, dynamic>;
    final body = switch (m['cmd']) {
      'info' => {
        'ok': true,
        'data': {'proto': kAppProto},
      },
      'crew_servers' => {
        'ok': true,
        'data': {'servers': []},
      },
      _ => {'ok': false, 'error': 'forbidden'},
    };
    scheduleMicrotask(() {
      if (!_in.isClosed) _in.add(jsonEncode({'id': m['id'], ...body}));
    });
  }
}

/// The phone with [devices] paired; each dials [computerFor] (the demo by default). Above the app, so
/// the sheets a screen opens see it too.
AppWrapper _phone(
  List<MacDevice> devices,
  Transport Function(MacDevice) computerFor,
  List<Transport> dialled,
) =>
    (app) => ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(DemoStorage(devices)),
        transportFactoryProvider.overrideWithValue((d, _) async {
          final t = computerFor(d);
          dialled.add(t);
          return t;
        }),
      ],
      child: app,
    );

Future<void> _check(
  WidgetTester t,
  String label,
  Widget Function(BuildContext) screen, {
  List<MacDevice> devices = const [demoDevice],
  Transport Function(MacDevice)? computerFor,
  Future<void> Function(WidgetTester)? prepare,
}) async {
  final dialled = <Transport>[];
  try {
    await expectNoLayoutProblems(
      t,
      screen,
      wrap: _phone(devices, computerFor ?? (_) => demoTransport(), dialled),
      label: label,
      prepare: prepare,
      settleFor: const Duration(milliseconds: 3000),
    );
  } finally {
    // Not awaited: a stream close completes on the real event loop, which a fake-async test never
    // gives it. Its timers are cancelled synchronously.
    for (final c in dialled) {
      unawaited(c.close());
    }
    await t.pump();
  }
}

void main() {
  group('crew tab', () {
    testWidgets('with crewmates', (t) async {
      await _check(t, 'crew tab', (_) => const CrewScreen());
    });

    testWidgets('a computer that failed next to one that loaded', (t) async {
      await _check(
        t,
        'crew tab, error card',
        (_) => const CrewScreen(),
        devices: const [demoDevice, _studio],
        computerFor: (d) => d == _studio ? _Forbidden() : demoTransport(),
      );
    });
  });

  testWidgets('crewmate profile', (t) async {
    await _check(
      t,
      'profile',
      (_) => const CrewmateProfileScreen(
        device: demoDevice,
        crewmateId: 'crewmate-1',
      ),
    );
  });

  testWidgets('profile of the crewmate that needs you', (t) async {
    await _check(
      t,
      'profile, needs you',
      (_) => const CrewmateProfileScreen(
        device: demoDevice,
        crewmateId: 'crewmate-2',
      ),
    );
  });

  // Every attention row is a 48 dp target: "Nightly needs you · Crew" and "payments failed · orbit-shop" (the
  // extra rows, `AttentionMore`) were 46 dp at every width.
  testWidgets('inbox with items', (t) async {
    await _check(t, 'inbox', (_) => const InboxScreen());
  });

  // Every trigger kind builds its own fields; the kind chips switch between them.
  for (final kind in const [
    'Schedule',
    'Interval',
    'GitHub',
    'File',
    'After another crewmate',
    'Service',
  ]) {
    testWidgets('automation editor sheet, $kind', (t) async {
      await _check(
        t,
        'automation editor, $kind',
        (context) => Scaffold(
          body: Center(
            child: ElevatedButton(
              onPressed: () =>
                  showAutomationEditor(context, crewmateName: 'Reviewer'),
              child: const Text('Open'),
            ),
          ),
        ),
        prepare: (t) async {
          await t.tap(find.text('Open'));
          await settle(t, 800);
          await t.tap(find.text(kind));
          await settle(t, 400);
        },
      );
    });
  }
}
