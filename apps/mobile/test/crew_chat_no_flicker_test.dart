// Regression (screen recording 2026-11-12: chat_open_fade_50s, send_jump_36s, statusbar_overlap_49s): the crewmate
// chat faded its history in message by message on every open, jumped when a message was sent, moved the reader's
// place when something arrived below it, and let the thread draw under the clock. Frame by frame, through the
// app's REAL routes (Crew tab -> row -> chat), real providers and a scripted crew server.
import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat/crewmate_chat_screen.dart';
import 'package:atlas_mobile/features/crew/list/crew_screen.dart';
import 'package:atlas_mobile/features/crew/profile/crewmate_profile_screen.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/main.dart';

import 'support/demo_harness.dart';
import 'support/frame_probe.dart';

const _mac = MacDevice(nodeId: 'mac-1', name: 'Mac');
const _id = 'crewmate-working';
const _statusBar = 47.0;

/// Paired storage with nothing read from the platform's secure storage (the inbox badge asks for its last-seen time).
class _Storage extends DemoStorage {
  _Storage(super.devices);
  @override
  Future<DateTime?> loadInboxSeen() async => null;
}

/// A crew server that answers like hostd (`crewmate_list` is the real fixture) with a thread it keeps newest first,
/// and that the test changes the way hostd does: a task is added or moves on, then a `crewmate_task` push says so.
class _Server implements Transport {
  _Server({int tasks = 40}) {
    for (var n = 0; n < tasks; n++) {
      thread.insert(0, _task(n));
    }
  }

  /// Newest first, as `crewmate_tasks` answers.
  final List<Map<String, Object?>> thread = [];
  final _in = StreamController<String>.broadcast();

  /// How long `crewmate_tasks` takes to answer (a real link is never instant).
  Duration tasksDelay = Duration.zero;
  int tasksCalls = 0;
  String crewState = 'idle';
  int queued = 0;

  /// Task `n`: one prompt and a reply of a few lines (heights differ), one day per six tasks.
  static Map<String, Object?> _task(int n) => {
    'id': 'task-$n',
    'ts': 1762000000 + n * 4 * 3600,
    'source': 'mobile',
    'prompt': 'question $n',
    'state': 'done',
    'result': List.generate(1 + n % 4, (i) => 'answer $n.$i').join('\n\n'),
    'started_at': 1762000000 + n * 4 * 3600 + 5,
    'finished_at': 1762000000 + n * 4 * 3600 + 60,
  };

  @override
  Stream<String> get incomingLines => _in.stream;

  @override
  Future<void> close() async {
    if (!_in.isClosed) await _in.close();
  }

  void emit(Map<String, Object?> event) {
    if (!_in.isClosed) _in.add(jsonEncode(event));
  }

  /// hostd's push for a task that changed.
  void pushTask(String taskId) => emit({
    'ev': 'crewmate_task',
    'crewmate_id': _id,
    'task': {'id': taskId},
  });

  /// A task that did not come from this phone, at the newest end of the thread.
  void addFromDesktop(String prompt, {String state = 'queued'}) {
    thread.insert(0, {
      'id': 'task-desktop',
      'ts': 1762000000 + 200 * 3600,
      'source': 'desktop',
      'prompt': prompt,
      'state': state,
    });
    pushTask('task-desktop');
  }

  /// The newest task moves on to [state] (with a reply when it is done).
  void advanceNewest(String state, {String? result}) {
    final t = thread.first;
    t['state'] = state;
    if (state == 'running') {
      final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
      t['started_at'] = now;
      t['steps'] = [
        {'ts': now, 'text': 'Reading files'},
      ];
    }
    if (result != null) t['result'] = result;
    crewState = state == 'running' ? 'working' : 'idle';
    queued = state == 'queued' ? 1 : 0;
    pushTask(t['id']! as String);
  }

  Map<String, Object?> _list() {
    final real = jsonDecode(
      File('test/fixtures/crewmate_list.json').readAsStringSync(),
    ) as Map<String, dynamic>;
    final first = Map<String, Object?>.of(
      (real['crewmates'] as List).first as Map<String, dynamic>,
    );
    first['state'] = crewState;
    first['queued'] = queued;
    if (crewState != 'working') first.remove('running_task');
    return {
      ...real,
      'crewmates': [first],
    };
  }

