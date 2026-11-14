/// Golden screenshot: chat with a running task (2 step groups) and the add_trigger card.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';

import 'support/golden_frame.dart';

// ── fixtures ──────────────────────────────────────────────────────────────────

final _now = DateTime(2026, 10, 30, 14, 0, 0).millisecondsSinceEpoch;

final _runningTask = CrewmateTask(
  id: 'task-r',
  ts: _now - 120000,
  source: 'mobile',
  prompt: 'Review PR #118 — auth middleware refactor',
  state: 'running',
  startedAt: _now - 90000,
  title: 'REVIEW: Auth middleware',
  steps: [
    CrewmateStep(ts: _now - 80000, text: 'Read src/middleware/auth.ts'),
    CrewmateStep(
      ts: _now - 75000,
      text: 'Read src/middleware/auth.test.ts',
      group: 'REVIEW: Auth middleware',
    ),
    CrewmateStep(
      ts: _now - 60000,
      text: 'Read src/routes/user.ts',
      group: 'REVIEW: Auth middleware',
    ),
    CrewmateStep(
      ts: _now - 45000,
      text: 'Bash bun test --filter auth',
      group: 'REVIEW: Auth middleware',
    ),
    CrewmateStep(
      ts: _now - 20000,
      text: 'Grep handleLogin',
      group: 'FIX: Fix token validation',
    ),
    CrewmateStep(
      ts: _now - 10000,
      text: 'Edit src/middleware/auth.ts',
      group: 'FIX: Fix token validation',
    ),
    CrewmateStep(
      ts: _now - 5000,
      text: 'Read src/types.ts',
      group: 'FIX: Fix token validation',
    ),
  ],
);

final _addTriggerTask = CrewmateTask(
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
            crewmateName: 'Reviewer',
          ),
          const SizedBox(height: 8),
          // running task with 2 groups
          _TaskPairStub(
            task: _runningTask,
            avatarColor: avatarColor,
            initial: initial,
            crewmateName: 'Reviewer',
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
    required this.crewmateName,
  });

  final CrewmateTask task;
  final Color avatarColor;
  final String initial;
  final String crewmateName;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    const r = Radius.circular(18);

    Widget crewmateContent;
    if (task.state == 'waiting_action' && task.action?.kind == 'add_trigger') {
      final action = task.action!;
      crewmateContent = Container(
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
                    '$crewmateName wants to add an automation',
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
      crewmateContent = Container(
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
        // Crewmate bubble
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
            Flexible(child: crewmateContent),
          ],
        ),
        const SizedBox(height: 16),
      ],
    );
  }
}

void main() {
  testWidgets('chat: running task with 2 step groups and add_trigger card', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(393, 852));
    await tester.pumpWidget(goldenFrame(const _ChatFixture()));
    await tester.pump(const Duration(milliseconds: 100));
    await expectLater(
      find.byType(_ChatFixture),
      matchesGoldenFile('goldens/chat_running_task_and_add_trigger.png'),
    );
    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(milliseconds: 100));
  });
}
