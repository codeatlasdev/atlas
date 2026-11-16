import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';
import 'package:atlas_mobile/features/agents/approval_banner.dart';
import 'package:atlas_mobile/features/home/dashboard.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/features/home/project_tile.dart';
import 'package:atlas_mobile/features/home/splash.dart';
import 'package:atlas_mobile/features/inbox/activity_row.dart';
import 'package:atlas_mobile/features/inbox/attention_cards.dart';
import 'package:atlas_mobile/features/inbox/inbox.dart';
import 'package:atlas_mobile/features/projects/detail/header.dart';
import 'package:atlas_mobile/features/projects/detail/skeleton.dart';
import 'package:atlas_mobile/features/projects/project_avatar.dart';
import 'package:atlas_mobile/features/projects/project_detail_screen.dart';
import 'package:atlas_mobile/features/terminal/live_screen.dart';
import 'package:atlas_mobile/features/terminal/terminal_feed.dart';
import 'package:atlas_mobile/features/terminal/terminal_screen.dart';
import 'package:atlas_mobile/main.dart' show buildRouter;

import 'support/demo_harness.dart';
import 'support/frame_probe.dart';

// Regression: moving between Home, a project, an agent chat, the Inbox and a terminal flickered: content the
// phone already had faded in or showed a spinner for a frame, and things that arrived later pushed what the user
// was reading. Every test here drives the REAL screens through the app's own router (`buildRouter` in
// lib/main.dart: the shell with its tabs, `/project`, `/terminal`; the chat is pushed by the screens themselves)
// against the demo computer behind a link with latency, and reads every frame with FrameProbe.

const _mac = MacDevice(nodeId: 'mac-1', name: 'Mac');

/// The demo computer behind a link that takes [delay] to carry each request.
class _Slow implements Transport {
  _Slow(this.inner, this.delay);
  final Transport inner;
  Duration delay;

  @override
  Stream<String> get incomingLines => inner.incomingLines;

  @override
  Future<void> sendLine(String line) async {
    if (delay > Duration.zero) await Future<void>.delayed(delay);
    return inner.sendLine(line);
  }

  @override
  Future<void> close() => inner.close();
}

/// The app on the Home tab, settled (splash gone, project list in), on a phone with iOS page transitions.
Future<({GoRouter router, _Slow link})> _app(
  WidgetTester t, {
  Duration latency = const Duration(milliseconds: 120),
  bool settled = true,
  Size size = const Size(393, 852),
}) async {
  await t.binding.setSurfaceSize(size);
  final router = buildRouter();
  addTearDown(router.dispose);
  late _Slow link;
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(DemoStorage([_mac])),
        transportFactoryProvider.overrideWithValue(
          (_, _) async => link = _Slow(demoTransport(), Duration.zero),
        ),
        onboardingSeenProvider.overrideWith((_) => true),
      ],
      child: MaterialApp.router(
        theme: atlasTheme().copyWith(platform: TargetPlatform.iOS),
        routerConfig: router,
      ),
    ),
  );
  link.delay = latency;
  if (!settled) return (router: router, link: link);
  await settle(t);
  await settle(t);
  return (router: router, link: link);
}

Future<void> _leave(WidgetTester t) async {
  await t.pumpWidget(const SizedBox());
  await t.pump(const Duration(seconds: 3));
}

final _detail = find.byType(ProjectDetailScreen);
final _name = find.descendant(of: _detail, matching: find.text('orbit-shop'));
final _avatar = find.descendant(
  of: _detail,
  matching: find.byType(ProjectAvatar),
);

Future<void> _openProject(WidgetTester t) async {
  await t.tap(find.text('orbit-shop'));
  await t.pump();
}

Future<void> _back(WidgetTester t) async {
  await t.tap(find.byType(BackButton));
  await t.pump();
  for (var i = 0; i < 40; i++) {
    await t.pump(const Duration(milliseconds: 50));
  }
}

