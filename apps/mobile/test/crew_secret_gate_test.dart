import 'dart:convert';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/auth/device_auth.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/protocol/crew_task.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/features/crew/chat/action_card.dart';

import 'support/demo_harness.dart';
import 'support/fake_permissions.dart';

// Secrets from the phone (PhoneCrewAudit gap 7): a crewmate that needs a value asks with a `provide_secret`
// card; the owner types it on the phone. Decision: allowed, Face ID first, and nothing leaves the phone
// until they pass it. The transport records every line the app really sends.

/// The demo computer's transport, remembering what the app sent it.
class _Recording implements Transport {
  _Recording(this.inner);
  final Transport inner;
  final sent = <Map<String, dynamic>>[];
  @override
  Stream<String> get incomingLines => inner.incomingLines;
  @override
  Future<void> sendLine(String line) {
    sent.add(jsonDecode(line) as Map<String, dynamic>);
    return inner.sendLine(line);
  }

  @override
  Future<void> close() => inner.close();

  List<Map<String, dynamic>> answers() => [
    for (final m in sent)
      if (m['cmd'] == 'crewmate_action_answer') m,
  ];
}

const _mac = MacDevice(nodeId: 'demo', name: 'Demo MacBook');

/// The chat screen keeps the device's connection open while its cards are on screen; so does this.
Widget _withConnection(Widget card) => Consumer(
  builder: (context, ref, _) {
    ref.watch(connectionNotifierProvider(_mac));
    return card;
  },
);

CrewmateTask _secretTask() => CrewmateTask.fromJson({
  'id': 'task-1',
  'ts': 1700000000,
  'source': 'mobile',
  'prompt': 'deploy',
  'state': 'waiting_action',
  'action': {
    'kind': 'provide_secret',
    'name': 'API_KEY',
    'label': 'API Key',
    'reason': 'needed for the API',
  },
});

Future<_Recording> _pump(
  WidgetTester t,
  DeviceAuth auth, [
  FakePermissions? perms,
]) async {
  final rec = _Recording(demoTransport());
  await t.pumpWidget(
    demoWrap(
      Scaffold(
        body: SingleChildScrollView(
          child: _withConnection(
            TaskActionCard(
              task: _secretTask(),
              device: _mac,
              crewmateId: 'crewmate-1',
            ),
          ),
        ),
      ),
      [_mac],
      overrides: [
        deviceAuthProvider.overrideWithValue(auth),
        permissionsProvider.overrideWithValue(perms ?? FakePermissions()),
      ],
      transport: () => rec,
    ),
  );
  await settle(t);
  return rec;
}

Future<void> _typeAndSubmit(WidgetTester t, String value) async {
  await t.enterText(find.byType(TextField), value);
  await t.tap(find.text('Submit'));
  await settle(t);
}

void main() {
  testWidgets(
    'a refused Face ID sends nothing: the secret never leaves the phone',
    (t) async {
      var asked = <String>[];
      final rec = await _pump(t, (reason) async {
        asked.add(reason);
        return false;
      });
      await _typeAndSubmit(t, 'ghp_secret');
      expect(asked, hasLength(1), reason: 'the gate must ask');
      expect(asked.single, contains('API_KEY'), reason: 'say which secret');
      expect(rec.answers(), isEmpty, reason: 'sent despite a refused Face ID');
      expect(find.textContaining('Face ID'), findsWidgets);
    },
  );

  testWidgets('a device that cannot ask (no passcode) sends nothing either', (
    t,
  ) async {
    final rec = await _pump(
      t,
      (_) async => throw StateError('no passcode set'),
    );
    await _typeAndSubmit(t, 'ghp_secret');
    expect(rec.answers(), isEmpty);
    expect(find.textContaining('no passcode set'), findsOneWidget);
    expect(find.textContaining('was not sent'), findsOneWidget);
  });

  testWidgets('the first secret explains Face ID before the lock is asked', (
    t,
  ) async {
    var asked = 0;
    final rec = await _pump(
      t,
      (_) async {
        asked++;
        return true;
      },
      FakePermissions(
        states: {AtlasPermission.faceId: PermissionState.notAsked},
      ),
    );
    await t.enterText(find.byType(TextField), 'ghp_secret');
    await t.tap(find.text('Submit'));
    await settle(t);
    expect(find.text('Confirm it is you'), findsOneWidget);
    expect(asked, 0);
    expect(rec.answers(), isEmpty);

    await t.tap(find.text('Continue'));
    await settle(t);
    expect(asked, 1);
    expect(rec.answers(), hasLength(1));
  });

  testWidgets('a phone with no lock set up says so and sends nothing', (
    t,
  ) async {
    var asked = 0;
    final rec = await _pump(
      t,
      (_) async {
        asked++;
        return true;
      },
      FakePermissions(
        states: {AtlasPermission.faceId: PermissionState.unavailable},
      ),
    );
    await _typeAndSubmit(t, 'ghp_secret');
    expect(asked, 0);
    expect(rec.answers(), isEmpty);
    expect(find.textContaining('no lock set up'), findsOneWidget);
    expect(find.textContaining('was not sent'), findsOneWidget);
  });

  testWidgets(
    'after Face ID the value goes out once, as crewmate_action_answer',
    (t) async {
      final rec = await _pump(t, (_) async => true);
      await _typeAndSubmit(t, 'ghp_secret');
      final answers = rec.answers();
      expect(answers, hasLength(1));
      expect(answers.single['args'], containsPair('value', 'ghp_secret'));
      expect(answers.single['args'], containsPair('task', 'task-1'));
    },
  );

  testWidgets('a plain input card (not a secret) needs no Face ID', (t) async {
    var asked = 0;
    final rec = _Recording(demoTransport());
    final task = CrewmateTask.fromJson({
      'id': 'task-2',
      'ts': 1700000000,
      'source': 'mobile',
      'prompt': 'p',
      'state': 'waiting_action',
      'action': {
        'kind': 'input',
        'label': 'Branch name',
        'reason': 'which one',
      },
    });
    await t.pumpWidget(
      demoWrap(
        Scaffold(
          body: SingleChildScrollView(
            child: _withConnection(
              TaskActionCard(
                task: task,
                device: _mac,
                crewmateId: 'crewmate-1',
              ),
            ),
          ),
        ),
        [_mac],
        overrides: [
          deviceAuthProvider.overrideWithValue((_) async {
            asked++;
            return true;
          }),
          permissionsProvider.overrideWithValue(FakePermissions()),
        ],
        transport: () => rec,
      ),
    );
    await settle(t);
    await _typeAndSubmit(t, 'main');
    expect(asked, 0);
    expect(rec.answers(), hasLength(1));
  });
}
