// Golden screenshots: CrewmateProfileScreen with 2 automations, and the editor sheet.
// Incident: profile broke after removing the old crewmate_get; automations section is new.
// Run with: cd apps/mobile && flutter test test/crewmate_profile_automations_test.dart
// To update goldens: flutter test --update-goldens test/crewmate_profile_automations_test.dart
import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/profile/crewmate_profile_screen.dart';
import 'package:atlas_mobile/features/crew/automation_editor_sheet.dart';

// ─── Fake transport: full NDJSON protocol ─────────────────────────────────────

/// Responds to NDJSON calls with configurable crewmate_list data.
class _CrewmateTransport implements Transport {
  _CrewmateTransport(this._crewmateList);
  final Map<String, Object?> _crewmateList;

  final _ctrl = StreamController<String>.broadcast();
  bool _open = true;

  @override
  Stream<String> get incomingLines => _ctrl.stream;

  @override
  Future<void> sendLine(String line) async {
    if (!_open) throw StateError('closed');
    final req = jsonDecode(line) as Map<String, dynamic>;
    final id = req['id'];
    final cmd = req['cmd'] as String? ?? '';
    final Object data = switch (cmd) {
      'crewmate_list' => _crewmateList,
      'crew_github_status' => {'installed': false},
      'info' => {
        'caps': <String>['crewmates'],
        'version': '1.0.0',
        'hostname': 'Test Mac',
      },
      _ => <String, Object?>{},
    };
    _ctrl.add(jsonEncode({'id': id, 'ok': true, 'data': data}));
  }

  @override
  Future<void> close() async {
    if (!_open) return;
    _open = false;
    await _ctrl.close();
  }
}

// ─── Fixture payloads ─────────────────────────────────────────────────────────

const _device = MacDevice(nodeId: 'test', name: 'Test Mac');
const _crewmateId = 'crewmate-reviewer';

final _crewmateListWith2Triggers = {
  'host_name': 'Test Mac',
  'crewmates': [
    {
      'id': _crewmateId,
      'name': 'Reviewer',
      'role': 'Reviews PRs in this project',
      'agent': 'claude',
      'approval': 'ask',
      'state': 'idle',
      'queued': 0,
      'running_task': null,
      'last': null,
      'pending_approval': null,
      'triggers': [
        {
          'id': 't-sched',
          'kind': {'type': 'schedule', 'cron': '0 9 * * 1-5'},
          'enabled': true,
          'prompt': 'Review open PRs',
          'tz': 'America/Sao_Paulo',
          'next_fire': 1700100000,
        },
        {
          'id': 't-interval',
          'kind': {'type': 'interval', 'every_secs': 3600},
          'enabled': false,
          'prompt': 'Check deployments',
        },
      ],
    },
  ],
};

final _crewmateListEmpty = {
  'host_name': 'Test Mac',
  'crewmates': [
    {
      'id': _crewmateId,
      'name': 'Reviewer',
      'role': 'Reviews PRs in this project',
      'agent': 'claude',
      'approval': 'ask',
      'state': 'idle',
      'queued': 0,
      'running_task': null,
      'last': null,
      'pending_approval': null,
      'triggers': <Object>[],
    },
  ],
};

// ─── Helpers ──────────────────────────────────────────────────────────────────

class _Storage extends MacStorage {
  _Storage() : super(storage: null);
  @override
  Future<List<MacDevice>> loadDevices() async => const [_device];
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

Widget _wrap(Widget child, Transport transport) => ProviderScope(
  overrides: [
    macStorageProvider.overrideWithValue(_Storage()),
    transportFactoryProvider.overrideWithValue((_, _) async => transport),
  ],
  child: MaterialApp(theme: atlasTheme(), home: child),
);

Future<void> _settle(WidgetTester t) async {
  for (var i = 0; i < 20; i++) {
    await t.pump(const Duration(milliseconds: 100));
  }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

void main() {
  const size = Size(393, 852); // iPhone 14 logical pixels

  testWidgets('profile with 2 automations lists both', (t) async {
    await t.binding.setSurfaceSize(size);
    final transport = _CrewmateTransport(_crewmateListWith2Triggers);
    await t.pumpWidget(
      _wrap(
        CrewmateProfileScreen(device: _device, crewmateId: _crewmateId),
        transport,
      ),
    );
    await _settle(t);

    // Automations section renders both trigger descriptions.
    expect(find.text('Automations'), findsOneWidget);
    expect(find.text('Weekdays at 09:00 (America/Sao_Paulo)'), findsOneWidget);
    expect(find.text('Every 1 hour'), findsOneWidget);
    expect(find.text('Add automation'), findsOneWidget);

    await t.pumpWidget(const SizedBox());
    await _settle(t);
    await transport.close();
  });

  testWidgets(
    'profile with 2 automations — golden',
    // The profile was redesigned (2026-11-12) and the stored golden was regenerated from the
    // new code before anyone approved the look, so it proves nothing yet. Regenerate it and
    // drop this skip once the redesign is approved on a device.
    skip: true,
    (t) async {
      await t.binding.setSurfaceSize(size);
      final transport = _CrewmateTransport(_crewmateListWith2Triggers);
      await t.pumpWidget(
        _wrap(
          CrewmateProfileScreen(device: _device, crewmateId: _crewmateId),
          transport,
        ),
      );
      await _settle(t);
      await expectLater(
        find.byType(CrewmateProfileScreen),
        matchesGoldenFile('goldens/crewmate_profile_with_automations.png'),
      );
      await t.pumpWidget(const SizedBox());
      await _settle(t);
      await transport.close();
    },
  );

  testWidgets('profile with no automations shows empty-state hint', (t) async {
    await t.binding.setSurfaceSize(size);
    final transport = _CrewmateTransport(_crewmateListEmpty);
    await t.pumpWidget(
      _wrap(
        CrewmateProfileScreen(device: _device, crewmateId: _crewmateId),
        transport,
      ),
    );
    await _settle(t);

    // Empty-state hint must name the crewmate.
    expect(
      find.textContaining('Or just ask Reviewer'),
      findsOneWidget,
      reason: 'empty-state hint must name the crewmate',
    );
    // Add button still present.
    expect(find.text('Add automation'), findsOneWidget);

    await t.pumpWidget(const SizedBox());
    await _settle(t);
    await transport.close();
  });

  testWidgets('automation editor sheet — golden', (t) async {
    await t.binding.setSurfaceSize(size);
    await t.pumpWidget(
      MaterialApp(
        theme: atlasTheme(),
        home: Builder(
          builder: (ctx) => Scaffold(
            body: Center(
              child: ElevatedButton(
                onPressed: () {
                  showAutomationEditor(ctx, crewmateName: 'Reviewer');
                },
                child: const Text('Open'),
              ),
            ),
          ),
        ),
      ),
    );

    await t.tap(find.text('Open'));
    await t.pumpAndSettle();

    expect(find.text('New automation'), findsOneWidget);
    expect(find.text('Schedule'), findsOneWidget);
    expect(find.text('Days'), findsOneWidget);

    await expectLater(
      find.byType(MaterialApp),
      matchesGoldenFile('goldens/automation_editor_sheet.png'),
    );

    await t.pumpWidget(const SizedBox());
    await _settle(t);
  });
}