/// [child] keeps its offset inside [parent] on every frame both are on screen: a page that is still sliding in
/// moves as a whole, but nothing in it may move relative to it (content landing late, a second layout pass).
void _expectPinned(FrameProbe p, String child, String parent) {
  Offset? first;
  for (var i = 0; i < p.frames.length; i++) {
    final c = p.frames[i][child]!.rect, q = p.frames[i][parent]!.rect;
    if (c == null || q == null) continue;
    final rel = c.topLeft - q.topLeft;
    first ??= rel;
    expect(
      (rel - first).distance,
      lessThan(0.5),
      reason:
          '$child moved inside $parent on frame $i: it was at $first and is at $rel',
    );
  }
  expect(first, isNotNull, reason: '$child was never on screen');
}

/// [child] is never fainter than [parent] on any frame it is in the tree: it has no fade of its own on top of
/// the page's (content that is already there must not fade in after its page).
void _expectNoOwnFade(FrameProbe p, String child, String parent) {
  for (var i = 0; i < p.frames.length; i++) {
    final c = p.frames[i][child]!, q = p.frames[i][parent]!;
    if (c.rect == null) continue;
    expect(
      c.opacity,
      greaterThanOrEqualTo(q.opacity - 0.01),
      reason:
          '$child fades on its own on frame $i: opacity ${c.opacity.toStringAsFixed(2)} under a page at ${q.opacity.toStringAsFixed(2)}',
    );
  }
}

final _chat = find.byType(AgentChatScreen);

/// The thread: the one reversed list (the crew rail above it is a horizontal one).
final _chatList = find.descendant(
  of: _chat,
  matching: find.byWidgetPredicate((w) => w is ListView && w.reverse),
);

/// How far the thread is from its end, in pixels.
double _chatOffset(WidgetTester t) => t
    .state<ScrollableState>(
      find.descendant(of: _chatList, matching: find.byType(Scrollable)).first,
    )
    .position
    .pixels;