  @override
  Future<void> sendLine(String line) async {
    final m = jsonDecode(line) as Map<String, dynamic>;
    final cmd = m['cmd'] as String;
    final args = (m['args'] as Map?)?.cast<String, dynamic>() ?? const {};
    Map<String, Object?> reply(Object data) => {'ok': true, 'data': data};
    Future<void> answer(Map<String, Object?> body, [Duration? after]) async {
      void send() {
        if (!_in.isClosed) _in.add(jsonEncode({'id': m['id'], ...body}));
      }

      if (after == null || after == Duration.zero) {
        scheduleMicrotask(send);
      } else {
        Timer(after, send);
      }
    }

    switch (cmd) {
      case 'info':
        return answer(reply({'proto': kAppProto}));
      case 'crew_servers':
        return answer(reply({'servers': []}));
      case 'crewmate_list':
        return answer(reply(_list()));
      case 'crewmate_tasks':
        tasksCalls++;
        return answer(reply({'tasks': thread}), tasksDelay);
      case 'crewmate_task':
        thread.insert(0, {
          'id': 'task-sent',
          'ts': 1762000000 + 200 * 3600,
          'source': 'mobile',
          'prompt': args['prompt'],
          'state': 'queued',
        });
        queued = 1;
        return answer(
          reply({
            'task': {'id': 'task-sent', 'state': 'queued'},
          }),
        );
      default:
        return answer(reply({}));
    }
  }
}

class _Rig {
  _Rig(this.t, this.server, this.router);
  final WidgetTester t;
  final _Server server;
  final GoRouter router;

  /// The chat's own page: what everything in it is measured against (the route's slide and fade are not the
  /// chat's doing).
  static final page = find.byType(CrewmateChatScreen);
  static final thread = find.byWidgetPredicate(
    (w) => w is ListView && w.reverse,
  );

  ScrollController? get controller =>
      thread.evaluate().isEmpty ? null : t.widget<ListView>(thread).controller;

  /// How far the reader is from the newest message; 0 with no thread on screen.
  double offset() => controller?.hasClients ?? false ? controller!.offset : 0;

  Future<void> openChat() async {
    await t.tap(find.text('Reviewer').first);
  }

  Future<void> back() async {
    await t.tap(find.byIcon(Icons.arrow_back_ios_new_rounded).first);
  }
}

/// The app's routes on an iPhone-sized screen with a notch, at the Crew tab, on [server].
Future<_Rig> _launch(WidgetTester t, _Server server) async {
  final view = t.view;
  view.physicalSize = const Size(393, 852);
  view.devicePixelRatio = 1;
  view.padding = const FakeViewPadding(top: _statusBar, bottom: 34);
  view.viewPadding = const FakeViewPadding(top: _statusBar, bottom: 34);
  addTearDown(() {
    view.resetPhysicalSize();
    view.resetDevicePixelRatio();
    view.resetPadding();
    view.resetViewPadding();
  });
  final router = buildRouter();
  addTearDown(router.dispose);
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage([_mac])),
        transportFactoryProvider.overrideWithValue((_, _) async => server),
        onboardingSeenProvider.overrideWith((_) => true),
      ],
      child: MaterialApp.router(
        theme: atlasTheme().copyWith(platform: TargetPlatform.iOS),
        routerConfig: router,
      ),
    ),
  );
  router.go('/crew');
  await settle(t);
  expect(find.byType(CrewScreen), findsOneWidget);
  expect(find.text('Reviewer'), findsWidgets);
  return _Rig(t, server, router);
}

/// What the probe tracks: the page and the newest [count] exchanges of the thread (prompt and reply).
Map<String, Finder> _tracked({int newest = 39, int count = 3}) => {
  'page': _Rig.page,
  for (var n = newest; n > newest - count; n--) ...{
    'q$n': find.text('question $n'),
    'a$n': find.text('answer $n.0'),
  },
};

