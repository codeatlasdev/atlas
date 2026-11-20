/// Golden screenshots: the composer in every state of dictation (the mic, listening on this phone, listening on
/// the online system recognizer, microphone off with words waiting, Portuguese). The live dot is drawn still
/// (reduced motion) so the picture does not depend on when it was taken.
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/backdrop.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/core/voice/dictation.dart';
import 'package:atlas_mobile/core/voice/platform_dictation.dart';
import 'package:atlas_mobile/features/crew/chat/composer.dart';

import 'support/fake_dictation.dart';
import 'support/fake_permissions.dart';

Widget _frame(
  FakeDictation fake,
  TextEditingController controller, {
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
        key: const ValueKey('frame'),
        child: Align(
          alignment: Alignment.bottomCenter,
          child: ChatComposer(
            controller: controller,
            sending: false,
            online: true,
            queued: 0,
            voice: true,
            onSend: (_, _) async => true,
          ),
        ),
      ),
    ),
  ),
);

Future<void> _shot(WidgetTester t, String name) => expectLater(
  find.byKey(const ValueKey('frame')),
  matchesGoldenFile('goldens/$name.png'),
);

Future<void> _open(
  WidgetTester t,
  FakeDictation fake,
  TextEditingController c, {
  Locale locale = const Locale('en'),
  String label = 'Dictate',
}) async {
  await t.binding.setSurfaceSize(const Size(393, 260));
  await t.pumpWidget(_frame(fake, c, locale: locale));
  await t.pump();
  await t.tap(find.bySemanticsLabel(label));
  await t.pump();
  await t.pump(const Duration(milliseconds: 300));
}

void main() {
  testWidgets('idle: the mic next to the field', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 260));
    await t.pumpWidget(_frame(FakeDictation(), TextEditingController()));
    await t.pump();
    await _shot(t, 'crew_dictation_idle');
  });

  testWidgets('listening on this phone: words so far dimmed at the cursor', (
    t,
  ) async {
    final fake = FakeDictation();
    final c = TextEditingController(text: 'fix the  bug in the parser');
    c.selection = const TextSelection.collapsed(offset: 8);
    await _open(t, fake, c);
    fake.say('broken tokenizer');
    await t.pump(const Duration(milliseconds: 300));
    await _shot(t, 'crew_dictation_listening');
  });

  testWidgets(
    'listening on the online system recognizer: named, in the warning colour',
    (t) async {
      final fake = FakeDictation(
        supports: const DictationSupport(online: true),
      );
      await t.binding.setSurfaceSize(const Size(393, 520));
      await t.pumpWidget(_frame(fake, TextEditingController()));
      await t.pump();
      await t.tap(find.bySemanticsLabel('Dictate'));
      await t.pumpAndSettle();
      await _shot(t, 'crew_dictation_opt_in');
      await t.tap(find.text('Use the system recognizer (online)'));
      for (var i = 0; i < 4; i++) {
        await t.pump(const Duration(milliseconds: 200));
      }
      fake.say('open the settings page');
      await t.pump(const Duration(milliseconds: 300));
      await _shot(t, 'crew_dictation_online');
    },
  );

  testWidgets(
    'the cap reached: microphone off, the words waiting for the check',
    (t) async {
      final fake = FakeDictation();
      final c = TextEditingController();
      await _open(t, fake, c);
      fake.say('a long monologue that ran into the five minute limit');
      await t.pump(dictationCap + const Duration(seconds: 1));
      await t.pump(const Duration(milliseconds: 300));
      await _shot(t, 'crew_dictation_review');
    },
  );

  testWidgets('Portuguese', (t) async {
    final fake = FakeDictation();
    await _open(
      t,
      fake,
      TextEditingController(),
      locale: const Locale('pt'),
      label: 'Ditar',
    );
    fake.say('abra o arquivo principal');
    await t.pump(const Duration(milliseconds: 300));
    await _shot(t, 'crew_dictation_listening_pt');
  });
}
