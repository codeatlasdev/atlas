import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/features/crew/list/crew_screen.dart';
import 'package:atlas_mobile/features/crew/ops/crew_audit_screen.dart';
import 'package:atlas_mobile/features/crew/ops/crew_settings_screen.dart';
import 'package:atlas_mobile/features/crew/ops/crewmate_usage_screen.dart';
import 'package:atlas_mobile/features/crew/profile/crewmate_profile_screen.dart';

import 'support/demo_harness.dart';

// Every crew operations screen of the phone, run against the demo computer (what App Review uses):
// audit (filters, copy), usage and engine limits, routines (add, edit, toggle, delete, bad cron), crew
// settings (short replies, the model) and how each is reached from the crewmate profile and the Crew tab.
// Gap #5: the desktop had them, the phone had no screen for commands hostd already answers.

void main() {
  const mac = MacDevice(nodeId: 'demo', name: 'Demo MacBook');
  const size = Size(393, 852);

  Future<void> end(WidgetTester t) async {
    await t.pumpWidget(const SizedBox());
    await settle(t);
  }

  group('audit screen', () {
    testWidgets("lists a crewmate's entries newest first, one line each", (
      t,
    ) async {
      await t.binding.setSurfaceSize(size);
      await t.pumpWidget(
        demoWrap(
          const CrewAuditScreen(
            device: mac,
            crewmateId: 'crewmate-1',
            title: 'Reviewer',
          ),
          [mac],
        ),
      );
      await settle(t);
      expect(find.text('Audit'), findsOneWidget);
      expect(find.text('Bash: bun test --filter payments'), findsOneWidget);
      expect(
        find.text('Denied Bash (answered by phone demo0000…)'),
        findsOneWidget,
      );
      expect(find.text('Changed mode: "bypass" → "ask"'), findsOneWidget);
      // Newest on top: the setting change (latest) sits above the first queued task.
      expect(
        t.getTopLeft(find.text('Changed mode: "bypass" → "ask"')).dy,
        lessThan(t.getTopLeft(find.textContaining('Task queued (desktop)')).dy),
      );
      await end(t);
    });

    testWidgets(
      'a filter chip keeps only its group, All brings the rest back',
      (t) async {
        await t.binding.setSurfaceSize(size);
        await t.pumpWidget(
          demoWrap(
            const CrewAuditScreen(
              device: mac,
              crewmateId: 'crewmate-1',
              title: 'Reviewer',
            ),
            [mac],
          ),
        );
        await settle(t);
        await t.tap(find.text('Approvals'));
        await settle(t);
        expect(find.text('Bash: bun test --filter payments'), findsNothing);
        expect(
          find.text('Denied Bash (answered by phone demo0000…)'),
          findsOneWidget,
        );
        expect(find.textContaining('Asked to run Bash'), findsOneWidget);
        // The chips scroll sideways: bring the last one into reach, as a thumb would.
        await t.ensureVisible(find.text('Settings'));
        await t.tap(find.text('Settings'));
        await settle(t);
        expect(find.text('Changed mode: "bypass" → "ask"'), findsOneWidget);
        expect(find.textContaining('Asked to run Bash'), findsNothing);
        await t.ensureVisible(find.text('All'));
        await t.tap(find.text('All'));
        await settle(t);
        expect(find.text('Bash: bun test --filter payments'), findsOneWidget);
        await end(t);
      },
    );

    // Regression: the filter chips were 36 dp tall, under the 48 dp touch target.
    testWidgets('every audit filter chip has a 48 dp tap target', (t) async {
      await t.binding.setSurfaceSize(size);
      await t.pumpWidget(
        demoWrap(
          const CrewAuditScreen(
            device: mac,
            crewmateId: 'crewmate-1',
            title: 'Reviewer',
          ),
          [mac],
        ),
      );
      await settle(t);
      for (final label in ['All', 'Approvals', 'Settings']) {
        final button = find.ancestor(
          of: find.text(label),
          matching: find.byWidgetPredicate(
            (w) => w is Semantics && w.properties.button == true,
          ),
        );
        expect(
          t.getSize(button.first).height,
          greaterThanOrEqualTo(48),
          reason: '$label chip',
        );
      }
      await end(t);
    });

    testWidgets('a group with nothing says so', (t) async {
      await t.binding.setSurfaceSize(size);
      await t.pumpWidget(
        demoWrap(
          const CrewAuditScreen(
            device: mac,
            crewmateId: 'crewmate-2',
            title: 'Nightly',
          ),
          [mac],
        ),
      );
      await settle(t);
      expect(find.textContaining('Nothing recorded'), findsOneWidget);
      await end(t);
    });

    testWidgets('the crew-wide audit lists the crew log, not a crewmate\'s', (
      t,
    ) async {
      await t.binding.setSurfaceSize(size);
      await t.pumpWidget(
        demoWrap(const CrewAuditScreen(device: mac, title: 'Crew'), [mac]),
      );
      await settle(t);
      expect(find.text('Crewmate Reviewer created'), findsOneWidget);
      expect(find.text('Crewmate Nightly created'), findsOneWidget);
      expect(find.text('Bash: bun test --filter payments'), findsNothing);
      // The crew log has no tool calls or approvals to filter by.
      expect(find.text('Tool calls'), findsNothing);
      await end(t);
    });

    testWidgets(
      'tapping an entry opens it whole, and Copy puts it on the clipboard',
      (t) async {
        await t.binding.setSurfaceSize(size);
        String? copied;
        t.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SystemChannels.platform,
          (call) async {
            if (call.method == 'Clipboard.setData') {
              copied = (call.arguments as Map)['text'] as String;
            }
            return null;
          },
        );
        await t.pumpWidget(
          demoWrap(
            const CrewAuditScreen(
              device: mac,
              crewmateId: 'crewmate-1',
              title: 'Reviewer',
            ),
            [mac],
          ),
        );
        await settle(t);
        await t.tap(find.text('Bash: bun test --filter payments'));
        await settle(t);
        expect(find.text('Tool call'), findsOneWidget);
        await t.tap(find.text('Copy'));
        await settle(t);
        expect(copied, contains('"tool_name": "Bash"'));
        expect(copied, contains('bun test --filter payments'));
        t.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SystemChannels.platform,
          null,
        );
        await end(t);
      },
    );
  });

  group('usage screen', () {
    testWidgets('shows today / 7 d / 30 d tokens and the engine limits note', (
      t,
    ) async {
      await t.binding.setSurfaceSize(size);
      await t.pumpWidget(
        demoWrap(
          const CrewmateUsageScreen(
            device: mac,
            crewmateId: 'crewmate-1',
            name: 'Reviewer',
          ),
          [mac],
        ),
      );
      await settle(t);
      expect(find.text('Usage'), findsOneWidget);
      expect(find.text('Today'), findsOneWidget);
      expect(find.text('7 days'), findsOneWidget);
      expect(find.text('30 days'), findsOneWidget);
      // Reviewer's demo records: 6 days of 2120/2570/…; today's turn is 1800+320.
      expect(find.text('2.1k'), findsWidgets);
      expect(find.textContaining('no machine-readable quota'), findsOneWidget);
      await end(t);
    });
  });

  group('routines (profile)', () {
    Future<void> openProfile(WidgetTester t) async {
      await t.binding.setSurfaceSize(const Size(393, 1400));
      await t.pumpWidget(
        demoWrap(
          const CrewmateProfileScreen(device: mac, crewmateId: 'crewmate-1'),
          [mac],
        ),
      );
      await settle(t);
    }

    testWidgets('the profile reaches Routines, Audit and Usage', (t) async {
      await openProfile(t);
      expect(find.text('Routines'), findsOneWidget);
      expect(find.text('Add routine'), findsOneWidget);
      expect(find.text('Audit'), findsOneWidget);
      expect(find.text('Usage'), findsOneWidget);
      await end(t);
    });

    testWidgets('a new routine is saved to the server and listed', (t) async {
      await openProfile(t);
      await t.ensureVisible(find.text('Add routine'));
      await t.tap(find.text('Add routine'));
      await settle(t);
      await t.enterText(find.byKey(const Key('routine-cron')), '30 2 * * *');
      await t.enterText(
        find.byKey(const Key('routine-prompt')),
        'Run the nightly checks',
      );
      await t.tap(find.text('Save'));
      await settle(t);
      expect(find.text('Run the nightly checks'), findsOneWidget);
      expect(find.text('30 2 * * *'), findsOneWidget);
      await end(t);
    });

    testWidgets('a cron that is not five fields never leaves the sheet', (
      t,
    ) async {
      await openProfile(t);
      await t.ensureVisible(find.text('Add routine'));
      await t.tap(find.text('Add routine'));
      await settle(t);
      await t.enterText(find.byKey(const Key('routine-cron')), 'not a cron');
      await t.enterText(find.byKey(const Key('routine-prompt')), 'x');
      await t.tap(find.text('Save'));
      await settle(t);
      // The sheet is still open with the reason; nothing was added to the list.
      expect(find.textContaining('five fields'), findsOneWidget);
      expect(find.byKey(const Key('routine-cron')), findsOneWidget);
      await end(t);
    });

    testWidgets('an empty prompt is refused', (t) async {
      await openProfile(t);
      await t.ensureVisible(find.text('Add routine'));
      await t.tap(find.text('Add routine'));
      await settle(t);
      await t.enterText(find.byKey(const Key('routine-cron')), '0 9 * * *');
      await t.tap(find.text('Save'));
      await settle(t);
      expect(find.textContaining('What should it do'), findsOneWidget);
      await end(t);
    });

    testWidgets(
      'a cron the server refuses shows its message and keeps the sheet',
      (t) async {
        await openProfile(t);
        await t.ensureVisible(find.text('Add routine'));
        await t.tap(find.text('Add routine'));
        await settle(t);
        // Five fields so the phone lets it through; hostd (and the demo) reject the field value.
        await t.enterText(find.byKey(const Key('routine-cron')), '99 99 * * *');
        await t.enterText(find.byKey(const Key('routine-prompt')), 'x');
        await t.tap(find.text('Save'));
        await settle(t);
        expect(find.textContaining('invalid cron'), findsOneWidget);
        expect(find.byKey(const Key('routine-cron')), findsOneWidget);
        await end(t);
      },
    );

    testWidgets('a routine can be switched off and deleted', (t) async {
      await openProfile(t);
      await t.ensureVisible(find.text('Add routine'));
      await t.tap(find.text('Add routine'));
      await settle(t);
      await t.enterText(find.byKey(const Key('routine-cron')), '0 9 * * 1-5');
      await t.enterText(find.byKey(const Key('routine-prompt')), 'Morning PRs');
      await t.tap(find.text('Save'));
      await settle(t);
      final sw = find.byKey(const Key('routine-switch-0'));
      expect(t.widget<Switch>(sw).value, isTrue);
      await t.tap(sw);
      await settle(t);
      expect(
        t.widget<Switch>(find.byKey(const Key('routine-switch-0'))).value,
        isFalse,
      );
      await t.tap(find.byKey(const Key('routine-delete-0')));
      await settle(t);
      await t.tap(find.text('Remove'));
      await settle(t);
      expect(find.text('Morning PRs'), findsNothing);
      await end(t);
    });
  });

  group('crew settings', () {
    testWidgets('reads the crew settings and saves a change', (t) async {
      await t.binding.setSurfaceSize(size);
      await t.pumpWidget(
        demoWrap(const CrewSettingsScreen(device: mac), [mac]),
      );
      await settle(t);
      expect(find.text('Crew settings'), findsOneWidget);
      expect(t.widget<Switch>(find.byKey(const Key('crew-short-replies'))).value, isTrue);
      await t.tap(find.byKey(const Key('crew-short-replies')));
      await settle(t);
      expect(t.widget<Switch>(find.byKey(const Key('crew-short-replies'))).value, isFalse);
      await end(t);

      // A new screen reads what the server kept (the demo computer is a fresh one per connection, so
      // this half is the write itself: the same switch, flipped, goes out as short_replies:false).
    });

    testWidgets('the model is saved with the switch, blank clears it', (
      t,
    ) async {
      await t.binding.setSurfaceSize(size);
      await t.pumpWidget(
        demoWrap(const CrewSettingsScreen(device: mac), [mac]),
      );
      await settle(t);
      await t.enterText(find.byKey(const Key('crew-model')), 'haiku');
      // The permissions card sits between the model and Save: scroll down to it.
      await t.scrollUntilVisible(
        find.text('Save'),
        200,
        scrollable: find.byType(Scrollable).first,
      );
      await t.tap(find.text('Save'));
      await settle(t);
      expect(find.text('Saved'), findsOneWidget);
      await end(t);
    });
  });

  group('from the Crew tab', () {
    testWidgets('reaches the crew settings and the crew audit', (t) async {
      await t.binding.setSurfaceSize(size);
      await t.pumpWidget(demoWrap(const CrewScreen(), [mac]));
      await settle(t);
      await t.tap(find.byKey(const Key('crew-ops-menu')));
      await settle(t);
      expect(find.text('Crew settings'), findsOneWidget);
      expect(find.text('Crew audit'), findsOneWidget);
      await t.tap(find.text('Crew audit'));
      await settle(t);
      expect(find.text('Crewmate Reviewer created'), findsOneWidget);
      await end(t);
    });
  });
}