void main() {
  group('Agent chat', () {
    /// Home, then the demo project's agent called [title], through the project's Agents tab.
    Future<void> openChatFromProject(WidgetTester t, String title) async {
      await _openProject(t);
      await settle(t);
      await t.tap(find.textContaining('Agents'));
      await settle(t);
      await t.ensureVisible(find.text(title).first);
      await t.pump();
      await t.tap(find.text(title).first);
    }

    testWidgets(
      'opens at the end on its first frame, the messages it has are opaque and the last one never moves',
      (t) async {
        await _app(t, latency: const Duration(milliseconds: 100));
        await _openProject(t);
        await settle(t);
        await t.tap(find.textContaining('Agents'));
        await settle(t);
        final probe = FrameProbe(t, {
          'chat': _chat,
          'answer': find.descendant(
            of: _chat,
            matching: find.textContaining('payments exits with code 2'),
          ),
        });
        await t.tap(find.text('Why is payments failing?').first);
        await probe.record(
          const Duration(milliseconds: 900),
          sample: {
            'fromEnd': () => _chat.evaluate().isEmpty ? 0 : _chatOffset(t),
          },
        );
        final first = probe.visibleFrames('chat').first;
        expect(
          probe.frames[first]['answer']!.rect,
          isNotNull,
          reason: 'the last message is not there on the chat\'s first frame',
        );
        probe.expectOpaqueWhenPresent('answer');
        probe.expectSampleWithin('fromEnd', 0, 0.5);
        _expectPinned(probe, 'answer', 'chat');
        await _leave(t);
      },
    );

    testWidgets(
      'a prompt the phone already knows about is on screen on the first frame, it does not push the thread when it arrives',
      (t) async {
        await _app(t, latency: const Duration(milliseconds: 100));
        await t.tap(find.byIcon(Icons.notifications_rounded));
        await settle(t);
        final probe = FrameProbe(t, {
          'chat': _chat,
          'banner': find.byType(ApprovalBanner),
          'answer': find.descendant(
            of: _chat,
            matching: find.textContaining('I wrote migration'),
          ),
        });
        await t.tap(find.byType(AttentionHero));
        await probe.record(const Duration(milliseconds: 900));
        final first = probe.visibleFrames('chat').first;
        expect(
          probe.frames[first]['banner']!.rect,
          isNotNull,
          reason: 'the approval prompt was not on the chat\'s first frame',
        );
        _expectPinned(probe, 'answer', 'chat');
        await _leave(t);
      },
    );

    testWidgets(
      'a prompt that goes away or arrives while the chat is open glides: it never shoves the thread in one frame',
      (t) async {
        await _app(t, latency: const Duration(milliseconds: 100));
        await t.tap(find.byIcon(Icons.notifications_rounded));
        await settle(t);
        await t.tap(find.byType(AttentionHero));
        await settle(t);
        expect(find.byType(ApprovalBanner), findsOneWidget);

        // The room the prompt takes above the composer.
        final room = find.descendant(
          of: _chat,
          matching: find.byWidgetPredicate(
            (w) => w is AnimatedSize && w.alignment == Alignment.bottomCenter,
          ),
        );
        final probe = FrameProbe(t, {'room': room});
        await t.tap(find.text('Allow'));
        await probe.record(const Duration(milliseconds: 800));

        final heights = [for (final f in probe.frames) f['room']!.rect!.height];
        final moved = heights.first - heights.last;
        expect(
          moved,
          greaterThan(40),
          reason: 'the prompt left and gave its room back',
        );
        var step = 0.0;
        for (var i = 1; i < heights.length; i++) {
          step = [
            step,
            (heights[i] - heights[i - 1]).abs(),
          ].reduce((a, b) => a > b ? a : b);
        }
        expect(
          step,
          lessThan(moved / 4),
          reason:
              'the room changed by ${moved.toStringAsFixed(0)} px, ${step.toStringAsFixed(0)} of them in one frame',
        );
        await _leave(t);
      },
    );

    testWidgets(
      'a reader above the end is not moved by sending, by the agent starting to work or by its answer; the end is one tap away',
      (t) async {
        await _app(
          t,
          latency: const Duration(milliseconds: 100),
          size: const Size(393, 600),
        );
        await openChatFromProject(t, 'Why is payments failing?');
        await settle(t);
        await t.drag(_chatList, const Offset(0, 200));
        await settle(t);
        expect(
          _chatOffset(t),
          greaterThan(100),
          reason: 'the reader is above the end',
        );
        final scout = find.descendant(
          of: _chatList,
          matching: find.text('LedgerScout'),
        );
        expect(scout, findsOneWidget);

        final probe = FrameProbe(t, {'scout': scout});
        await t.enterText(find.byType(TextField), 'hello there');
        await t.pump();
        await t.tap(find.byIcon(Icons.arrow_upward_rounded));
        // The agent works for 2.6 s, then answers.
        await probe.record(
          const Duration(milliseconds: 4500),
          sample: {'offset': () => _chatOffset(t)},
        );
        probe.expectNoShift('scout');
        probe.expectSampleWithin('offset', 179.5, 180.5);
        expect(find.textContaining('This is a demo'), findsNothing);
        expect(find.text('New activity'), findsOneWidget);

        await t.tap(find.text('New activity'));
        await settle(t);
        expect(_chatOffset(t), lessThan(1));
        expect(find.textContaining('This is a demo'), findsOneWidget);
        await _leave(t);
      },
    );

    testWidgets(
      'a reader at the end stays at the end through sending and the answer',
      (t) async {
        await _app(
          t,
          latency: const Duration(milliseconds: 100),
          size: const Size(393, 600),
        );
        await openChatFromProject(t, 'Why is payments failing?');
        await settle(t);
        expect(_chatOffset(t), 0);

        final probe = FrameProbe(t, {'chat': _chat});
        await t.enterText(find.byType(TextField), 'hello there');
        await t.pump();
        await t.tap(find.byIcon(Icons.arrow_upward_rounded));
        await probe.record(
          const Duration(milliseconds: 4500),
          sample: {'offset': () => _chatOffset(t)},
        );
        probe.expectSampleWithin('offset', 0, 0.5);
        expect(find.textContaining('This is a demo'), findsOneWidget);
        await _leave(t);
      },
    );
  });

  testWidgets(
    'the terminal of an agent opens complete on its first frame: its strip, its body and its button never move or appear later',
    (t) async {
      await _app(t, latency: const Duration(milliseconds: 100));
      await t.tap(find.byIcon(Icons.notifications_rounded));
      await settle(t);
      await t.tap(find.byType(AttentionHero));
      await settle(t);
      final terminal = find.byType(TerminalScreen);
      final probe = FrameProbe(t, {
        'terminal': terminal,
        'feed': find.byType(TerminalFeed),
        'live': find.byType(LiveScreen),
        'notice': find.text('Reconnecting…'),
        'tap': find.text('Tap to type'),
        'strip': find.descendant(
          of: terminal,
          matching: find.text('needs you'),
        ),
      });
      await t.tap(find.byIcon(Icons.terminal));
      await probe.record(const Duration(milliseconds: 1500));

      final first = probe.visibleFrames('terminal').first;
      final f = probe.frames[first];
      expect(
        f['tap']!.rect,
        isNotNull,
        reason: 'no "Tap to type" on the first frame',
      );
      expect(
        f['feed']!.rect != null || f['live']!.rect != null,
        isTrue,
        reason: 'no terminal body on the first frame',
      );
      expect(
        probe.visibleFrames('notice'),
        isEmpty,
        reason: 'a notice line pushed the terminal down',
      );
      _expectPinned(probe, 'tap', 'terminal');
      // The strip above the body is there from the first frame, so the body never gets pushed down by it.
      expect(
        f['strip']!.rect,
        isNotNull,
        reason: 'the agent strip arrived after the page',
      );
      _expectPinned(probe, 'strip', 'terminal');
      await _leave(t);
    },
  );

  group('Under the status bar', () {
    // Regression: Home and a project scrolled their text under the clock (the scroll view spanned the whole
    // screen and only padded its top): the lines read through the status bar. A scroll view has to start
    // below it, so what scrolls away is cut there.
    const notch = 47.0;
    void withNotch(WidgetTester t) {
      final top = notch * t.view.devicePixelRatio;
      t.view.padding = FakeViewPadding(top: top);
      t.view.viewPadding = FakeViewPadding(top: top);
      addTearDown(t.view.reset);
    }

    testWidgets('Home scrolls below the status bar', (t) async {
      withNotch(t);
      await _app(t);
      final list = find.descendant(
        of: find.byType(HomeDashboard),
        matching: find.byType(CustomScrollView),
      );
      expect(t.getRect(list).top, greaterThanOrEqualTo(notch));
      await _leave(t);
    });

    testWidgets('a project scrolls below the status bar', (t) async {
      withNotch(t);
      await _app(t);
      await _openProject(t);
      await settle(t);
      final list = find.descendant(
        of: _detail,
        matching: find.byType(CustomScrollView),
      );
      expect(t.getRect(list).top, greaterThanOrEqualTo(notch));
      await _leave(t);
    });
  });

  testWidgets('Inbox: what it already lists is there on its first frame', (
    t,
  ) async {
    await _app(t, latency: const Duration(milliseconds: 100));
    final probe = FrameProbe(t, {
      'page': find.byType(InboxScreen),
      'hero': find.byType(AttentionHero),
      'row': find.byType(ActivityRow),
    });
    await t.tap(find.byIcon(Icons.notifications_rounded));
    await probe.record(const Duration(milliseconds: 1200));
    // The page's own fade-in (the tab switch) is not the rows': a row that was already known has no fade of its
    // own and does not lift into place.
    _expectNoOwnFade(probe, 'hero', 'page');
    _expectNoOwnFade(probe, 'row', 'page');
    _expectPinned(probe, 'hero', 'page');
    _expectPinned(probe, 'row', 'page');
    await _leave(t);
  });

  testWidgets(
    'Home: the splash hands over to a dashboard whose projects are complete on their first frame',
    (t) async {
      await _app(t, latency: const Duration(milliseconds: 100), settled: false);
      final probe = FrameProbe(t, {
        'splash': find.byType(HomeSplash),
        'dashboard': find.byType(HomeDashboard),
        'tile': find.byType(ProjectTile),
      });
      await probe.record(const Duration(milliseconds: 3500));
      probe.expectNeverBothVisible('splash', 'dashboard');
      // The page fades in over the ground (a short fade, not the tiles': they are already there with it).
      _expectNoOwnFade(probe, 'tile', 'dashboard');
      await _leave(t);
    },
  );

  group('Home to a project', () {
    testWidgets(
      'the project slides in over Home, which stays under it, and its header never moves when the data lands',
      (t) async {
        await _app(t, latency: const Duration(milliseconds: 100));
        final probe = FrameProbe(t, {
          'home': find.byType(HomeScreen),
          'detail': _detail,
          'skeleton': find.byType(ProjectSkeleton),
          'header': find.byType(ProjectHeader),
          'avatar': _avatar,
          'name': _name,
        });
        await t.tap(find.text('orbit-shop'));
        await probe.record(const Duration(milliseconds: 700));

        // The page is built on the first frame it exists (skeleton: the phone has nothing cached yet) and
        // slides in from the right.
        final first = probe.visibleFrames('detail').first;
        final left = [
          for (final f in probe.frames.skip(first)) f['detail']!.rect!.left,
        ];
        expect(left.first, greaterThan(300), reason: 'starts off to the right');
        expect(left.last, 0);
        expect(
          [for (var i = 1; i < left.length; i++) left[i] <= left[i - 1]],
          everyElement(isTrue),
          reason: 'one slide, no back-and-forth',
        );
        // Home stays opaque under it for the whole slide: no blank frame behind the moving page.
        probe.expectOpaqueWhenPresent('home');
        probe.expectOpaqueWhenPresent('detail');
        for (var i = first; i < probe.frames.length; i++) {
          final f = probe.frames[i];
          expect(
            f['skeleton']!.visibleOn(const Rect.fromLTWH(0, 0, 393, 852)) ||
                f['header']!.visibleOn(const Rect.fromLTWH(0, 0, 393, 852)),
            isTrue,
            reason: 'frame $i shows neither the skeleton nor the header',
          );
        }
        // The skeleton becomes the real header (frame where the data lands) without moving the avatar or the name.
        expect(
          probe.visibleFrames('skeleton'),
          isNotEmpty,
          reason: 'the test must see the skeleton to cover its hand-over',
        );
        expect(probe.visibleFrames('header'), isNotEmpty);
        _expectPinned(probe, 'avatar', 'detail');
        _expectPinned(probe, 'name', 'detail');
        await _leave(t);
      },
    );

    testWidgets(
      'a project opened again shows its content on the first frame, never the skeleton',
      (t) async {
        await _app(t, latency: const Duration(milliseconds: 100));
        await _openProject(t);
        await settle(t);
        expect(find.byType(ProjectHeader), findsOneWidget);
        await _back(t);
        expect(_detail, findsNothing);

        final probe = FrameProbe(t, {
          'detail': _detail,
          'skeleton': find.byType(ProjectSkeleton),
          'header': find.byType(ProjectHeader),
          'avatar': _avatar,
          'name': _name,
        });
        await t.tap(find.text('orbit-shop'));
        await probe.record(const Duration(milliseconds: 700));

        final first = probe.visibleFrames('detail').first;
        expect(
          probe.frames[first]['header']!.rect,
          isNotNull,
          reason: 'the first frame of the page has no header: it shows the skeleton',
        );
        expect(
          probe.visibleFrames('skeleton'),
          isEmpty,
          reason: 'skeleton frames: ${probe.visibleFrames('skeleton')}',
        );
        probe.expectOpaqueWhenPresent('header');
        _expectPinned(probe, 'avatar', 'detail');
        _expectPinned(probe, 'name', 'detail');
        await _leave(t);
      },
    );

    testWidgets('an edge swipe from the left takes the project back to Home', (
      t,
    ) async {
      await _app(t, latency: Duration.zero);
      await _openProject(t);
      await settle(t);
      expect(_detail, findsOneWidget);

      await t.dragFrom(const Offset(2, 400), const Offset(300, 0));
      await settle(t);
      expect(_detail, findsNothing);
      expect(find.byType(HomeScreen), findsOneWidget);
      await _leave(t);
    });

    testWidgets(
      'with Reduce motion the project is there on the first frame, no slide',
      (t) async {
        t.platformDispatcher.accessibilityFeaturesTestValue =
            const FakeAccessibilityFeatures(disableAnimations: true);
        addTearDown(t.platformDispatcher.clearAccessibilityFeaturesTestValue);
        await _app(t, latency: const Duration(milliseconds: 100));
        final probe = FrameProbe(t, {'detail': _detail});
        await t.tap(find.text('orbit-shop'));
        await probe.record(const Duration(milliseconds: 300));
        final first = probe.visibleFrames('detail').first;
        expect(probe.frames[first]['detail']!.rect!.left, 0);
        probe.expectNoShift('detail');
        await _leave(t);
      },
    );
  });
}
