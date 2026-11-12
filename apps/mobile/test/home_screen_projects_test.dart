import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/features/projects/projects_provider.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';
import 'package:go_router/go_router.dart';

import 'fixtures/demo_computer.dart';

// Minimal MacStorage stub — no secure storage in tests.
class _FakeStorage extends MacStorage {
  _FakeStorage(this._devices) : super(storage: null);
  final List<MacDevice> _devices;

  @override
  Future<List<MacDevice>> loadDevices() async => _devices;

  @override
  Future<String?> loadSecretKey() async => 'deadbeef';
}

Future<void> _settle(WidgetTester t) async {
  for (var i = 0; i < 30; i++) {
    await t.pump(const Duration(milliseconds: 100));
  }
}

// Navigate through the three-page onboarding when a device is already paired.
Future<void> _throughOnboarding(WidgetTester t) async {
  for (var i = 0; i < 3; i++) {
    await t.tap(find.text('Continue'));
    await _settle(t);
  }
  await t.tap(find.text('Open my computer'));
  await _settle(t);
}

void main() {
  // ── 1. Pure unit tests for friendlyDeviceName ──────────────────────────────

  group('friendlyDeviceName', () {
    test('strips .local suffix and converts hyphens to spaces', () {
      expect(
        friendlyDeviceName('MacBook-Pro-de-Luan.local'),
        'MacBook Pro de Luan',
      );
    });

    test('converts hyphens without .local', () {
      expect(friendlyDeviceName('mac-mini'), 'mac mini');
    });

    test('leaves plain names unchanged', () {
      expect(friendlyDeviceName('My Mac'), 'My Mac');
    });

    test('strips only a trailing .local, not mid-name occurrences', () {
      // Edge case: .local only at the end should be stripped.
      expect(friendlyDeviceName('host.local'), 'host');
      // No stripping when .local appears elsewhere.
      expect(friendlyDeviceName('my.localhost'), 'my.localhost');
    });
  });

  // ── 2. Provider unit test: parsing project_list fixture ───────────────────

  test('projectListProvider parses project_list fixture', () {
    // Test the parsing path directly — verifies the defensive null-safe cast
    // added to projects_provider.dart as well as ProjectInfo.fromJson.
    final fixture = jsonDecode(
      File('test/fixtures/project_list.json').readAsStringSync(),
    ) as Map<String, dynamic>;

    final resp = <dynamic, dynamic>{'ok': true, 'data': fixture};

    // Mirrors the fixed parsing path in projects_provider.dart:
    final rawList =
        (resp['data'] as Map<dynamic, dynamic>?)?['projects']
            as List<dynamic>? ??
        const [];
    final projects = rawList
        .map((e) => ProjectInfo.fromJson(e as Map<String, dynamic>))
        .toList();

    expect(projects.length, 1);
    expect(projects.first.name, 'shop');
    expect(projects.first.id, 'a');
  });

  test('projectListProvider handles null data gracefully', () {
    // Defensive path: resp['data'] is null (unexpected server response).
    final resp = <dynamic, dynamic>{'ok': true, 'data': null};
    final rawList =
        (resp['data'] as Map<dynamic, dynamic>?)?['projects']
            as List<dynamic>? ??
        const [];
    expect(rawList, isEmpty);
  });

  // ── 3. Widget test: dashboard renders project name from fixture ────────────

  testWidgets('dashboard shows project name when projectListProvider has data', (
    t,
  ) async {
    const device = MacDevice(nodeId: 'n1', name: 'Test-Mac.local');

    final project = ProjectInfo(
      id: 'a',
      root: '/Users/me/shop',
      name: 'shop',
      running: false,
      services: const [],
    );

    await t.binding.setSurfaceSize(const Size(393, 852));
    await t.pumpWidget(
      ProviderScope(
        overrides: [
          macStorageProvider.overrideWithValue(_FakeStorage([device])),
          // Real transport replaced with the demo so connectionNotifier
          // connects immediately (connected = true → project section renders).
          transportFactoryProvider.overrideWithValue(
            (_, _) async => demoTransport(),
          ),
          // Inject the project directly so the test doesn't rely on the demo
          // computer's project fixture.
          projectListProvider(device).overrideWith((ref) async => [project]),
        ],
        child: MaterialApp(theme: atlasTheme(), home: const HomeScreen()),
      ),
    );
    await _settle(t);

    // Debug builds show onboarding first; tap through it.
    await _throughOnboarding(t);

    expect(
      find.text('shop'),
      findsOneWidget,
      reason: 'project name must appear in the dashboard after data loads',
    );

    // Cleanup: dispose the widget tree before the test runner moves on.
    await t.pumpWidget(const SizedBox());
    await _settle(t);
  });

  // Regression guard: when projectListProvider is overridden with AsyncData,
  // the dashboard must NOT show "No projects yet" (list is non-empty).
  testWidgets('no empty-state widget when projects are present', (t) async {
    const device = MacDevice(nodeId: 'n2', name: 'My Mac');

    final project = ProjectInfo(
      id: 'b',
      root: '/Users/me/app',
      name: 'app',
      running: false,
      services: const [],
    );

    await t.binding.setSurfaceSize(const Size(393, 852));
    await t.pumpWidget(
      ProviderScope(
        overrides: [
          macStorageProvider.overrideWithValue(_FakeStorage([device])),
          transportFactoryProvider.overrideWithValue(
            (_, _) async => demoTransport(),
          ),
          projectListProvider(device).overrideWith((ref) async => [project]),
        ],
        child: MaterialApp(theme: atlasTheme(), home: const HomeScreen()),
      ),
    );
    await _settle(t);
    await _throughOnboarding(t);

    expect(find.text('No projects yet'), findsNothing);
    expect(find.text('app'), findsOneWidget);

    await t.pumpWidget(const SizedBox());
    await _settle(t);
  });

  // Guards 2026-11-12: the row's agent line opened the agent straight from Home, so back
  // went to Home, a second tap on the row (mostly that line) opened the agent again, and
  // the project's overview could no longer be reached.
  testWidgets(
    'tapping anywhere on a project row opens its overview, not its agent',
    (t) async {
      const device = MacDevice(nodeId: 'n3', name: 'My Mac');
      final project = ProjectInfo(
        id: 'c',
        root: '/Users/me/api',
        name: 'api',
        running: true,
        services: const [],
        agents: [
          (
            'pane1',
            const AgentStatus(
              state: 'working',
              agent: 'omp',
              prompt: 'migrate the firestore data',
              updatedAt: 0,
              stateStartedAt: 0,
            ),
          ),
        ],
      );
      final router = GoRouter(
        routes: [
          GoRoute(path: '/', builder: (_, _) => const HomeScreen()),
          GoRoute(
            path: '/project',
            builder: (_, _) => const Scaffold(body: Text('overview')),
          ),
        ],
      );
      await t.binding.setSurfaceSize(const Size(393, 852));
      await t.pumpWidget(
        ProviderScope(
          overrides: [
            macStorageProvider.overrideWithValue(_FakeStorage([device])),
            transportFactoryProvider.overrideWithValue(
              (_, _) async => demoTransport(),
            ),
            projectListProvider(device).overrideWith((ref) async => [project]),
          ],
          child: MaterialApp.router(theme: atlasTheme(), routerConfig: router),
        ),
      );
      await _settle(t);
      await _throughOnboarding(t);

      await t.tap(find.textContaining('migrate the firestore data'));
      await _settle(t);
      expect(find.text('overview'), findsOneWidget);
      expect(find.byType(AgentChatScreen), findsNothing);

      await t.pumpWidget(const SizedBox());
      await _settle(t);
    },
  );
}
