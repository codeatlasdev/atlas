import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/features/inbox/inbox.dart';
import 'package:atlas_mobile/features/projects/projects_provider.dart';

// A screen reader finds a row by its role and name. A bare InkWell/GestureDetector row has a tap
// action but no "button" role, and merges every Text inside it into one long label (or, under
// excludeSemantics, none). Each row below must be a button named for what it opens.

class _Storage extends MacStorage {
  _Storage(this.devices) : super(storage: null);
  final List<MacDevice> devices;
  @override
  Future<List<MacDevice>> loadDevices() async => devices;
  @override
  Future<String?> loadSecretKey() async => 'ab';
  @override
  Future<DateTime?> loadInboxSeen() async => null;
  @override
  Future<void> saveInboxSeen(DateTime at) async {}
}

/// A paired Mac that relays a crew server with one crewmate that just finished (as
/// inbox_recent_test does), so the Inbox has a Recent row.
class _Mac implements Transport {
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
            'ts':
                DateTime.now()
                    .subtract(const Duration(minutes: 3))
                    .millisecondsSinceEpoch ~/
                1000,
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

Future<void> _settle(WidgetTester t, [int n = 30]) async {
  for (var i = 0; i < n; i++) {
    await t.pump(const Duration(milliseconds: 100));
  }
}

/// The semantics of the one node that is a button and whose label contains [name].
SemanticsNode _button(WidgetTester t, Pattern name) {
  final nodes = <SemanticsNode>[];
  void walk(SemanticsNode n) {
    final d = n.getSemanticsData();
    if (d.flagsCollection.isButton && d.label.contains(name)) nodes.add(n);
    n.visitChildren((c) {
      walk(c);
      return true;
    });
  }

  walk(t.binding.renderViews.first.owner!.semanticsOwner!.rootSemanticsNode!);
  expect(
    nodes,
    hasLength(1),
    reason: 'exactly one button should be named like "$name"',
  );
  return nodes.single;
}

void main() {
  testWidgets('an Inbox activity row is a button named for the event', (
    t,
  ) async {
    final h = t.ensureSemantics();
    await t.binding.setSurfaceSize(const Size(393, 852));
    addTearDown(() => t.binding.setSurfaceSize(null));
    await t.pumpWidget(
      ProviderScope(
        overrides: [
          macStorageProvider.overrideWithValue(
            _Storage(const [MacDevice(nodeId: 'mac', name: 'Mac')]),
          ),
          transportFactoryProvider.overrideWithValue((_, _) async => _Mac()),
        ],
        child: MaterialApp(theme: atlasTheme(), home: const InboxScreen()),
      ),
    );
    await _settle(t);

    final row = _button(t, 'carlos finished');
    final d = row.getSemanticsData();
    expect(d.hasAction(SemanticsAction.tap), isTrue);
    expect(
      d.label,
      contains('PR merged'),
      reason: 'the preview is part of the name',
    );

    await t.pumpWidget(const SizedBox());
    await _settle(t, 10);
    h.dispose();
  });

  testWidgets(
    'a Home project row is a button named for the project and its state',
    (t) async {
      final h = t.ensureSemantics();
      const device = MacDevice(nodeId: 'n1', name: 'My Mac');
      final project = ProjectInfo(
        id: 'a',
        root: '/Users/me/shop',
        name: 'shop',
        running: true,
        services: const [
          ServiceInfo(name: 'api', state: 'running'),
          ServiceInfo(name: 'db', state: 'failed'),
        ],
      );
      await t.binding.setSurfaceSize(const Size(393, 852));
      addTearDown(() => t.binding.setSurfaceSize(null));
      await t.pumpWidget(
        ProviderScope(
          overrides: [
            macStorageProvider.overrideWithValue(_Storage(const [device])),
            transportFactoryProvider.overrideWithValue(
              (_, _) async => demoTransport(),
            ),
            projectListProvider(device).overrideWith((ref) async => [project]),
          ],
          child: MaterialApp(theme: atlasTheme(), home: const HomeScreen()),
        ),
      );
      await _settle(t);

      final row = _button(t, 'shop');
      final d = row.getSemanticsData();
      expect(d.hasAction(SemanticsAction.tap), isTrue);
      expect(
        d.label,
        contains('1 down'),
        reason: 'the state is part of the name',
      );

      await t.pumpWidget(const SizedBox());
      await _settle(t, 10);
      h.dispose();
    },
  );
}
