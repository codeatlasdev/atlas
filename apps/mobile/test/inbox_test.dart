// The Inbox replaced the Crew tab's red dot, which fired on `state == 'needs_you'`,
// while the Inbox counted only bots with a pending approval. These pin that a
// crewmate shows up on either signal, so nothing the old dot showed is lost.
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/features/inbox/inbox.dart';

const _mac = MacDevice(nodeId: 'n1', name: 'mac');

BotListEntry _bot(String id, String state, {BotPendingApproval? pending}) =>
    BotListEntry(
      id: id,
      name: id,
      role: '',
      agent: 'omp',
      approval: 'ask',
      state: state,
      pendingApproval: pending,
    );

AgentStatus _agent(String state) =>
    AgentStatus(state: state, agent: 'omp', updatedAt: 0, stateStartedAt: 0);

List<Attention> _inbox({
  List<ProjectInfo> projects = const [],
  List<BotListEntry> bots = const [],
}) => attentionFor(_mac, projects: projects, live: const {}, bots: bots);

void main() {
  test(
    'a crewmate that says it needs you is in the inbox without an approval',
    () {
      final items = _inbox(bots: [_bot('scout', 'needs_you')]);
      expect(items.single, isA<BotAttention>());
      expect((items.single as BotAttention).bot.id, 'scout');
    },
  );

  test(
    'a crewmate with a pending approval is in the inbox whatever its state',
    () {
      final pending = BotPendingApproval.fromJson({
        'pane_key': 'p',
        'req': 'r',
        'tool_name': 'bash',
      });
      final items = _inbox(bots: [_bot('deploy', 'working', pending: pending)]);
      expect((items.single as BotAttention).bot.id, 'deploy');
    },
  );

  test('idle and working crewmates are not in the inbox', () {
    expect(_inbox(bots: [_bot('a', 'idle'), _bot('b', 'working')]), isEmpty);
  });

  test('someone waiting comes before something broken', () {
    final project = ProjectInfo(
      id: 'p1',
      root: '/p',
      name: 'api',
      running: true,
      services: [
        ServiceInfo.fromJson({'name': 'db', 'state': 'failed'}),
      ],
      agents: [('pane1', _agent('blocked')), ('pane2', _agent('working'))],
    );
    final items = _inbox(
      projects: [project],
      bots: [_bot('scout', 'needs_you')],
    );
    expect(items.map((i) => i.runtimeType), [
      AgentAttention,
      BotAttention,
      ServiceAttention,
    ]);
  });

  test("a stopped project's failed service is not reported", () {
    final project = ProjectInfo(
      id: 'p1',
      root: '/p',
      name: 'api',
      running: false,
      services: [
        ServiceInfo.fromJson({'name': 'db', 'state': 'failed'}),
      ],
      agents: const [],
    );
    expect(_inbox(projects: [project]), isEmpty);
  });

  // The Inbox only listed what waits on you; a crewmate finishing never showed up
  // (2026-11-12). Recent activity: finished/failed tasks and agents that are done.
  group('recent activity', () {
    final now = DateTime(2026, 11, 12, 18);
    int sec(DateTime d) => d.millisecondsSinceEpoch ~/ 1000;
    BotListEntry ended(
      String id,
      String state,
      DateTime at, {
      String bot = 'idle',
    }) => BotListEntry.fromJson({
      'id': id,
      'name': id,
      'state': bot,
      'last': {'text': 'ok', 'state': state, 'ts': sec(at)},
    });

    test(
      'a finished or failed task is news, newest first in the provider order',
      () {
        final items = activityFor(
          _mac,
          projects: const [],
          live: const {},
          bots: [
            ended('a', 'done', now.subtract(const Duration(minutes: 5))),
            ended('b', 'failed', now.subtract(const Duration(hours: 2))),
          ],
          now: now,
        );
        expect(items.map((i) => (i as CrewActivity).failed), [false, true]);
        expect(items.first.at, now.subtract(const Duration(minutes: 5)));
      },
    );

    test(
      'old news, and crewmates still busy or waiting on you, are not recent',
      () {
        final items = activityFor(
          _mac,
          projects: const [],
          live: const {},
          bots: [
            ended('old', 'done', now.subtract(const Duration(days: 8))),
            ended('busy', 'done', now, bot: 'working'),
            ended('asks', 'done', now, bot: 'needs_you'),
          ],
          now: now,
        );
        expect(items, isEmpty);
      },
    );

    test('an agent that is done is news, timed by state_started_at (ms)', () {
      final at = now.subtract(const Duration(minutes: 3));
      final p = ProjectInfo(
        id: 'p',
        root: '/p',
        name: 'api',
        running: true,
        services: const [],
        agents: [
          (
            'pane',
            AgentStatus(
              state: 'done',
              agent: 'omp',
              updatedAt: 0,
              stateStartedAt: at.millisecondsSinceEpoch,
            ),
          ),
        ],
      );
      final items = activityFor(
        _mac,
        projects: [p],
        live: const {},
        bots: const [],
        now: now,
      );
      expect(items.single, isA<AgentActivity>());
      expect(items.single.at, at);
    });

    test('unread is what came after the last visit', () {
      final items = activityFor(
        _mac,
        projects: const [],
        live: const {},
        bots: [
          ended('a', 'done', now.subtract(const Duration(minutes: 5))),
          ended('b', 'done', now.subtract(const Duration(hours: 2))),
        ],
        now: now,
      );
      expect(unreadCount(items, null), 2);
      expect(unreadCount(items, now.subtract(const Duration(hours: 1))), 1);
      expect(unreadCount(items, now), 0);
    });
  });
}
