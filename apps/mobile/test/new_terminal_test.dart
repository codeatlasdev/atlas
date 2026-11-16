import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';

import 'fake_transport.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/pty_open.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/projects/project_detail_screen.dart';
import 'package:atlas_mobile/features/terminal/new_terminal.dart';

import 'support/demo_harness.dart';

// "New terminal" (#53): a phone opens a shell on the computer. `pty_open` takes registry ids, never a path, and the
// terminal it answers opens in the existing terminal screen (typing there stays behind Face ID).
//   - the reply parses from hostd's REAL payload (`test/fixtures/pty_open.json`, written by
//     `UPDATE_FIXTURES=1 cargo test -p atlas-host --lib -- mobile::tests::a_phone_opens_a_shell`);
//   - the project page and the computer page start it and open `/terminal`; a crew server has no projects, so its
//     terminal starts in the crew home; one reached through a Mac is not offered it (`canOpenTerminal`: hostd
//     refuses `pty_open` inside `crew_call`, and the Rust test `opening_a_terminal_needs_act…` pins that);
//   - a refusal (a phone without `act`, an unknown project) is shown and opens nothing.

Map<String, dynamic> _fixture(String name) =>
    jsonDecode(File('test/fixtures/$name').readAsStringSync())
        as Map<String, dynamic>;

/// The demo computer, recording every request the app sends it.
class _Spy implements Transport {
  _Spy(this.inner, this.calls);
  final Transport inner;
  final List<Map<String, dynamic>> calls;
  @override
  Stream<String> get incomingLines => inner.incomingLines;
  @override
  Future<void> sendLine(String line) {
    calls.add(jsonDecode(line) as Map<String, dynamic>);
    return inner.sendLine(line);
  }

  @override
  Future<void> close() => inner.close();
}

const _mac = MacDevice(nodeId: 'demo', name: 'Demo Mac');
const _project = ProjectInfo(
  id: 'demo-shop',
  root: '/Users/demo/Projetos/orbit-shop',
  name: 'orbit-shop',
  running: true,
  services: [],
);

typedef _Run = ({
  List<Map<String, dynamic>> opened,
  List<Map<String, dynamic>> sent,
});

/// [home] under a router that records where `/terminal` is pushed, over [transport] (the demo computer, spied on).
Future<_Run> _pump(
  WidgetTester t,
  Widget Function(WidgetRef ref) home, {
  MacDevice device = _mac,
  Transport Function(List<Map<String, dynamic>> sent)? transport,
}) async {
  final opened = <Map<String, dynamic>>[];
  final sent = <Map<String, dynamic>>[];
  final router = GoRouter(
    routes: [
      GoRoute(
        path: '/',
        builder: (_, _) => Scaffold(
          body: Consumer(
            builder: (_, ref, _) {
              ref.watch(connectionNotifierProvider(device));
              return home(ref);
            },
          ),
        ),
      ),
      GoRoute(
        path: '/terminal',
        builder: (_, state) {
          opened.add((state.extra! as Map).cast<String, dynamic>());
          return const Scaffold(body: Text('terminal screen'));
        },
      ),
    ],
  );
  await t.binding.setSurfaceSize(const Size(393, 852));
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(DemoStorage([device])),
        transportFactoryProvider.overrideWithValue(
          (_, _) async => (transport ?? (s) => _Spy(demoTransport(), s))(sent),
        ),
      ],
      child: MaterialApp.router(theme: atlasTheme(), routerConfig: router),
    ),
  );
  await settle(t);
  return (opened: opened, sent: sent);
}

List<Map<String, dynamic>> _opens(List<Map<String, dynamic>> sent) =>
    sent.where((c) => c['cmd'] == 'pty_open').toList();

/// The button sits beside "New agent", on the project's Agents tab.
Future<void> _toAgentsTab(WidgetTester t) async {
  await t.tap(find.textContaining('Agents').first);
  await settle(t);
}

Widget _go(WidgetRef ref, MacDevice device) => Builder(
  builder: (context) => TextButton(
    key: const Key('go'),
    onPressed: () => newTerminalOnComputer(context, ref, device),
    child: const Text('go'),
  ),
);

