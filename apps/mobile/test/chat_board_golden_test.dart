/// Golden screenshots: chat with a running task (2 step groups) + add_trigger card,
/// and the board with cards in every column.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/bots/board.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';

// ── helpers ───────────────────────────────────────────────────────────────────

class _FakeStorage extends MacStorage {
  _FakeStorage() : super(storage: null);
  @override
  Future<List<MacDevice>> loadDevices() async => const [];
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

Widget _frame(Widget child) => ProviderScope(
  overrides: [
    macStorageProvider.overrideWithValue(_FakeStorage()),
    transportFactoryProvider.overrideWith((_) => throw UnimplementedError()),
  ],
  child: MaterialApp(
    theme: atlasTheme(),
    home: Scaffold(body: child),
    debugShowCheckedModeBanner: false,
  ),
);

// ── fixtures ──────────────────────────────────────────────────────────────────

final _now = DateTime(2026, 10, 30, 14, 0, 0).millisecondsSinceEpoch;

final _runningTask = BotTask(
  id: 'task-r',
  ts: _now - 120000,
  source: 'mobile',
  prompt: 'Review PR #118 — auth middleware refactor',
  state: 'running',
  startedAt: _now - 90000,
  title: 'REVIEW: Auth middleware',
  steps: [
    BotStep(ts: _now - 80000, text: 'Read src/middleware/auth.ts'),
    BotStep(
      ts: _now - 75000,
      text: 'Read src/middleware/auth.test.ts',
      group: 'REVIEW: Auth middleware',
    ),
    BotStep(
      ts: _now - 60000,
      text: 'Read src/routes/user.ts',
      group: 'REVIEW: Auth middleware',
    ),
    BotStep(
      ts: _now - 45000,
      text: 'Bash bun test --filter auth',
      group: 'REVIEW: Auth middleware',
    ),
    BotStep(
      ts: _now - 20000,
      text: 'Grep handleLogin',
      group: 'FIX: Fix token validation',
    ),
    BotStep(
      ts: _now - 10000,
      text: 'Edit src/middleware/auth.ts',
      group: 'FIX: Fix token validation',
    ),
    BotStep(
      ts: _now - 5000,
      text: 'Read src/types.ts',
      group: 'FIX: Fix token validation',
    ),
  ],
);

final _addTriggerTask = BotTask(
  id: 'task-t',
  ts: _now - 60000,
  source: 'mobile',
  prompt: 'Schedule a daily review',
  state: 'waiting_action',
  action: TaskAction.fromJson({
    'kind': 'add_trigger',
    'prompt': 'Review open PRs and leave comments',
    'reason': 'You asked for a daily review',
    'trigger': {'type': 'schedule', 'cron': '0 9 * * 1-5'},
  }),
);

// ── chat golden ───────────────────────────────────────────────────────────────

class _ChatFixture extends StatelessWidget {
  const _ChatFixture();

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final avatarColor = const Color(0xFF26A69A);
    const initial = 'R';

    return Container(
      color: c.ground,
      child: ListView(
        reverse: true,
        padding: const EdgeInsets.all(12),
        children: [
          // add_trigger card (most recent)
          _TaskPairStub(
            task: _addTriggerTask,
            avatarColor: avatarColor,
            initial: initial,
            botName: 'Reviewer',
          ),
          const SizedBox(height: 8),
          // running task with 2 groups
          _TaskPairStub(
            task: _runningTask,
            avatarColor: avatarColor,
            initial: initial,
            botName: 'Reviewer',
          ),
        ],
      ),
    );
  }
}

// Simplified _TaskPair for golden — mirrors the real one but without providers.
class _TaskPairStub extends StatelessWidget {
  const _TaskPairStub({
    required this.task,
    required this.avatarColor,
    required this.initial,
    required this.botName,
  });

  final BotTask task;
  final Color avatarColor;
  final String initial;
  final String botName;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    const r = Radius.circular(18);