/// [name] is opaque in the page, on every frame it is in the tree: the route's own fade is divided out.
void _expectOpaqueInPage(
  FrameProbe p,
  Iterable<String> names, {
  String page = 'page',
}) {
  for (final name in names) {
    final faded = [
      for (var i = 0; i < p.frames.length; i++)
        if (p.frames[i][name]!.rect != null &&
            p.frames[i][page]!.opacity > 0.01 &&
            p.frames[i][name]!.opacity / p.frames[i][page]!.opacity < 0.99)
          '$i:${(p.frames[i][name]!.opacity / p.frames[i][page]!.opacity).toStringAsFixed(2)}',
    ];
    expect(
      faded,
      isEmpty,
      reason:
          '"$name" was not opaque in the chat on frames (frame:opacity) $faded',
    );
  }
}

/// [name] never moves inside the page (the route's slide is divided out) from its first frame.
void _expectStillInPage(
  FrameProbe p,
  Iterable<String> names, {
  double tol = 0.5,
  String page = 'page',
}) {
  for (final name in names) {
    Offset? first;
    final moved = <String>[];
    for (var i = 0; i < p.frames.length; i++) {
      final r = p.frames[i][name]!.rect;
      final pr = p.frames[i][page]!.rect;
      if (r == null || pr == null) continue;
      final at = r.topLeft - pr.topLeft;
      first ??= at;
      if ((at - first).distance > tol) moved.add('$i:${(at - first)}');
    }
    expect(
      moved,
      isEmpty,
      reason: '"$name" moved inside the chat on frames (frame:delta) $moved',
    );
  }
}

const _long = Duration(milliseconds: 1200);

/// How many times the direction of [steps] flips (ignoring rests).
int _bounces(List<double> steps) {
  var flips = 0;
  double last = 0;
  for (final d in steps) {
    if (d.abs() < 0.5) continue;
    if (last != 0 && d.sign != last.sign) flips++;
    last = d;
  }
  return flips;
}

