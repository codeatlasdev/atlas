// The permission modes on the phone, end to end against the demo computer (which answers hostd's
// `crewmate_set_approval`, `crew_settings_*` and `agent_approve` the way crates/atlas-host does, coded refusals
// included): the mode in the chat header and the crew list, switching it, Bypass hidden when the crew does not
// allow it, a fixed control for an engine that cannot ask, live updates from another client, the three answers to a
// prompt with an optional comment, the crew settings and the recruit sheet. Every screen is also laid out at the
// largest text and the smallest phone.
import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/permission_mode.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/features/crew/chat/approval_bubble.dart';
import 'package:atlas_mobile/features/crew/chat/crewmate_chat_screen.dart';
import 'package:atlas_mobile/features/crew/list/crew_screen.dart';
import 'package:atlas_mobile/features/crew/new_crewmate_sheet.dart';
import 'package:atlas_mobile/features/crew/ops/crew_settings_screen.dart';
import 'package:atlas_mobile/features/crew/permissions/approval_actions.dart';
import 'package:atlas_mobile/features/crew/permissions/crewmate_mode_control.dart';
import 'package:atlas_mobile/features/crew/permissions/mode_badge.dart';
import 'package:atlas_mobile/features/crew/permissions/mode_button.dart';
import 'package:atlas_mobile/features/crew/permissions/mode_picker.dart';
import 'package:atlas_mobile/features/crew/profile/crewmate_profile_screen.dart';

import 'support/demo_harness.dart';
import 'support/layout_harness.dart' as layout;

const _mac = MacDevice(nodeId: 'demo', name: 'Demo MacBook');

/// The demo computer's transport, recording what the phone sends, and able to play another client (the desktop,
/// Telegram) that sends a command of its own: the demo answers it and pushes what hostd would.
class _Rec implements Transport {
  _Rec(this.inner);
  final Transport inner;
  final sent = <Map<String, dynamic>>[];
  var _other = 9000;

  /// The push that tells the phone a prompt was answered elsewhere has not reached it yet.
  var holdApprovalDone = false;

  @override
  Stream<String> get incomingLines => inner.incomingLines.where(
    (l) => !(holdApprovalDone && l.contains('"agent_approval_done"')),
  );
  @override
  Future<void> sendLine(String line) {
    sent.add(jsonDecode(line) as Map<String, dynamic>);
    return inner.sendLine(line);
  }

  @override
  Future<void> close() => inner.close();

  List<Map<String, dynamic>> calls(String cmd) => [
    for (final m in sent)
      if (m['cmd'] == cmd) (m['args'] as Map).cast<String, dynamic>(),
  ];

  /// Another client's command on the same computer; nothing of it is in [sent].
  Future<void> otherClient(String cmd, Map<String, dynamic> args) =>
      inner.sendLine(jsonEncode({'id': _other++, 'cmd': cmd, 'args': args}));
}

late _Rec _computer;

Transport _dial() => _computer = _Rec(demoTransport());

Future<void> _pump(WidgetTester t, Widget screen, {Size? size}) async {
  await t.binding.setSurfaceSize(size ?? const Size(393, 852));
  addTearDown(() => t.binding.setSurfaceSize(null));
  await t.pumpWidget(demoWrap(screen, const [_mac], transport: _dial));
  await settle(t);
}

Future<void> _unmount(WidgetTester t) async {
  await t.pumpWidget(const SizedBox());
  await t.pump(const Duration(seconds: 1));
}

Widget _chat(String id) => CrewmateChatScreen(device: _mac, crewmateId: id);

/// The mode button of the chat header.
Finder get _modeButton => find.byType(ModeButton);

Finder _inButton(String text) =>
    find.descendant(of: _modeButton, matching: find.text(text));

/// The colour of the label in the header's mode button.
Color? _labelColor(WidgetTester t, String text) =>
    t.widget<Text>(_inButton(text)).style?.color;