    Widget botContent;
    if (task.state == 'waiting_action' && task.action?.kind == 'add_trigger') {
      final action = task.action!;
      botContent = Container(
        constraints: BoxConstraints(
          maxWidth: MediaQuery.of(context).size.width * 0.82,
        ),
        padding: const EdgeInsets.all(12),
        decoration: BoxDecoration(
          color: c.warning.withValues(alpha: .08),
          border: Border.all(color: c.warning.withValues(alpha: .35)),
          borderRadius: const BorderRadius.only(
            topLeft: Radius.circular(4),
            topRight: r,
            bottomLeft: r,
            bottomRight: r,
          ),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(Icons.schedule_outlined, size: 14, color: c.warning),
                const SizedBox(width: 6),
                Flexible(
                  child: Text(
                    '$botName wants to add an automation',
                    style: TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                      color: c.warning,
                    ),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8),
            Text(
              'Schedule: 0 9 * * 1-5',
              style: TextStyle(fontSize: 13, color: c.text),
            ),
            const SizedBox(height: 2),
            Text(
              action.triggerPrompt ?? '',
              style: TextStyle(fontSize: 12, color: c.muted),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
            ),
            const SizedBox(height: 2),
            Text(
              action.reason ?? '',
              style: TextStyle(fontSize: 12, color: c.muted),
            ),
            const SizedBox(height: 6),
            Row(
              children: [
                FilledButton(
                  onPressed: () {},
                  style: FilledButton.styleFrom(
                    backgroundColor: c.warning,
                    foregroundColor: c.ground,
                    padding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 6,
                    ),
                    minimumSize: Size.zero,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(20),
                    ),
                  ),
                  child: const Text('Add'),
                ),
                const SizedBox(width: 8),
                OutlinedButton(
                  onPressed: () {},
                  style: OutlinedButton.styleFrom(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 6,
                    ),
                    minimumSize: Size.zero,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(20),
                    ),
                  ),
                  child: const Text('Not now', style: TextStyle(fontSize: 13)),
                ),
              ],
            ),
          ],
        ),
      );
    } else {
      // running task with grouped steps
      botContent = Container(
        constraints: BoxConstraints(
          maxWidth: MediaQuery.of(context).size.width * 0.72,
        ),
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
        decoration: BoxDecoration(
          color: c.base,
          border: Border.all(color: c.borderSubtle),
          borderRadius: const BorderRadius.only(
            topLeft: Radius.circular(4),
            topRight: r,
            bottomLeft: r,
            bottomRight: r,
          ),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                SizedBox.square(
                  dimension: 12,
                  child: CircularProgressIndicator(
                    strokeWidth: 2,
                    color: c.accent,
                  ),
                ),
                const SizedBox(width: 8),
                Text(
                  'Working · 1m 30s',
                  style: TextStyle(color: c.accent, fontSize: 14),
                ),
              ],
            ),
            const SizedBox(height: 6),
            // Finished group 1: folded
            Row(
              children: [
                Icon(Icons.check_circle_outline, size: 13, color: c.success),
                const SizedBox(width: 4),
                Flexible(
                  child: Text(
                    'REVIEW: Auth middleware · 3 steps',
                    style: TextStyle(fontSize: 12, color: c.muted),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                const SizedBox(width: 2),
                Icon(Icons.keyboard_arrow_down, size: 14, color: c.faint),
              ],
            ),
            const SizedBox(height: 4),
            // Current group 2: open
            Text(
              'FIX: Fix token validation',
              style: TextStyle(
                fontSize: 12,
                fontWeight: FontWeight.w600,
                color: c.muted,
              ),
            ),
            const SizedBox(height: 4),
            for (final step in [
              'Searching for "handleLogin"',
              'Editing auth.ts',
              'Reading types.ts',
            ])
              Padding(
                padding: const EdgeInsets.only(bottom: 2),
                child: Text(
                  '· $step',
                  style: TextStyle(
                    fontSize: 12,
                    color: c.muted,
                    fontFamily: 'Menlo',
                    fontFamilyFallback: const [
                      'SF Mono',
                      'RobotoMono',
                      'monospace',
                    ],
                  ),
                ),
              ),
          ],
        ),
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        // Timestamp
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 8),
          child: Center(
            child: Text(
              '14:00',
              style: TextStyle(fontSize: 11, color: c.faint),
            ),
          ),
        ),
        // User bubble
        Align(
          alignment: Alignment.centerRight,
          child: Container(
            constraints: BoxConstraints(
              maxWidth: MediaQuery.of(context).size.width * 0.75,
            ),
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
            decoration: BoxDecoration(
              color: c.accent.withValues(alpha: .22),
              borderRadius: const BorderRadius.only(
                topLeft: r,
                topRight: r,
                bottomLeft: r,
                bottomRight: Radius.circular(4),
              ),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                Text(
                  task.prompt,
                  style: TextStyle(color: c.text, height: 1.4, fontSize: 14),
                ),
                const SizedBox(height: 3),
                Text('mobile', style: TextStyle(fontSize: 10, color: c.muted)),
              ],
            ),
          ),
        ),
        const SizedBox(height: 4),
        // Bot bubble
        Row(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            CircleAvatar(
              radius: 14,
              backgroundColor: avatarColor.withValues(alpha: .22),
              child: Text(
                initial,
                style: TextStyle(
                  color: avatarColor,
                  fontSize: 11,
                  fontWeight: FontWeight.w700,
                ),
              ),
            ),
            const SizedBox(width: 8),
            Flexible(child: botContent),
          ],
        ),
        const SizedBox(height: 16),
      ],
    );
  }
}

