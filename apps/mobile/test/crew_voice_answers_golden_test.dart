/// Golden screenshots of #648: an approval card and a choice card with "Answer by voice" in every state hostd's
/// verdict can leave it in, the "Speaking" indicator (a speaker, not the red microphone dot) and the read-aloud
/// switch. Verdicts are hostd's real ones (`crew_voice_answers.json`); the microphone is a fake behind the
/// platform interface. Animations are off so a picture does not depend on when it was taken.
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/crew_task.dart';
import 'package:atlas_mobile/core/protocol/voice_verdict.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/backdrop.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/core/voice/platform_dictation.dart';
import 'package:atlas_mobile/core/voice/speech.dart';
import 'package:atlas_mobile/features/crew/chat/action/choose_action.dart';
import 'package:atlas_mobile/features/crew/chat/approval_bubble.dart';
import 'package:atlas_mobile/features/crew/chat/read_aloud_toggle.dart';

import 'support/fake_dictation.dart';
import 'support/fake_permissions.dart';
import 'support/fake_speech.dart';
import 'support/voice_host.dart';

const _frameKey = ValueKey('frame');

Widget _frame(
  Widget child,
  FakeDictation fake, {
  Locale locale = const Locale('en'),
}) => ProviderScope(
  overrides: [
    dictationProvider.overrideWithValue(fake),
    permissionsProvider.overrideWithValue(FakePermissions()),
  ],
  child: MaterialApp(
    theme: atlasTheme(),
    locale: locale,
    localizationsDelegates: atlasLocalizationsDelegates,
    supportedLocales: AppLocalizations.supportedLocales,
    debugShowCheckedModeBanner: false,
    builder: (context, child) => MediaQuery(
      data: MediaQuery.of(context).copyWith(disableAnimations: true),
      child: atlasGround(context, child),
    ),
    home: Scaffold(
      body: RepaintBoundary(
        key: _frameKey,
        child: Align(
          alignment: Alignment.bottomCenter,
          child: IntrinsicHeight(child: child),
        ),
      ),
    ),
  ),
);

Future<void> _shot(String name) =>
    expectLater(find.byKey(_frameKey), matchesGoldenFile('goldens/$name.png'));

const _approval = CrewmatePendingApproval(
  req: 'r1',
  toolName: 'Bash',
  toolInput: {'command': 'rm -rf build && bun run build'},
  paneKey: 'crew:cm-1:task-1',
);

Widget _approvalCard(String verdict) => CrewmateApprovalBubble(
  look: null,
  approval: _approval,
  crewmateName: 'Reviewer',
  onAnswer: (_) async {},
  voiceAnswer: (words, lang) async =>
      VoiceVerdict.fromJson(voiceVerdict('approve', verdict)),
);

Widget _choiceCard(String verdict) => Padding(
  padding: const EdgeInsets.all(Space.md),
  child: ChooseAction(
    action: TaskAction.fromJson(const {
      'kind': 'choose',
      'question': 'Which branch?',
      'options': ['use main', 'use develop'],
    }),
    busy: false,
    onPick: (_) {},
    onCancel: () {},
    voiceAnswer: (words, lang) async =>
        VoiceVerdict.fromJson(voiceVerdict('answer', verdict)),
  ),
);

/// Opens [card], taps Answer by voice, says [words] and sends it; leaves hostd's verdict on screen.
Future<void> _answered(
  WidgetTester t,
  Widget card,
  String words, {
  Locale locale = const Locale('en'),
  String button = 'Answer by voice',
  String send = 'Send answer',
  double height = 520,
}) async {
  final fake = FakeDictation();
  await t.binding.setSurfaceSize(Size(393, height));
  await t.pumpWidget(_frame(card, fake, locale: locale));
  await t.pump();
  await t.tap(find.bySemanticsLabel(button));
  await t.pump();
  await t.pump();
  fake.say(words);
  fake.finalOnStop = words;
  await t.tap(find.bySemanticsLabel(send));
  await t.pump(const Duration(milliseconds: 300));
  await t.pump(const Duration(milliseconds: 300));
}

void main() {
  testWidgets('approval: the voice button under the answers', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 520));
    await t.pumpWidget(_frame(_approvalCard('allow'), FakeDictation()));
    await t.pump();
    await _shot('crew_voice_answer_approval_idle');
  });

  testWidgets('approval: listening for the answer', (t) async {
    final fake = FakeDictation();
    await t.binding.setSurfaceSize(const Size(393, 520));
    await t.pumpWidget(_frame(_approvalCard('allow'), fake));
    await t.pump();
    await t.tap(find.bySemanticsLabel('Answer by voice'));
    await t.pump();
    await t.pump();
    fake.say('allow');
    await t.pump(const Duration(milliseconds: 300));
    await _shot('crew_voice_answer_approval_listening');
  });

  testWidgets('approval: destructive call, voice cannot allow it', (t) async {
    await _answered(t, _approvalCard('needs_tap_destructive'), 'allow');
    await _shot('crew_voice_answer_approval_needs_tap');
  });

  testWidgets('approval: unclear', (t) async {
    await _answered(t, _approvalCard('unclear'), 'mumble');
    await _shot('crew_voice_answer_approval_unclear');
  });

  testWidgets('approval: rail, in Portuguese', (t) async {
    await _answered(
      t,
      _approvalCard('needs_tap_rail'),
      'permitir',
      locale: const Locale('pt'),
      button: 'Responder por voz',
      send: 'Enviar resposta',
    );
    await _shot('crew_voice_answer_approval_needs_tap_pt');
  });

  testWidgets('choice: the voice button under the options', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 520));
    await t.pumpWidget(_frame(_choiceCard('choice'), FakeDictation()));
    await t.pump();
    await _shot('crew_voice_answer_choice_idle');
  });

  testWidgets('choice: no option matched', (t) async {
    await _answered(t, _choiceCard('unclear'), 'the third');
    await _shot('crew_voice_answer_choice_unclear');
  });

  testWidgets('speaking indicator and the read-aloud switch, off and on', (
    t,
  ) async {
    final fake = FakeSpeech();
    final speech = SpeechController(fake);
    addTearDown(speech.dispose);
    await speech.speak('Fixed the bug.', 'en-US');
    await t.binding.setSurfaceSize(const Size(393, 220));
    await t.pumpWidget(
      _frame(
        Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceEvenly,
              children: [
                ReadAloudButton(on: false, onChanged: (_) {}),
                ReadAloudButton(on: true, onChanged: (_) {}),
              ],
            ),
            SpeakingIndicator(speech: speech),
          ],
        ),
        FakeDictation(),
      ),
    );
    await t.pump();
    await _shot('crew_voice_speaking');
  });
}