void main() {
  const c = AtlasColors.dark;

  group('the mode in the chat header', () {
    testWidgets('shows the mode, and a screen reader hears it', (t) async {
      final handle = t.ensureSemantics();
      await _pump(t, _chat('crewmate-1'));
      expect(_inButton('Ask'), findsOneWidget);
      expect(
        find.bySemanticsLabel('Permission mode: Ask'),
        findsWidgets,
        reason: 'the button says what mode it is in',
      );
      handle.dispose();
      await _unmount(t);
    });

    testWidgets('only Bypass is amber', (t) async {
      await _pump(t, _chat('crewmate-1'));
      final ask = _labelColor(t, 'Ask');
      expect(ask, isNot(c.warning));
      await _computerSwitches(t, 'crewmate-1', 'bypass');
      expect(_inButton('Bypass'), findsOneWidget);
      expect(_labelColor(t, 'Bypass'), c.warning);
      await _computerSwitches(t, 'crewmate-1', 'edits');
      expect(_labelColor(t, 'Accept edits'), isNot(c.warning));
      await _unmount(t);
    });

    testWidgets('a change made by another client shows at once, with no tap', (
      t,
    ) async {
      await _pump(t, _chat('crewmate-1'));
      expect(_inButton('Ask'), findsOneWidget);
      await _computerSwitches(t, 'crewmate-1', 'edits');
      expect(_inButton('Accept edits'), findsOneWidget);
      expect(_inButton('Ask'), findsNothing);
      await _unmount(t);
    });
  });

  group('switching the mode', () {
    testWidgets('the sheet switches it, says so, and the header follows', (
      t,
    ) async {
      await _pump(t, _chat('crewmate-1'));
      await t.tap(_modeButton);
      await settle(t);
      expect(find.text('Permission mode for Reviewer'), findsOneWidget);
      // The three modes, each with what it lets run.
      expect(find.text('Ask'), findsWidgets);
      expect(find.text('Accept edits'), findsOneWidget);
      expect(find.text('Bypass'), findsOneWidget);
      // Reviewer is working: the note about the next tool call is there.
      expect(
        find.textContaining('applies from the next tool call'),
        findsOneWidget,
      );

      await t.tap(find.text('Accept edits'));
      await settle(t);

      expect(_computer.calls('crewmate_set_approval'), [
        {'id': 'crewmate-1', 'mode': 'edits'},
      ]);
      expect(
        find.text(
          'Reviewer is now in Accept edits mode. It applies from the next tool call.',
        ),
        findsOneWidget,
      );
      expect(find.text('Permission mode for Reviewer'), findsNothing);
      expect(_inButton('Accept edits'), findsOneWidget);
      await _unmount(t);
    });

    testWidgets('picking the mode it is already in sends nothing', (t) async {
      await _pump(t, _chat('crewmate-1'));
      await t.tap(_modeButton);
      await settle(t);
      await t.tap(find.text('Ask').last);
      await settle(t);
      expect(_computer.calls('crewmate_set_approval'), isEmpty);
      await _unmount(t);
    });

    testWidgets('with Bypass off for the crew the sheet does not offer it', (
      t,
    ) async {
      await _pump(t, _chat('crewmate-1'));
      await _computer.otherClient('crew_settings_set', {
        'short_replies': true,
        'allow_bypass': false,
      });
      await settle(t);
      await t.tap(_modeButton);
      await settle(t);
      expect(find.text('Accept edits'), findsOneWidget);
      expect(find.text('Bypass'), findsNothing);
      await _unmount(t);
    });

    testWidgets(
      'a refusal hostd codes is said in words, and the sheet stays open',
      (t) async {
        await _pump(t, _chat('crewmate-1'));
        await t.tap(_modeButton);
        await settle(t);
        // The crew turns Bypass off after the sheet read its settings: the phone still offers it, hostd refuses.
        await _computer.otherClient('crew_settings_set', {
          'short_replies': true,
          'allow_bypass': false,
        });
        await settle(t);
        await t.tap(find.text('Bypass'));
        await settle(t);
        expect(
          find.text(
            'Bypass is turned off for this crew. Turn it on in the crew settings to use it.',
          ),
          findsOneWidget,
        );
        expect(find.text('Permission mode for Reviewer'), findsOneWidget);
        expect(_inButton('Ask'), findsOneWidget, reason: 'nothing changed');
        await _unmount(t);
      },
    );

    testWidgets(
      'an engine with no approval channel is fixed, with the reason',
      (t) async {
        final handle = t.ensureSemantics();
        // Reviewer's computer gets a Codex crewmate while the chat for it is open.
        await _pump(t, _chat('crewmate-3'));
        expect(_modeButton, findsNothing, reason: 'nobody by that id yet');
        await _computer.otherClient('crewmate_create', {
          'name': 'Cx',
          'agent': 'codex',
          'role': 'r',
        });
        await settle(t);
        expect(_inButton('Bypass'), findsOneWidget);
        expect(
          find.bySemanticsLabel(
            "Permission mode: Bypass. It can't be changed.",
          ),
          findsWidgets,
        );
        await t.tap(_modeButton);
        await settle(t);
        expect(
          find.text('This engine has no approval channel.'),
          findsOneWidget,
        );
        // The other rows do nothing: the control is fixed.
        await t.tap(find.text('Accept edits'));
        await settle(t);
        expect(_computer.calls('crewmate_set_approval'), isEmpty);
        expect(find.text('Permission mode for Cx'), findsOneWidget);
        handle.dispose();
        await _unmount(t);
      },
    );
  });

  group('the mode on the crew list and the profile', () {
    testWidgets('each crewmate row says its mode, amber for Bypass only', (
      t,
    ) async {
      await _pump(t, const CrewScreen());
      Finder badge(String mode) => find.widgetWithText(ModeBadge, mode);
      // Both demo crewmates ask; the logbook has a row for each.
      expect(badge('Ask'), findsWidgets);
      expect(badge('Bypass'), findsNothing);
      await _computerSwitches(t, 'crewmate-2', 'bypass');
      expect(badge('Bypass'), findsWidgets);
      final amber = t.widget<Text>(
        find.descendant(of: badge('Bypass').first, matching: find.byType(Text)),
      );
      expect(amber.style?.color, c.warning);
      final calm = t.widget<Text>(
        find.descendant(of: badge('Ask').first, matching: find.byType(Text)),
      );
      expect(calm.style?.color, isNot(c.warning));
      await _unmount(t);
    });

    testWidgets('the profile shows it and the row opens the same sheet', (
      t,
    ) async {
      await _pump(
        t,
        const CrewmateProfileScreen(device: _mac, crewmateId: 'crewmate-1'),
      );
      expect(find.text('Permission mode'), findsOneWidget);
      await t.tap(find.text('Permission mode'));
      await settle(t);
      expect(find.text('Permission mode for Reviewer'), findsOneWidget);
      await t.tap(find.text('Bypass'));
      await settle(t);
      expect(_computer.calls('crewmate_set_approval'), [
        {'id': 'crewmate-1', 'mode': 'bypass'},
      ]);
      await _unmount(t);
    });
  });

  group('answering a prompt', () {
    testWidgets('Allow once is one tap and sends no scope', (t) async {
      await _pump(t, _chat('crewmate-2'));
      expect(find.text('Allow once'), findsOneWidget);
      expect(find.text('Allow for this task'), findsOneWidget);
      expect(find.text('Deny'), findsOneWidget);
      await t.tap(find.text('Allow once'));
      await settle(t);
      expect(_computer.calls('agent_approve'), [
        {
          'pane': 'crew:crewmate-2:task-n2',
          'req': 'crewmate-approval-1',
          'allow': true,
        },
      ]);
      expect(find.text('Allow once'), findsNothing, reason: 'answered');
      await _unmount(t);
    });

    testWidgets('Allow for this task sends scope task', (t) async {
      await _pump(t, _chat('crewmate-2'));
      await t.tap(find.text('Allow for this task'));
      await settle(t);
      expect(_computer.calls('agent_approve').single['scope'], 'task');
      expect(_computer.calls('agent_approve').single['allow'], isTrue);
      await _unmount(t);
    });

    testWidgets('Deny is one tap', (t) async {
      await _pump(t, _chat('crewmate-2'));
      await t.tap(find.text('Deny'));
      await settle(t);
      final call = _computer.calls('agent_approve').single;
      expect(call['allow'], isFalse);
      expect(call, isNot(contains('scope')));
      await _unmount(t);
    });

    testWidgets('a comment goes with the answer and reaches the audit', (
      t,
    ) async {
      await _pump(t, _chat('crewmate-2'));
      await t.tap(find.text('Comment or more options'));
      await settle(t);
      expect(find.text('Answer Nightly'), findsOneWidget);
      await t.enterText(
        find.byKey(const Key('approval-comment')),
        '  Not in dist, use the cache dir  ',
      );
      await t.tap(find.byKey(const Key('approval-sheet-deny')));
      await settle(t);
      final call = _computer.calls('agent_approve').single;
      expect(call['allow'], isFalse);
      expect(call['comment'], 'Not in dist, use the cache dir');
      expect(find.text('Answer Nightly'), findsNothing);
      expect(find.text('Allow once'), findsNothing, reason: 'answered');
      await _unmount(t);
    });

    testWidgets('the sheet also allows for the task, with the comment', (
      t,
    ) async {
      await _pump(t, _chat('crewmate-2'));
      await t.tap(find.text('Comment or more options'));
      await settle(t);
      await t.enterText(find.byKey(const Key('approval-comment')), 'go ahead');
      await t.tap(find.byKey(const Key('approval-sheet-allow-task')));
      await settle(t);
      final call = _computer.calls('agent_approve').single;
      expect(call['scope'], 'task');
      expect(call['comment'], 'go ahead');
      await _unmount(t);
    });

    testWidgets('the Crew tab\'s card answers the same way', (t) async {
      await _pump(t, const CrewScreen());
      await t.tap(find.text('Allow for this task'));
      await settle(t);
      expect(_computer.calls('agent_approve').single['scope'], 'task');
      await _unmount(t);
    });

    testWidgets('a refused answer says why and the prompt stays', (t) async {
      await _pump(t, _chat('crewmate-2'));
      // Another client answered first and the phone does not know yet: hostd refuses the second answer.
      _computer.holdApprovalDone = true;
      await _computer.otherClient('agent_approve', {
        'pane': 'crew:crewmate-2:task-n2',
        'req': 'crewmate-approval-1',
        'allow': true,
      });
      await t.tap(find.text('Deny'));
      await settle(t);
      expect(find.textContaining("Couldn't send the answer"), findsOneWidget);
      await _unmount(t);
    });

    testWidgets('a call a rail held can only be allowed once, and says why', (
      t,
    ) async {
      final answers = <ApprovalDecision>[];
      await t.pumpWidget(
        MaterialApp(
          theme: atlasTheme(),
          home: Scaffold(
            body: SingleChildScrollView(
              child: CrewmateApprovalBubble(
                look: null,
                crewmateName: 'Pat',
                approval: const CrewmatePendingApproval(
                  req: 'r',
                  toolName: 'Write',
                  toolInput: {'file_path': '.env'},
                  paneKey: 'crew:c:t',
                  rail: ApprovalRail.protectedPath,
                ),
                onAnswer: (d) async => answers.add(d),
              ),
            ),
          ),
        ),
      );
      expect(find.text('Allow for this task'), findsNothing);
      expect(find.text('Allow once'), findsOneWidget);
      expect(find.text('Deny'), findsOneWidget);
      expect(find.textContaining('Protected path'), findsOneWidget);
      await t.tap(find.text('Allow once'));
      expect(answers.single.allow, isTrue);
      expect(answers.single.scope, ApprovalScope.once);
    });
  });

  group('the crew settings', () {
    testWidgets('the default mode is saved, Bypass off hides it everywhere', (
      t,
    ) async {
      await _pump(t, const CrewSettingsScreen(device: _mac));
      expect(find.text('Permissions'), findsOneWidget);
      await t.scrollUntilVisible(
        find.text('Default mode for new crewmates'),
        200,
        scrollable: find.byType(Scrollable).first,
      );
      await t.drag(find.byType(ListView), const Offset(0, -300));
      await settle(t);
      await t.tap(find.text('Bypass'));
      await settle(t);
      var saved = _computer.calls('crew_settings_set').last;
      expect(saved['default_approval'], 'bypass');
      expect(saved['allow_bypass'], isTrue);

      // Turning Bypass off falls the default back to Ask, and the option goes.
      await t.tap(find.byKey(const Key('crew-allow-bypass')));
      await settle(t);
      saved = _computer.calls('crew_settings_set').last;
      expect(saved['allow_bypass'], isFalse);
      expect(saved['default_approval'], 'ask');
      expect(find.text('Bypass'), findsNothing);
      expect(
        find.textContaining('Codex, OpenCode'),
        findsOneWidget,
        reason: 'engines that cannot ask say what Bypass off does to them',
      );
      await _unmount(t);
    });
  });

  group('recruiting a crewmate', () {
    // The connection stays open behind the sheet, as it does behind the Crew tab.
    Widget sheet() => Consumer(
      builder: (context, ref, _) {
        ref.watch(connectionNotifierProvider(_mac));
        return Scaffold(
          body: Center(
            child: ElevatedButton(
              onPressed: () => showNewCrewmateSheet(context, const [_mac]),
              child: const Text('Recruit'),
            ),
          ),
        );
      },
    );

    Future<void> open(WidgetTester t) async {
      await _pump(t, sheet());
      await t.tap(find.text('Recruit'));
      await settle(t);
    }

    testWidgets('starts in the crew\'s default mode', (t) async {
      await _pump(t, sheet());
      await _computer.otherClient('crew_settings_set', {
        'short_replies': true,
        'default_approval': 'edits',
      });
      await settle(t);
      await t.tap(find.text('Recruit'));
      await settle(t);
      await t.enterText(find.byType(TextField).first, 'Pat');
      await t.tap(find.text('Create crewmate'));
      await settle(t);
      expect(_computer.calls('crewmate_create').single['mode'], 'edits');
      expect(
        _computer.calls('crewmate_create').single,
        isNot(contains('approval')),
      );
      await _unmount(t);
    });

    testWidgets('Bypass is not offered when the crew does not allow it', (
      t,
    ) async {
      await _pump(t, sheet());
      await _computer.otherClient('crew_settings_set', {
        'short_replies': true,
        'allow_bypass': false,
      });
      await settle(t);
      await t.tap(find.text('Recruit'));
      await settle(t);
      expect(find.text('Accept edits'), findsOneWidget);
      expect(find.text('Bypass'), findsNothing);
      await _unmount(t);
    });

    testWidgets('an engine that cannot ask is Bypass, fixed, with the reason', (
      t,
    ) async {
      await open(t);
      await t.tap(find.byType(DropdownButtonFormField<String>));
      await settle(t);
      await t.tap(find.text('Codex').last);
      await settle(t);
      expect(find.text('This engine has no approval channel.'), findsOneWidget);
      await t.enterText(find.byType(TextField).first, 'Cx');
      await t.tap(find.text('Create crewmate'));
      await settle(t);
      final call = _computer.calls('crewmate_create').single;
      expect(call['agent'], 'codex');
      expect(call['mode'], 'bypass');
      await _unmount(t);
    });
  });

  group('in Brazilian Portuguese', () {
    // The phone's pt-BR picks the one Portuguese catalog (ARB `pt`), the language alone.
    testWidgets('the picker, the badge and the prompt speak it', (t) async {
      await t.pumpWidget(
        MaterialApp(
          theme: atlasTheme(),
          locale: const Locale('pt', 'BR'),
          localizationsDelegates: atlasLocalizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: SingleChildScrollView(
              child: Column(
                children: [
                  ModePicker(value: PermissionMode.edits, onChanged: (_) {}),
                  const ModeBadge(mode: PermissionMode.bypass),
                  ApprovalActions(
                    approval: const CrewmatePendingApproval(
                      req: 'r',
                      toolName: 'Bash',
                      paneKey: 'crew:c:t',
                    ),
                    crewmateName: 'Pat',
                    onAnswer: (_) async {},
                  ),
                ],
              ),
            ),
          ),
        ),
      );
      await t.pump();
      expect(find.text('Perguntar'), findsOneWidget);
      expect(find.text('Aceitar edições'), findsOneWidget);
      expect(find.text('Bypass'), findsWidgets);
      expect(find.text('Permitir uma vez'), findsOneWidget);
      expect(find.text('Permitir nesta tarefa'), findsOneWidget);
      expect(find.text('Negar'), findsOneWidget);
      expect(find.text('Allow once'), findsNothing);
    });
  });

  group('large text on the smallest phone', () {
    layout.AppWrapper wrapDemo() =>
        (app) => ProviderScope(
          overrides: [
            macStorageProvider.overrideWithValue(DemoStorage(const [_mac])),
            transportFactoryProvider.overrideWithValue(
              (_, _) async => demoTransport(),
            ),
          ],
          child: app,
        );

    Future<void> check(
      WidgetTester t,
      String label,
      Widget Function(BuildContext) screen, {
      Future<void> Function(WidgetTester)? prepare,
      bool tapTargets = true,
    }) => layout.expectNoLayoutProblems(
      t,
      screen,
      wrap: wrapDemo(),
      label: label,
      prepare: prepare,
      tapTargets: tapTargets,
      settleFor: const Duration(milliseconds: 3000),
    );

    /// A screen with one button that opens [open]: what a sheet sits over.
    Widget opener(void Function(BuildContext) open) => Consumer(
      builder: (context, ref, _) {
        ref.watch(connectionNotifierProvider(_mac));
        return Scaffold(
          body: Center(
            child: ElevatedButton(
              onPressed: () => open(context),
              child: const Text('Open'),
            ),
          ),
        );
      },
    );

    const held = CrewmatePendingApproval(
      req: 'r',
      toolName: 'mcp__github__create_pull_request_review_comment',
      toolInput: {
        'command':
            'git push --force-with-lease origin feat/crew-permission-modes',
      },
      paneKey: 'crew:crewmate-2:task-n2',
      rail: ApprovalRail.protectedPath,
    );

    testWidgets('the prompt bubble: three answers, a long tool, a rail', (
      t,
    ) async {
      await check(
        t,
        'prompt bubble',
        (_) => Scaffold(
          body: SingleChildScrollView(
            padding: const EdgeInsets.all(Space.md),
            child: Column(
              children: [
                CrewmateApprovalBubble(
                  look: null,
                  crewmateName: 'Nightly',
                  approval: const CrewmatePendingApproval(
                    req: 'r',
                    toolName: 'Bash',
                    toolInput: {'command': 'bun test --filter payments'},
                    paneKey: 'crew:crewmate-2:task-n2',
                  ),
                  onAnswer: (_) async {},
                ),
                CrewmateApprovalBubble(
                  look: null,
                  crewmateName: 'Nightly',
                  approval: held,
                  onAnswer: (_) async {},
                ),
              ],
            ),
          ),
        ),
      );
    });

    testWidgets('the mode button in the header', (t) async {
      await check(
        t,
        'mode button',
        (_) => Scaffold(
          body: Center(
            child: ModeButton(
              crewmate: CrewmateEntry.fromJson({
                'id': 'c',
                'name': 'Pat',
                'agent': 'claude',
                'mode': 'edits',
                'state': 'idle',
              }),
              onTap: () {},
            ),
          ),
        ),
      );
    });

    testWidgets('the mode sheet', (t) async {
      await check(
        t,
        'mode sheet',
        (_) => opener(
          (context) =>
              showModeSheet(context, device: _mac, crewmateId: 'crewmate-1'),
        ),
        prepare: (t) async {
          await t.tap(find.text('Open'));
          await layout.settle(t, 1500);
        },
      );
    });

    testWidgets('the answer sheet', (t) async {
      await check(
        t,
        'answer sheet',
        (_) => opener(
          (context) => showApprovalAnswerSheet(
            context,
            approval: held,
            crewmateName: 'Nightly',
          ),
        ),
        prepare: (t) async {
          await t.tap(find.text('Open'));
          await layout.settle(t, 800);
        },
      );
    });

    testWidgets('the answer sheet of a call a mode could allow', (t) async {
      await check(
        t,
        'answer sheet, task scope',
        (_) => opener(
          (context) => showApprovalAnswerSheet(
            context,
            approval: const CrewmatePendingApproval(
              req: 'r',
              toolName: 'Bash',
              toolInput: {'command': 'rm -rf dist'},
              paneKey: 'crew:crewmate-2:task-n2',
            ),
            crewmateName: 'Nightly',
          ),
        ),
        prepare: (t) async {
          await t.tap(find.text('Open'));
          await layout.settle(t, 800);
        },
      );
    });

    testWidgets('the crew settings', (t) async {
      await check(
        t,
        'crew settings',
        (_) => const CrewSettingsScreen(device: _mac),
      );
    });

    testWidgets('the recruit sheet', (t) async {
      await check(
        t,
        'recruit sheet',
        (_) => opener((context) => showNewCrewmateSheet(context, const [_mac])),
        // The sheet's Name field is 47 dp tall in the theme (every text field is); the modes are 56 dp rows.
        tapTargets: false,
        prepare: (t) async {
          await t.tap(find.text('Open'));
          await layout.settle(t, 1500);
        },
      );
    });
  });
}

/// Another client (the desktop) switches [id] to [mode] on the computer; the phone is told by push.
Future<void> _computerSwitches(WidgetTester t, String id, String mode) async {
  await _computer.otherClient('crewmate_set_approval', {
    'id': id,
    'mode': mode,
  });
  await settle(t);
}