// ── board golden ──────────────────────────────────────────────────────────────

class _BoardFixture extends StatelessWidget {
  const _BoardFixture();

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final now = DateTime(2026, 10, 30, 14, 0, 0).millisecondsSinceEpoch;

    final bots = [
      BotListEntry(
        id: 'b1',
        name: 'Reviewer',
        role: '',
        agent: 'omp',
        approval: 'ask',
        state: 'working',
      ),
      BotListEntry(
        id: 'b2',
        name: 'Nightly',
        role: '',
        agent: 'omp',
        approval: 'ask',
        state: 'idle',
      ),
      BotListEntry(
        id: 'b3',
        name: 'Deployer',
        role: '',
        agent: 'omp',
        approval: 'ask',
        state: 'idle',
      ),
      BotListEntry(
        id: 'b4',
        name: 'Monitor',
        role: '',
        agent: 'omp',
        approval: 'ask',
        state: 'idle',
      ),
    ];

    final entries = [
      BoardEntry(
        hostKey: 'demo',
        bot: bots[0],
        task: BotTask(
          id: 't1',
          ts: now - 90000,
          source: 'mobile',
          prompt: 'Review PR #118',
          state: 'running',
          title: 'REVIEW: Auth middleware refactor',
          startedAt: now - 90000,
          steps: [
            BotStep(
              ts: now - 80000,
              text: 'Read auth.ts',
              group: 'REVIEW: Auth middleware refactor',
            ),
          ],
        ),
      ),
      BoardEntry(
        hostKey: 'demo',
        bot: bots[1],
        task: BotTask(
          id: 't2',
          ts: now - 30000,
          source: 'trigger',
          prompt: 'Run nightly checks',
          state: 'queued',
        ),
      ),
      BoardEntry(
        hostKey: 'demo',
        bot: bots[2],
        task: BotTask(
          id: 't3',
          ts: now - 120000,
          source: 'desktop',
          prompt: 'Schedule daily review',
          state: 'waiting_action',
          title: 'SCHEDULE: Add daily review trigger',
          action: TaskAction.fromJson({
            'kind': 'add_trigger',
            'prompt': 'Review PRs daily',
            'trigger': {'type': 'schedule', 'cron': '0 9 * * 1-5'},
          }),
        ),
      ),
      BoardEntry(
        hostKey: 'demo',
        bot: bots[3],
        task: BotTask(
          id: 't4',
          ts: now - 7200000,
          source: 'trigger',
          prompt: 'Check for dependency updates',
          state: 'done',
          title: 'DEPS: Dependency audit',
          startedAt: now - 7200000,
          finishedAt: now - 7140000,
          result: 'All dependencies up to date.',
        ),
      ),
    ];

    final cols = boardColumns(
      entries,
      now,
      opts: const BoardFilterOptions(doneDays: 7),
    );

