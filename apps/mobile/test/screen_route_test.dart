import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';
import 'package:atlas_mobile/features/git/changes_screen.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/features/inbox/attention_cards.dart';
import 'package:atlas_mobile/main.dart' show buildRouter;

import 'support/demo_harness.dart';
import 'support/frame_probe.dart';

// One way to push a screen. Every screen the app opens from another one goes through `screenRoute` (core/ui/
// route_page.dart), the same rules as the pages go_router builds with `routePage`: on iOS it slides in and an edge
// swipe pops it, elsewhere the theme's transition, and with Reduce motion it is simply there on its first frame.
// Before, ten call sites built a `MaterialPageRoute` each: they slid correctly, but ignored Reduce motion.
//
// Every test drives the REAL screens through the app's own router (`buildRouter`) against the demo computer and
// reads each frame with FrameProbe.

const _mac = MacDevice(nodeId: 'mac-1', name: 'Mac');
const _phone = Size(393, 852);

/// The app on the Home tab, settled, on [platform], with Reduce motion [reduced].
Future<void> _app(
  WidgetTester t, {
  required bool reduced,
  TargetPlatform platform = TargetPlatform.iOS,
}) async {
  await t.binding.setSurfaceSize(_phone);
  final router = buildRouter();
  addTearDown(router.dispose);
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(DemoStorage([_mac])),
        transportFactoryProvider.overrideWithValue(
          (_, _) async => demoTransport(),
        ),
        onboardingSeenProvider.overrideWith((_) => true),
      ],
      child: MaterialApp.router(
        theme: atlasTheme().copyWith(platform: platform),
        routerConfig: router,
        builder: (context, child) => MediaQuery(
          data: MediaQuery.of(context).copyWith(disableAnimations: reduced),
          child: child!,
        ),
      ),
    ),
  );
  await settle(t);
  await settle(t);
}

Future<void> _leave(WidgetTester t) async {
  await t.pumpWidget(const SizedBox());
  await t.pump(const Duration(seconds: 3));
}

/// A place that pushes a screen: how to get there, what it pushes, how to trigger the push.
class _Site {
  const _Site(this.name, this.screen, this.prepare, this.open);
  final String name;
  final Finder screen;

  /// Gets to the screen that has the trigger.
  final Future<void> Function(WidgetTester) prepare;

  /// The tap that pushes (nothing is pumped after it).
  final Future<void> Function(WidgetTester) open;
}

Future<void> _toProject(WidgetTester t) async {
  await t.tap(find.text('orbit-shop'));
  await settle(t);
}

final _sites = [
  _Site('project -> agent chat', find.byType(AgentChatScreen), (t) async {
    await _toProject(t);
    await t.tap(find.textContaining('Agents'));
    await settle(t);
    await t.ensureVisible(find.text('Why is payments failing?').first);
    await t.pump();
  }, (t) => t.tap(find.text('Why is payments failing?').first)),
  _Site('project -> review changes', find.byType(ChangesScreen), (t) async {
    await _toProject(t);
    await t.ensureVisible(find.text('Review changes'));
    await t.pump();
  }, (t) => t.tap(find.text('Review changes'))),
  _Site('inbox -> agent chat (root navigator)', find.byType(AgentChatScreen), (
    t,
  ) async {
    await t.tap(find.byIcon(Icons.notifications_rounded));
    await settle(t);
  }, (t) => t.tap(find.byType(AttentionHero))),
];

void main() {
  for (final site in _sites) {
    testWidgets(
      '${site.name}: with Reduce motion the screen is whole on its first frame, in place, no slide',
      (t) async {
        await _app(t, reduced: true);
        await site.prepare(t);
        final probe = FrameProbe(t, {'screen': site.screen});
        await site.open(t);
        await probe.record(const Duration(milliseconds: 500));

        final first = probe.frames.firstWhere((f) => f['screen']!.rect != null);
        expect(
          first['screen']!.rect!.left,
          0,
          reason: 'the screen slid in under Reduce motion',
        );
        expect(first['screen']!.opacity, closeTo(1, 0.01));
        probe.expectNoShift('screen');
        await _leave(t);
      },
    );

    testWidgets(
      '${site.name}: on iOS the screen slides in from the right edge',
      (t) async {
        await _app(t, reduced: false);
        await site.prepare(t);
        final probe = FrameProbe(t, {'screen': site.screen});
        await site.open(t);
        await probe.record(const Duration(milliseconds: 700));

        final first = probe.frames.firstWhere((f) => f['screen']!.rect != null);
        expect(
          first['screen']!.rect!.left,
          greaterThan(_phone.width * .8),
          reason:
              'the screen was not still off the right edge on its first frame',
        );
        expect(probe.frames.last['screen']!.rect!.left, 0);
        await _leave(t);
      },
    );
  }

  testWidgets('on iOS an edge swipe pops a pushed screen', (t) async {
    await _app(t, reduced: false);
    await _sites.first.prepare(t);
    await _sites.first.open(t);
    await settle(t);
    expect(find.byType(AgentChatScreen), findsOneWidget);

    await t.dragFrom(const Offset(2, 400), const Offset(300, 0));
    await settle(t);
    expect(find.byType(AgentChatScreen), findsNothing);
    await _leave(t);
  });

  testWidgets(
    'on Android the screen is pushed with the theme\'s transition and the system back pops it',
    (t) async {
      await _app(t, reduced: false, platform: TargetPlatform.android);
      await _sites.first.prepare(t);
      final probe = FrameProbe(t, {'screen': find.byType(AgentChatScreen)});
      await _sites.first.open(t);
      await probe.record(const Duration(milliseconds: 700));
      final first = probe.frames.firstWhere((f) => f['screen']!.rect != null);
      expect(
        first['screen']!.opacity,
        lessThan(1),
        reason: 'the Material transition fades the screen in',
      );
      expect(probe.frames.last['screen']!.opacity, closeTo(1, 0.01));

      final back = await t.binding.handlePopRoute();
      await settle(t);
      expect(back, isTrue);
      expect(find.byType(AgentChatScreen), findsNothing);
      await _leave(t);
    },
  );

  // The one decision lives in route_page.dart: a screen that builds its own route would ignore Reduce motion again.
  test('no screen builds its own route: they all use screenRoute', () {
    final own = RegExp(
      r'\b(MaterialPageRoute|CupertinoPageRoute|PageRouteBuilder)\s*[<(]',
    );
    final bad = <String>[];
    for (final f in Directory('lib').listSync(recursive: true)) {
      if (f is! File || !f.path.endsWith('.dart')) continue;
      final path = f.path.replaceAll('\\', '/');
      if (path == 'lib/core/ui/route_page.dart') continue;
      final lines = f.readAsLinesSync();
      for (var i = 0; i < lines.length; i++) {
        final code = lines[i].split('//').first;
        if (own.hasMatch(code)) bad.add('$path:${i + 1}: ${lines[i].trim()}');
      }
    }
    expect(
      bad,
      isEmpty,
      reason:
          'use screenRoute / pushScreen from core/ui/route_page.dart:\n${bad.join('\n')}',
    );
  });
}