void main() {
  test('pty_open from hostd parses: the terminal to attach', () {
    final o = PtyOpened.fromJson(_fixture('pty_open.json'));
    expect(o.pty, 'pty-fixture');
  });

  test('only a computer or a directly paired crew server opens terminals', () {
    expect(canOpenTerminal(_mac), isTrue);
    expect(
      canOpenTerminal(MacDevice.crew(_mac, 'box', 'Crew box')),
      isFalse,
      reason: 'hostd refuses pty_open inside crew_call: the phone could never attach',
    );
    expect(
      isCrewServer(const MacDevice(nodeId: 'c', name: 'c', caps: ['crew'])),
      isTrue,
    );
    expect(isCrewServer(_mac), isFalse);
  });

  testWidgets('the project page opens a shell in the project by its id', (
    t,
  ) async {
    final r = await _pump(
      t,
      (_) => const ProjectDetailScreen(device: _mac, project: _project),
    );
    await _toAgentsTab(t);
    await t.ensureVisible(find.byKey(const Key('new-terminal')));
    await t.tap(find.byKey(const Key('new-terminal')));
    await settle(t);
    // The demo project has two checkouts: asked where, "main" is the project's own folder.
    await t.tap(find.byKey(const Key('new-terminal-main')));
    await settle(t);
    final sent = _opens(r.sent);
    expect(sent, hasLength(1));
    expect(sent.single['args'], {
      'project': 'demo-shop',
    }, reason: 'an id, never a path');
    expect(r.opened.single['pty'], startsWith('pty-shell-'));
    await t.pumpWidget(const SizedBox());
    await settle(t);
  });

  testWidgets('a worktree is named by its id and the title says where', (
    t,
  ) async {
    final r = await _pump(
      t,
      (_) => const ProjectDetailScreen(device: _mac, project: _project),
    );
    await _toAgentsTab(t);
    await t.ensureVisible(find.byKey(const Key('new-terminal')));
    await t.tap(find.byKey(const Key('new-terminal')));
    await settle(t);
    await t.tap(find.byKey(const Key('new-terminal-fix-checkout')));
    await settle(t);
    expect(_opens(r.sent).single['args'], {
      'project': 'demo-shop',
      'worktree': 'demo-wt-fix',
    });
    expect(r.opened.single['title'], 'fix-checkout');
    await t.pumpWidget(const SizedBox());
    await settle(t);
  });

  testWidgets(
    'the computer page asks which project, then opens a shell there',
    (t) async {
      final r = await _pump(t, (ref) => _go(ref, _mac));
      await t.tap(find.byKey(const Key('go')));
      await settle(t);
      await t.tap(find.byKey(const Key('new-terminal-docs')));
      await settle(t);
      expect(_opens(r.sent).single['args'], {'project': 'demo-docs'});
      expect(r.opened, hasLength(1));
      await t.pumpWidget(const SizedBox());
      await settle(t);
    },
  );

  testWidgets(
    'a crew server has no projects: its terminal starts in the crew home',
    (t) async {
      const crew = MacDevice(nodeId: 'crew1', name: 'crew-box', caps: ['crew']);
      // A crew server's hostd: it answers `info` (the connect check), then the test answers `pty_open` with the
      // real reply (`pty_open.json`), as a crew server's hostd does for no argument.
      final fake = FakeTransport(answerInfo: true);
      final r = await _pump(
        t,
        (ref) => _go(ref, crew),
        device: crew,
        transport: (_) => fake,
      );
      await t.tap(find.byKey(const Key('go')));
      await t.pump(const Duration(milliseconds: 100));
      final req = fake.sentLines
          .map((l) => jsonDecode(l) as Map<String, dynamic>)
          .singleWhere((c) => c['cmd'] == 'pty_open');
      expect(req['args'], isEmpty, reason: 'no argument = the crew home');
      fake.injectLine(
        jsonEncode({
          'id': req['id'],
          'ok': true,
          'data': _fixture('pty_open.json'),
        }),
      );
      await settle(t);
      expect(r.opened.single['pty'], 'pty-fixture');
      expect(r.opened.single['title'], 'crew-box');
      await t.pumpWidget(const SizedBox());
      await settle(t);
    },
  );

  testWidgets('a refusal is shown and opens nothing', (t) async {
    final r = await _pump(
      t,
      (ref) => Builder(
        builder: (context) => TextButton(
          key: const Key('go'),
          onPressed: () => openNewTerminal(context, ref, _mac, project: '/etc'),
          child: const Text('go'),
        ),
      ),
    );
    await t.tap(find.byKey(const Key('go')));
    await settle(t);
    expect(find.textContaining("Couldn't open a terminal"), findsOneWidget);
    expect(find.textContaining('unknown project'), findsOneWidget);
    expect(r.opened, isEmpty);
    await t.pumpWidget(const SizedBox());
    await settle(t);
  });
}