    return Container(
      color: c.ground,
      child: Column(
        children: [
          // Column tab bar
          Container(
            color: c.base,
            child: Row(
              children: [
                for (var i = 0; i < cols.length; i++)
                  Expanded(
                    child: Container(
                      padding: const EdgeInsets.symmetric(vertical: 10),
                      decoration: BoxDecoration(
                        border: Border(
                          bottom: BorderSide(
                            color: i == 1 ? c.accent : Colors.transparent,
                            width: 2,
                          ),
                        ),
                      ),
                      child: Column(
                        children: [
                          Text(
                            cols[i].label,
                            style: TextStyle(
                              fontSize: 12,
                              fontWeight: FontWeight.w600,
                              color: i == 1 ? c.accent : c.muted,
                            ),
                          ),
                          if (cols[i].cards.isNotEmpty)
                            Text(
                              '${cols[i].cards.length}',
                              style: TextStyle(
                                fontSize: 11,
                                color: i == 1 ? c.accent : c.faint,
                              ),
                            ),
                        ],
                      ),
                    ),
                  ),
              ],
            ),
          ),
          // Show all columns stacked for golden
          Expanded(
            child: ListView(
              padding: const EdgeInsets.all(12),
              children: [
                for (final col in cols)
                  if (col.cards.isNotEmpty) ...[
                    Padding(
                      padding: const EdgeInsets.fromLTRB(0, 8, 0, 4),
                      child: Text(
                        col.label.toUpperCase(),
                        style: TextStyle(
                          fontSize: 10,
                          fontWeight: FontWeight.w700,
                          color: c.faint,
                          letterSpacing: 0.8,
                        ),
                      ),
                    ),
                    for (final card in col.cards)
                      _BoardCardStub(card: card, columnId: col.id),
                  ],
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _BoardCardStub extends StatelessWidget {
  const _BoardCardStub({required this.card, required this.columnId});
  final BoardCard card;
  final ColumnId columnId;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final bot = card.bot;
    const palette = [
      Color(0xFF26A69A),
      Color(0xFFFF7043),
      Color(0xFF7E57C2),
      Color(0xFF42A5F5),
      Color(0xFF66BB6A),
      Color(0xFFEC407A),
      Color(0xFFFFCA28),
      Color(0xFF26C6DA),
    ];
    final hash = bot.id.codeUnits.fold(0, (a, u) => a ^ u);
    final color = palette[hash.abs() % palette.length];
    final initial = bot.name.isNotEmpty ? bot.name[0].toUpperCase() : '?';

    final accent = columnId == ColumnId.needsYou
        ? c.warning
        : columnId == ColumnId.working
        ? c.accent
        : card.failed
        ? c.error
        : null;

    return ClipRRect(
      borderRadius: BorderRadius.circular(12),
      child: Container(
        margin: const EdgeInsets.only(bottom: 8),
        decoration: BoxDecoration(
          color: c.base,
          border: Border.all(color: c.borderSubtle),
          borderRadius: BorderRadius.circular(12),
        ),
        child: IntrinsicHeight(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (accent != null) Container(width: 3, color: accent),
              Expanded(
                child: Padding(
                  padding: const EdgeInsets.all(12),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      CircleAvatar(
                        radius: 18,
                        backgroundColor: color.withValues(alpha: .22),
                        child: Text(
                          initial,
                          style: TextStyle(
                            color: color,
                            fontSize: 14,
                            fontWeight: FontWeight.w700,
                          ),
                        ),
                      ),
                      const SizedBox(width: 10),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              bot.name,
                              style: TextStyle(fontSize: 12, color: c.muted),
                            ),
                            const SizedBox(height: 2),
                            Text(
                              card.title ?? card.task.prompt,
                              style: TextStyle(
                                fontSize: 14,
                                color: c.text,
                                fontWeight: FontWeight.w600,
                              ),
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                            ),
                            if (card.task.state == 'waiting_action') ...[
                              const SizedBox(height: 2),
                              Text(
                                'Waiting for input',
                                style: TextStyle(
                                  fontSize: 12,
                                  color: c.warning,
                                ),
                              ),
                            ] else if (card.task.state == 'running' &&
                                card.task.steps.isNotEmpty) ...[
                              const SizedBox(height: 2),
                              Text(
                                card.task.steps.last.text,
                                style: TextStyle(fontSize: 12, color: c.muted),
                                overflow: TextOverflow.ellipsis,
                              ),
                            ],
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

// ── tests ─────────────────────────────────────────────────────────────────────

void main() {
  testWidgets('chat: running task with 2 step groups and add_trigger card', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(393, 852));
    await tester.pumpWidget(_frame(const _ChatFixture()));
    await tester.pump(const Duration(milliseconds: 100));
    await expectLater(
      find.byType(_ChatFixture),
      matchesGoldenFile('goldens/chat_running_task_and_add_trigger.png'),
    );
    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(milliseconds: 100));
  });

  testWidgets('board: cards in every column', (tester) async {
    await tester.binding.setSurfaceSize(const Size(393, 852));
    await tester.pumpWidget(_frame(const _BoardFixture()));
    await tester.pump(const Duration(milliseconds: 100));
    await expectLater(
      find.byType(_BoardFixture),
      matchesGoldenFile('goldens/board_all_columns.png'),
    );
    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(milliseconds: 100));
  });
}
