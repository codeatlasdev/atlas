// Guards 2026-11-12: the Inbox said "All clear" while carlos, on a crew server behind
// the paired Mac, had finished a task minutes before.
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
import 'package:atlas_mobile/features/inbox/inbox.dart';

/// A paired Mac that is no crew server itself and relays one, like hostd does.
class _Mac implements Transport {
  _Mac(this.finishedAt);
  final DateTime finishedAt;
  final _lines = StreamController<String>.broadcast();

  @override
  Stream<String> get incomingLines => _lines.stream;

  Object? _answer(String cmd, Map<String, dynamic> args) => switch (cmd) {
    'crew_servers' => {
      'servers': [
        {'id': 'atlas-crew@server', 'name': 'server', 'state': 'connected'},
      ],
    },
    'crew_call' when args['cmd'] == 'crewmate_list' => {
      'crewmates': [
        {
          'id': 'b1',
          'name': 'carlos',
          'state': 'idle',
          'last': {
            'text': 'PR merged',
            'state': 'done',
            'ts': finishedAt.millisecondsSinceEpoch ~/ 1000,
          },
        },
      ],
    },
    'project_list' => {'projects': []},
    'agent_status_all' => {'agents': []},
    _ => null,
  };

  @override
  Future<void> sendLine(String line) async {
    final req = jsonDecode(line) as Map<String, dynamic>;
    final data = _answer(
      req['cmd'] as String,
      (req['args'] as Map?)?.cast<String, dynamic>() ?? const {},
    );
    scheduleMicrotask(() {
      _lines.add(
        jsonEncode(
          data == null
              ? {'id': req['id'], 'ok': false, 'error': 'not a crew server'}
              : {'id': req['id'], 'ok': true, 'data': data},
        ),
      );
    });
  }

  @override
  Future<void> close() async => _lines.close();
}

class _Storage extends MacStorage {
  _Storage() : super(storage: null);
  @override
  Future<List<MacDevice>> loadDevices() async => const [
    MacDevice(nodeId: 'mac', name: 'Mac'),
  ];
  @override
  Future<String?> loadSecretKey() async => 'ab';
  @override
  Future<DateTime?> loadInboxSeen() async => null;
  @override
  Future<void> saveInboxSeen(DateTime at) async {}
}

void main() {
  test('a crewmate on a crew server that just finished is recent', () async {
    final c = ProviderContainer(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage()),
        transportFactoryProvider.overrideWithValue(
          (_, _) async =>
              _Mac(DateTime.now().subtract(const Duration(minutes: 3))),
        ),
      ],
    );
    addTearDown(c.dispose);
    final sub = c.listen(recentActivityProvider, (_, _) {});
    List<Activity> recent = const [];
    for (var i = 0; i < 50 && recent.isEmpty; i++) {
      await Future<void>.delayed(const Duration(milliseconds: 20));
      recent = c.read(recentActivityProvider);
    }
    sub.close();
    expect(recent, hasLength(1));
    expect((recent.single as CrewActivity).crewmate.name, 'carlos');
  });

  testWidgets('the Inbox screen lists it under Recent', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    await t.pumpWidget(
      ProviderScope(
        overrides: [
          macStorageProvider.overrideWithValue(_Storage()),
          transportFactoryProvider.overrideWithValue(
            (_, _) async =>
                _Mac(DateTime.now().subtract(const Duration(minutes: 3))),
          ),
        ],
        child: MaterialApp(theme: atlasTheme(), home: const InboxScreen()),
      ),
    );
    for (var i = 0; i < 30; i++) {
      await t.pump(const Duration(milliseconds: 100));
    }
    expect(find.text('All clear'), findsNothing);
    expect(find.text('carlos finished'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
    for (var i = 0; i < 10; i++) {
      await t.pump(const Duration(milliseconds: 100));
    }
  });
}
