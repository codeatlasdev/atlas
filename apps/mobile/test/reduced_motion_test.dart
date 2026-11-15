// "Reduce motion" (iOS) / "Remove animations" (Android) reaches the app as
// MediaQuery.disableAnimations. Every animation that loops forever must hold a still frame
// then: pumpAndSettle only returns once no frame is scheduled, so a loop left running makes it
// time out. Each case also runs with animations ON first, so the check proves it can fail.
import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/typing_dots.dart';
import 'package:atlas_mobile/features/agents/crew_widgets.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';
import 'package:atlas_mobile/features/projects/project_detail_screen.dart';

Widget _app(Widget child, {required bool reduced}) => MaterialApp(
  theme: atlasTheme(),
  builder: (context, home) => MediaQuery(
    data: MediaQuery.of(context).copyWith(disableAnimations: reduced),
    child: home!,
  ),
  home: Scaffold(body: Center(child: child)),
);

/// Whether the tree stops asking for frames within a few seconds.
Future<bool> _settles(WidgetTester t) async {
  try {
    await t.pumpAndSettle(
      const Duration(milliseconds: 100),
      EnginePhase.sendSemanticsUpdate,
      const Duration(seconds: 5),
    );
    return true;
  } on FlutterError {
    return false;
  }
}

/// Mounts [build] with animations on, then off, and checks the loop runs only in the first.
Future<void> _stillWhenReduced(WidgetTester t, Widget Function() build) async {
  await t.pumpWidget(_app(build(), reduced: false));
  expect(
    await _settles(t),
    isFalse,
    reason: 'the loop must run when motion is on',
  );
  await t.pumpWidget(const SizedBox());

  await t.pumpWidget(_app(build(), reduced: true));
  expect(
    await _settles(t),
    isTrue,
    reason: 'the loop must hold still under reduced motion',
  );
}

class _Storage extends MacStorage {
  _Storage() : super(storage: null);
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

/// A computer that accepts the connection and never answers `project_detail`, so the screen
/// stays on its skeleton.
class _Silent implements Transport {
  final _in = StreamController<String>.broadcast();
  @override
  Stream<String> get incomingLines => _in.stream;
  @override
  Future<void> sendLine(String line) async {}
  @override
  Future<void> close() => _in.close();
}

/// A computer that answers every command with an empty ok (`info` with the app's protocol), so
/// nothing is left waiting on a deadline when the screen goes away.
class _Answering implements Transport {
  final _in = StreamController<String>.broadcast();
  @override
  Stream<String> get incomingLines => _in.stream;
  @override
  Future<void> sendLine(String line) async {
    final req = jsonDecode(line) as Map<String, dynamic>;
    scheduleMicrotask(() {
      if (!_in.isClosed) {
        _in.add(
          jsonEncode({
            'id': req['id'],
            'ok': true,
            'data': req['cmd'] == 'info' ? {'proto': kAppProto} : {},
          }),
        );
      }
    });
  }

  @override
  Future<void> close() => _in.close();
}

void main() {
  testWidgets('typing dots hold still under reduced motion', (t) async {
    await _stillWhenReduced(t, () => const TypingDots(color: Colors.white));
  });

  testWidgets('crewmate shimmer holds still under reduced motion', (t) async {
    await _stillWhenReduced(
      t,
      () => const NowStrip(
        node: AgentNode(id: 'a', status: 'running', now: 'Reading files'),
        since: 0,
      ),
    );
  });

  testWidgets('the logo holds still under reduced motion', (t) async {
    await _stillWhenReduced(t, () => const AtlasLogo(animated: true));
  });

  testWidgets('project skeleton holds still under reduced motion', (t) async {
    Future<void> mount(bool reduced) => t.pumpWidget(
      ProviderScope(
        overrides: [
          macStorageProvider.overrideWithValue(_Storage()),
          transportFactoryProvider.overrideWithValue((_, _) async => _Silent()),
        ],
        child: MaterialApp(
          theme: atlasTheme(),
          builder: (context, home) => MediaQuery(
            data: MediaQuery.of(context).copyWith(disableAnimations: reduced),
            child: home!,
          ),
          home: const ProjectDetailScreen(
            device: MacDevice(nodeId: 'n1', name: 'My Mac'),
            project: ProjectInfo(
              id: 'a',
              root: '/Users/me/shop',
              name: 'shop',
              running: false,
              services: [ServiceInfo(name: 'api', state: 'running')],
            ),
          ),
        ),
      ),
    );

    // The detail screen polls on a timer, so "settled" means no frame is pending, not no timer.
    await mount(false);
    await t.pump(const Duration(milliseconds: 500));
    expect(find.byType(FadeTransition), findsWidgets);
    expect(
      await _settles(t),
      isFalse,
      reason: 'the skeleton pulses when motion is on',
    );
    await t.pumpWidget(const SizedBox());
    await t.pump(const Duration(seconds: 4));

    await mount(true);
    await t.pump(const Duration(milliseconds: 500));
    expect(
      await _settles(t),
      isTrue,
      reason: 'the skeleton must hold still under reduced motion',
    );
    await t.pumpWidget(const SizedBox());
    await t.pump(const Duration(seconds: 4));
  });

  testWidgets(
    'the agent chat typing indicator holds still under reduced motion',
    (t) async {
      Future<void> mount(bool reduced) => t.pumpWidget(
        ProviderScope(
          overrides: [
            macStorageProvider.overrideWithValue(_Storage()),
            transportFactoryProvider.overrideWithValue(
              (_, _) async => _Answering(),
            ),
          ],
          child: MaterialApp(
            theme: atlasTheme(),
            builder: (context, home) => MediaQuery(
              data: MediaQuery.of(context).copyWith(disableAnimations: reduced),
              child: home!,
            ),
            home: AgentChatScreen(
              device: const MacDevice(nodeId: 'n1', name: 'My Mac'),
              paneKey: 'p1',
              initial: AgentStatus.fromJson({
                'state': 'working',
                'agent': 'omp',
                'updated_at': 1000,
                'state_started_at': 1000,
              }),
              title: 'agent',
            ),
          ),
        ),
      );

      await mount(false);
      await t.pump(const Duration(milliseconds: 500));
      expect(
        await _settles(t),
        isFalse,
        reason: 'the dots bounce when motion is on',
      );
      await t.pumpWidget(const SizedBox());
      await t.pump(const Duration(seconds: 4));

      await mount(true);
      await t.pump(const Duration(milliseconds: 500));
      expect(
        await _settles(t),
        isTrue,
        reason: 'the chat indicator must hold still under reduced motion',
      );
      await t.pumpWidget(const SizedBox());
      await t.pump(const Duration(seconds: 4));
    },
  );
}
