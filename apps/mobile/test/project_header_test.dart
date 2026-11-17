// Product review 2026-11-15 (screenshot of a stopped project): the state pill and a large Start floated side by
// side above the numbers, and a stopped project showed its "0/6 running" in alarm red. The header is now one
// status panel: the state, its one action and the numbers. These pin what a person relies on.
import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/projects/detail/header.dart';
import 'package:atlas_mobile/features/projects/project_metrics.dart';

import 'support/layout_harness.dart';

ProjectSnapshot _snap({required bool running, int services = 6}) =>
    ProjectSnapshot(
      id: 'p1',
      name: 'analisa-ai',
      root: '/Users/me/Projetos/codeatlasdev/analisa-ai',
      running: running,
      pid: running ? 4821 : null,
      services: [
        for (var i = 0; i < services; i++)
          ServiceRow(name: 'svc$i', state: running ? 'running' : 'stopped'),
      ],
    );

// The project screen is a scroll view: the header is as tall as it needs, the page scrolls.
Widget _header(ProjectSnapshot s, {VoidCallback? onUp}) => Scaffold(
  body: SingleChildScrollView(
    padding: const EdgeInsets.all(16),
    child: ProjectHeader(snap: s, onUp: onUp),
  ),
);

void main() {
  testWidgets('stopped: says so with its services, and Start starts it', (
    t,
  ) async {
    var started = 0;
    await t.pumpWidget(
      appAround(_header(_snap(running: false), onUp: () => started++)),
    );
    expect(find.text('Stopped'), findsOneWidget);
    expect(find.text('6 services, none running'), findsOneWidget);
    await t.tap(find.widgetWithText(FilledButton, 'Start'));
    expect(started, 1);
  });

  testWidgets(
    'stopped is not an alarm: 0/6 is not red, memory and CPU are a dash',
    (t) async {
      await t.pumpWidget(appAround(_header(_snap(running: false))));
      final c = AtlasColors.dark;
      final running = t.widget<Text>(find.text('0/6'));
      expect(running.style?.color, isNot(c.error));
      expect(find.text('—'), findsNWidgets(2));
      expect(find.text('0 MB'), findsNothing);
    },
  );

  testWidgets('a start in flight shows Starting… and cannot be tapped again', (
    t,
  ) async {
    await t.pumpWidget(appAround(_header(_snap(running: false))));
    final b = t.widget<FilledButton>(
      find.byWidgetPredicate((w) => w is FilledButton),
    );
    expect(b.onPressed, isNull);
    expect(find.text('Starting…'), findsOneWidget);
  });

  testWidgets('running: no Start, uptime and pid instead', (t) async {
    await t.pumpWidget(appAround(_header(_snap(running: true))));
    expect(find.text('Running'), findsOneWidget);
    expect(find.byType(FilledButton), findsNothing);
    expect(find.textContaining('pid 4821'), findsOneWidget);
  });

  for (final phone in phoneSizes) {
    for (final scale in textScales) {
      testWidgets('fits ${phone.name} at text x$scale', (t) async {
        await withPhone(
          t,
          size: phone.size,
          scale: scale,
          body: () async {
            await t.pumpWidget(
              appAround(_header(_snap(running: false), onUp: () {})),
            );
            await t.pump();
            expect(t.takeException(), isNull);
            final start = t.getRect(find.widgetWithText(FilledButton, 'Start'));
            expect(start.right, lessThanOrEqualTo(phone.size.width));
          },
        );
      });
    }
  }
}
