/// Golden screenshot: the board with cards in every column.
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/board.dart';

import 'support/golden_frame.dart';

class _BoardFixture extends StatelessWidget {
  const _BoardFixture();

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final now = DateTime(2026, 10, 30, 14, 0, 0).millisecondsSinceEpoch;

    final crewmates = [
      CrewmateEntry(
        id: 'b1',
        name: 'Reviewer',
        role: '',
        agent: 'omp',
        approval: 'ask',
        state: 'working',
      ),
      CrewmateEntry(
        id: 'b2',
        name: 'Nightly',
        role: '',
        agent: 'omp',
        approval: 'ask',
        state: 'idle',
      ),
      CrewmateEntry(
        id: 'b3',
        name: 'Deployer',
        role: '',
        agent: 'omp',
        approval: 'ask',
        state: 'idle',
      ),
      CrewmateEntry(
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
        crewmate: crewmates[0],
        task: CrewmateTask(
          id: 't1',
          ts: now - 90000,
          source: 'mobile',
          prompt: 'Review PR #118',
          state: 'running',
          title: 'REVIEW: Auth middleware refactor',
          startedAt: now - 90000,
          steps: [
            CrewmateStep(
              ts: now - 80000,
              text: 'Read auth.ts',
              group: 'REVIEW: Auth middleware refactor',
            ),
          ],
        ),
      ),
      BoardEntry(
        hostKey: 'demo',
        crewmate: crewmates[1],
        task: CrewmateTask(
          id: 't2',
          ts: now - 30000,
          source: 'trigger',
          prompt: 'Run nightly checks',
          state: 'queued',
        ),
      ),
      BoardEntry(
        hostKey: 'demo',
        crewmate: crewmates[2],
        task: CrewmateTask(
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
        crewmate: crewmates[3],
        task: CrewmateTask(
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
    final crewmate = card.crewmate;
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
    final hash = crewmate.id.codeUnits.fold(0, (a, u) => a ^ u);
    final color = palette[hash.abs() % palette.length];
    final initial = crewmate.name.isNotEmpty
        ? crewmate.name[0].toUpperCase()
        : '?';

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
                              crewmate.name,
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

void main() {
  testWidgets('board: cards in every column', (tester) async {
    await tester.binding.setSurfaceSize(const Size(393, 852));
    await tester.pumpWidget(goldenFrame(const _BoardFixture()));
    await tester.pump(const Duration(milliseconds: 100));
    await expectLater(
      find.byType(_BoardFixture),
      matchesGoldenFile('goldens/board_all_columns.png'),
    );
    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(milliseconds: 100));
  });
}