void main() {
  testWidgets(
    'first open: the history that arrives is opaque and still, at the end, '
    'from the first frame it exists',
    (t) async {
      final server = _Server()..tasksDelay = const Duration(milliseconds: 500);
      final rig = await _launch(t, server);

      await rig.openChat();
      final names = _tracked().keys.where((k) => k != 'page');
      final probe = FrameProbe(t, _tracked());
      await probe.record(_long, sample: {'offset': rig.offset});

      // The history did land during the recording.
      expect(probe.visibleFrames('q39'), isNotEmpty);
      _expectOpaqueInPage(probe, names);
      _expectStillInPage(probe, names);
      probe.expectSampleWithin('offset', 0, 0);
    },
  );

  testWidgets(
    'reopening a chat: the thread is on the first frame, complete and at the end',
    (t) async {
      final server = _Server();
      final rig = await _launch(t, server);
      await rig.openChat();
      await settle(t);
      expect(find.text('question 39'), findsOneWidget);
      await rig.back();
      await settle(t);
      expect(find.byType(CrewmateChatScreen), findsNothing);

      await rig.openChat();
      final names = _tracked().keys.where((k) => k != 'page');
      final probe = FrameProbe(t, _tracked());
      await probe.record(_long, sample: {'offset': rig.offset});

      // The first frame the chat exists on (the route's own push may need a frame to install it) already has
      // the whole thread: no spinner, then the thread.
      final first = probe.frames.firstWhere((f) => f['page']!.rect != null);
      expect(
        first['q39']!.rect,
        isNotNull,
        reason: 'the thread was not there on the first frame of the chat (a spinner, then the thread)',
      );
      _expectOpaqueInPage(probe, names);
      _expectStillInPage(probe, names);
      probe.expectSampleWithin('offset', 0, 0);
    },
  );

  group('a chat that is open', () {
    /// Opens the chat, lets it settle, and returns the rig.
    Future<_Rig> openSettled(WidgetTester t, _Server server) async {
      final rig = await _launch(t, server);
      await rig.openChat();
      await settle(t);
      expect(find.text('question 39'), findsOneWidget);
      return rig;
    }

    /// The prompts of the thread that are on screen right now, as probe names.
    Map<String, Finder> onScreen(WidgetTester t) => {
      for (var n = 39; n >= 0; n--)
        if (find.text('question $n').evaluate().isNotEmpty &&
            t.getRect(find.text('question $n')).bottom < 852 - 120 &&
            t.getRect(find.text('question $n')).top > _statusBar + 64)
          'q$n': find.text('question $n'),
    };

    testWidgets(
      'sending a message never moves the reader off the end, through queued, working and done',
      (t) async {
        final server = _Server();
        final rig = await openSettled(t, server);

        await t.enterText(find.byType(TextField), 'ship it');
        await t.pump();
        await t.pump(
          const Duration(milliseconds: 600),
        ); // the send dot buds out
        await t.tap(find.bySemanticsLabel('Send'));
        // hostd moves the task on while the user watches: running after 300 ms, done after 900 ms.
        Timer(
          const Duration(milliseconds: 300),
          () => server.advanceNewest('running'),
        );
        Timer(
          const Duration(milliseconds: 900),
          () => server.advanceNewest(
            'done',
            result: 'sent reply\n\nsecond paragraph\n\nthird',
          ),
        );
        final probe = FrameProbe(t, {
          'page': _Rig.page,
          'old': find.text('question 39'),
        });
        await probe.record(
          const Duration(milliseconds: 2000),
          sample: {'offset': rig.offset},
        );

        expect(find.text('ship it'), findsOneWidget);
        expect(find.text('sent reply'), findsOneWidget);
        probe.expectSampleWithin('offset', 0, 0);
        // What was already read makes room for what arrives, a little per frame; it never jumps a bubble in one
        // frame, nor away and back.
        final tops = [
          for (final f in probe.frames)
            if (f['old']!.rect != null) f['old']!.rect!.top,
        ];
        expect(tops, isNotEmpty);
        final steps = [
          for (var i = 1; i < tops.length; i++) tops[i] - tops[i - 1],
        ];
        expect(
          steps.where((d) => d.abs() > 40),
          isEmpty,
          reason: 'the thread jumped (px per frame): $steps',
        );
        // Room is only made upwards while the new messages grow; the one place it may come back down is when the
        // reply settles shorter than its "Working" form, which is a smooth ease, never a bounce.
        final ups = [
          for (var i = 1; i < tops.length; i++) tops[i] < tops[i - 1] - 0.5,
        ];
        expect(
          _bounces(steps),
          lessThanOrEqualTo(1),
          reason: 'the thread went up and down more than once: $steps',
        );
        expect(ups, isNotEmpty);
      },
    );

    testWidgets(
      'a reply that arrives while the reader is at the end keeps them at the end',
      (t) async {
        final server = _Server();
        final rig = await openSettled(t, server);
        server.addFromDesktop('from the desktop');
        final probe = FrameProbe(t, {
          'page': _Rig.page,
          'new': find.text('from the desktop'),
        });
        await probe.record(_long, sample: {'offset': rig.offset});
        expect(find.text('from the desktop'), findsOneWidget);
        probe.expectSampleWithin('offset', 0, 0);
        // It arrives: it fades in (only it), and is whole within half a second.
        final seen = [for (final f in probe.frames) f['new']!.opacity];
        expect(
          seen.first,
          lessThan(0.5),
          reason: 'a message that arrives is animated in: $seen',
        );
        expect(seen.last, closeTo(1, 0.01));
        expect(
          seen.indexWhere((o) => o > 0.99) * 16,
          lessThanOrEqualTo(500),
          reason: 'the arrival takes at most half a second: $seen',
        );
        expect(t.getRect(find.text('from the desktop')).bottom, lessThan(852));
      },
    );

    testWidgets(
      'a message that arrives while the reader is scrolled up does not move what they read',
      (t) async {
        final server = _Server();
        final rig = await openSettled(t, server);
        rig.controller!.jumpTo(700);
        await t.pump();
        final read = onScreen(t);
        expect(read, isNotEmpty);
        server.addFromDesktop('from the desktop');
        final probe = FrameProbe(t, {'page': _Rig.page, ...read});
        await probe.record(_long);
        _expectStillInPage(probe, read.keys);
      },
    );

    testWidgets(
      'a task that changes below the reader does not move what they read',
      (t) async {
        final server = _Server();
        final rig = await openSettled(t, server);
        rig.controller!.jumpTo(700);
        await t.pump();
        final read = onScreen(t);
        expect(read, isNotEmpty);
        Timer(
          const Duration(milliseconds: 100),
          () => server.advanceNewest('running'),
        );
        Timer(
          const Duration(milliseconds: 600),
          () => server.advanceNewest(
            'done',
            result: List.generate(12, (i) => 'long reply $i').join('\n\n'),
          ),
        );
        final probe = FrameProbe(t, {'page': _Rig.page, ...read});
        await probe.record(const Duration(milliseconds: 1600));
        _expectStillInPage(probe, read.keys);
      },
    );

    testWidgets(
      'the header has a background from the status bar down, so the thread never draws under the clock',
      (t) async {
        final server = _Server();
        final rig = await openSettled(t, server);
        rig.controller!.jumpTo(900);
        await t.pump(const Duration(milliseconds: 100));

        final bg = find.byKey(const ValueKey('chat-header-background'));
        expect(bg, findsOneWidget);
        final r = t.getRect(bg);
        expect(
          r.top,
          0,
          reason: 'the background must start at the top of the screen',
        );
        expect(r.width, 393);
        // It covers the status bar and the whole header row (back button, pill: 64 dp).
        expect(r.bottom, greaterThanOrEqualTo(_statusBar + 64));
        // Message text really is in the zone under the header (so the test means something).
        final under = [
          for (var n = 39; n >= 0; n--)
            if (find.text('question $n').evaluate().isNotEmpty &&
                t.getRect(find.text('question $n')).top < r.bottom)
              n,
        ];
        expect(
          under,
          isNotEmpty,
          reason: 'scrolled far enough that text is under the header',
        );
        // And the background is a real, nearly opaque ground, blurred behind.
        final glass = t.widget<DecoratedBox>(
          find.descendant(of: bg, matching: find.byType(DecoratedBox)).first,
        );
        final color = (glass.decoration as BoxDecoration).color!;
        expect(color.a, greaterThanOrEqualTo(0.85));
        expect(
          find.descendant(of: bg, matching: find.byType(BackdropFilter)),
          findsOneWidget,
        );

        await expectLater(
          find.byType(CrewmateChatScreen),
          matchesGoldenFile('goldens/crew_chat_scrolled_under_header.png'),
        );
      },
    );

    testWidgets('opening the profile and coming back flashes nothing', (
      t,
    ) async {
      final server = _Server();
      final rig = await openSettled(t, server);
      final names = _tracked().keys.where((k) => k != 'page');

      await t.tap(
        find.descendant(of: _Rig.page, matching: find.text('Reviewer')).first,
      );
      final profile = find.byType(CrewmateProfileScreen);
      final opening = FrameProbe(t, {
        'page': profile,
        'name': find.descendant(of: profile, matching: find.text('Reviewer')),
      });
      await opening.record(const Duration(milliseconds: 700));
      expect(opening.visibleFrames('name'), isNotEmpty);
      _expectOpaqueInPage(opening, ['name']);
      _expectStillInPage(opening, ['name']);

      await t.tap(find.byIcon(Icons.arrow_back_ios_new_rounded).last);
      final closing = FrameProbe(t, _tracked());
      await closing.record(
        const Duration(milliseconds: 700),
        sample: {'offset': rig.offset},
      );
      expect(closing.frames.first['q39']!.rect, isNotNull);
      _expectOpaqueInPage(closing, names);
      _expectStillInPage(closing, names);
      closing.expectSampleWithin('offset', 0, 0);
    });

    testWidgets(
      'going back to the list shows its rows complete on every frame',
      (t) async {
        final server = _Server();
        final rig = await openSettled(t, server);
        await rig.back();
        final row = find.descendant(
          of: find.byType(CrewScreen),
          matching: find.text('Reviewer'),
        );
        final probe = FrameProbe(t, {
          'page': find.byType(CrewScreen),
          'row': row.first,
        });
        await probe.record(const Duration(milliseconds: 700));
        expect(probe.frames.first['row']!.rect, isNotNull);
        _expectOpaqueInPage(probe, ['row']);
        _expectStillInPage(probe, ['row']);
      },
    );
  });
}
