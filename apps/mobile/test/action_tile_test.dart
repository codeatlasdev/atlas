// Product review 2026-11-15 (macOS release check): a project's action rows ended in a loose letter ("x", "m"),
// the desktop's keyboard key for the action, which means nothing on a phone and read as a stray button. A row
// says what the action is and whether it asks first; one tap runs it, and screen readers hear it as one button.
import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/features/projects/detail/action_tile.dart';
import 'package:atlas_mobile/features/projects/project_metrics.dart';

import 'support/layout_harness.dart';

void main() {
  testWidgets('an action row never shows the desktop key', (t) async {
    var ran = 0;
    await t.pumpWidget(
      appAround(
        Scaffold(
          body: ProjectActionTile(
            action: const ProjectAction(
              key: 'x',
              label: 'Seed demo data',
              confirm: 'Wipe and reseed?',
            ),
            onRun: () => ran++,
          ),
        ),
      ),
    );
    expect(find.text('x'), findsNothing);
    expect(find.text('Seed demo data'), findsOneWidget);
    expect(find.text('Asks before running'), findsOneWidget);
    await t.tap(find.text('Seed demo data'));
    expect(ran, 1);
  });

  testWidgets('it is one button named for what it runs', (t) async {
    final h = t.ensureSemantics();
    await t.pumpWidget(
      appAround(
        Scaffold(
          body: ProjectActionTile(
            action: const ProjectAction(key: 'm', label: 'Run migrations'),
            onRun: () {},
          ),
        ),
      ),
    );
    expect(
      t.getSemantics(find.byType(ProjectActionTile)),
      matchesSemantics(
        label: 'Run migrations',
        isButton: true,
        hasTapAction: true,
      ),
    );
    h.dispose();
  });
}
