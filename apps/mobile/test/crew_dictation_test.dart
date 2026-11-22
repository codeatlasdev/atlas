// Dictation in the composer of a crewmate's chat, against a fake recognizer behind the platform interface:
// the words are only ever a draft until the check, the X puts the draft back, nothing sends by itself, and the
// microphone is released whenever the session ends. Permissions, the labelled online opt-in and the crew-wide
// switch are the other rules this file holds.
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/core/voice/dictation.dart';
import 'package:atlas_mobile/core/voice/platform_dictation.dart';
import 'package:atlas_mobile/features/crew/chat/attachments.dart';
import 'package:atlas_mobile/features/crew/chat/composer.dart';

import 'support/fake_dictation.dart';
import 'support/fake_permissions.dart';

class _Rig {
  _Rig({
    FakeDictation? fake,
    FakePermissions? perms,
    this.voice = true,
    this.locale = const Locale('en'),
    String draft = '',
  }) : fake = fake ?? FakeDictation(),
       perms = perms ?? FakePermissions(),
       controller = TextEditingController(text: draft);

  final FakeDictation fake;
  final FakePermissions perms;
  final bool voice;
  final Locale locale;
  final TextEditingController controller;
  final sends = <({String text, bool dictated})>[];

  Widget app({bool voiceOn = true}) => ProviderScope(
    overrides: [
      dictationProvider.overrideWithValue(fake),
      permissionsProvider.overrideWithValue(perms),
    ],
    child: MaterialApp(
      theme: atlasTheme(),
      locale: locale,
      localizationsDelegates: atlasLocalizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: Scaffold(
        body: Align(
          alignment: Alignment.bottomCenter,
          child: ChatComposer(
            controller: controller,
            sending: false,
            online: true,
            queued: 0,
            voice: voiceOn && voice,
            onSend: (List<PendingImage> images, bool dictated) async {
              sends.add((text: controller.text, dictated: dictated));
              controller.clear();
              return true;
            },
          ),
        ),
      ),
    ),
  );

  Future<void> pump(WidgetTester t) async {
    await t.pumpWidget(app());
    await t.pump();
  }

  /// Taps the mic and lets the start finish (no sheet is expected).
  Future<void> startDictation(WidgetTester t) async {
    await t.tap(find.bySemanticsLabel('Dictate'));
    await t.pump();
    await t.pump();
  }
}

/// A few frames: the sheet closes and the session starts. Never `pumpAndSettle` while the mic is live: the live
/// dot pulses forever, and settling would run the fake clock through the 5-minute cap.
Future<void> _flow(WidgetTester t) async {
  for (var i = 0; i < 4; i++) {
    await t.pump(const Duration(milliseconds: 200));
  }
}

Future<void> _check(WidgetTester t) async {
  await t.tap(find.bySemanticsLabel('Insert dictated text'));
  await t.pumpAndSettle();
}

void main() {
  group('the words are a draft until the check', () {
    testWidgets(
      'partial text is shown dimmed and is NOT in the message yet; the check inserts it at the cursor',
      (t) async {
        final rig = _Rig(draft: 'fix the  bug');
        rig.controller.selection = const TextSelection.collapsed(offset: 8);
        await rig.pump(t);
        await rig.startDictation(t);

        rig.fake.say('broken');
        await t.pump();
        expect(
          find.textContaining('broken', findRichText: true),
          findsOneWidget,
        );
        expect(
          rig.controller.text,
          'fix the  bug',
          reason: 'the partial is only a preview',
        );

        rig.fake.say('broken parser');
        rig.fake.finalOnStop = 'broken parser';
        await _check(t);
        expect(rig.controller.text, 'fix the broken parser bug');
        expect(
          rig.controller.selection,
          const TextSelection.collapsed(offset: 'fix the broken parser'.length),
        );
        expect(rig.fake.micLive, isFalse);
        expect(
          find.bySemanticsLabel('Insert dictated text'),
          findsNothing,
          reason: 'the bar is gone',
        );
      },
    );

    testWidgets('the words replace the selection', (t) async {
      final rig = _Rig(draft: 'rename foo to bar');
      rig.controller.selection = const TextSelection(
        baseOffset: 7,
        extentOffset: 10,
      );
      await rig.pump(t);
      await rig.startDictation(t);
      rig.fake.say('the helper');
      rig.fake.finalOnStop = 'the helper';
      await _check(t);
      expect(rig.controller.text, 'rename the helper to bar');
    });

    testWidgets(
      'the dimmed preview is drawn at the cursor, between the old words',
      (t) async {
        final rig = _Rig(draft: 'fix  bug');
        rig.controller.selection = const TextSelection.collapsed(offset: 4);
        await rig.pump(t);
        await rig.startDictation(t);
        rig.fake.say('the');
        await t.pump();
        expect(
          find.textContaining('fix the bug', findRichText: true),
          findsOneWidget,
        );
      },
    );

    testWidgets(
      'X restores the previous draft, drops the words and releases the mic',
      (t) async {
        final rig = _Rig(draft: 'keep this');
        rig.controller.selection = const TextSelection.collapsed(offset: 4);
        await rig.pump(t);
        await rig.startDictation(t);
        rig.fake.say('never mind');
        await t.pump();

        await t.tap(find.bySemanticsLabel('Discard dictation'));
        await t.pump();
        await t.pump();
        expect(rig.controller.text, 'keep this');
        expect(
          rig.controller.selection,
          const TextSelection.collapsed(offset: 4),
        );
        expect(rig.fake.micLive, isFalse);
        expect(rig.sends, isEmpty);
      },
    );

    testWidgets(
      'nothing is ever sent by dictating; the Send dot sends, with dictated: true',
      (t) async {
        final rig = _Rig();
        await rig.pump(t);
        await rig.startDictation(t);
        rig.fake.say('run the tests');
        rig.fake.finalOnStop = 'run the tests';
        await _check(t);
        expect(rig.sends, isEmpty, reason: 'the check inserts; it never sends');

        await t.tap(find.bySemanticsLabel('Send'));
        await t.pump();
        expect(rig.sends, [(text: 'run the tests', dictated: true)]);
      },
    );

    testWidgets(
      'a typed message is not dictated, and the flag clears after a send',
      (t) async {
        final rig = _Rig();
        await rig.pump(t);
        await t.enterText(find.byType(TextField), 'typed by hand');
        await t.pumpAndSettle();
        await t.tap(find.bySemanticsLabel('Send'));
        await t.pump();
        expect(rig.sends.single.dictated, isFalse);

        await rig.startDictation(t);
        rig.fake.say('then spoken');
        rig.fake.finalOnStop = 'then spoken';
        await _check(t);
        await t.tap(find.bySemanticsLabel('Send'));
        await t.pump();
        expect(rig.sends.last.dictated, isTrue);

        await t.enterText(find.byType(TextField), 'typed again');
        await t.pumpAndSettle();
        await t.tap(find.bySemanticsLabel('Send'));
        await t.pump();
        expect(
          rig.sends.last.dictated,
          isFalse,
          reason: 'the flag belongs to one message',
        );
      },
    );

    testWidgets(
      'a message dictated and then fully erased by hand is not dictated',
      (t) async {
        final rig = _Rig();
        await rig.pump(t);
        await rig.startDictation(t);
        rig.fake.say('spoken');
        rig.fake.finalOnStop = 'spoken';
        await _check(t);
        await t.enterText(find.byType(TextField), '');
        await t.enterText(find.byType(TextField), 'typed instead');
        await t.pumpAndSettle();
        await t.tap(find.bySemanticsLabel('Send'));
        await t.pump();
        expect(rig.sends.single.dictated, isFalse);
      },
    );
  });

  group('the microphone is released', () {
    testWidgets(
      'the 5-minute cap keeps what was said: mic off, words waiting for the check',
      (t) async {
        final rig = _Rig(draft: 'start:');
        rig.controller.selection = const TextSelection.collapsed(offset: 6);
        await rig.pump(t);
        await rig.startDictation(t);
        rig.fake.say('a long monologue');

        await t.pump(dictationCap - const Duration(seconds: 1));
        expect(rig.fake.micLive, isTrue, reason: 'still inside the cap');

        await t.pump(const Duration(seconds: 2));
        await t.pump();
        expect(rig.fake.micLive, isFalse);
        expect(find.text('Microphone off'), findsOneWidget);
        expect(
          rig.controller.text,
          'start:',
          reason: 'still a draft until the check',
        );

        await _check(t);
        expect(rig.controller.text, 'start: a long monologue');
      },
    );

    testWidgets(
      'the app going to the background releases the mic and keeps the words',
      (t) async {
        final rig = _Rig();
        await rig.pump(t);
        await rig.startDictation(t);
        rig.fake.say('half a thought');

        t.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
        await t.pump();
        expect(
          rig.fake.micLive,
          isTrue,
          reason: 'inactive is a system sheet, not the background',
        );

        t.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
        t.binding.handleAppLifecycleStateChanged(AppLifecycleState.paused);
        await t.pump();
        await t.pump();
        expect(rig.fake.micLive, isFalse);
        expect(find.text('Microphone off'), findsOneWidget);

        t.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
        await t.pump();
        await _check(t);
        expect(rig.controller.text, 'half a thought');
      },
    );

    testWidgets('leaving the chat while listening releases the mic', (t) async {
      final rig = _Rig();
      await rig.pump(t);
      await rig.startDictation(t);
      await t.pumpWidget(const SizedBox());
      expect(rig.fake.micLive, isFalse);
    });

    testWidgets('the crew switching voice off while listening stops it', (
      t,
    ) async {
      final rig = _Rig();
      await rig.pump(t);
      await rig.startDictation(t);
      rig.fake.say('about to be cut');
      await t.pumpWidget(rig.app(voiceOn: false));
      await t.pump();
      expect(rig.fake.micLive, isFalse);
      expect(find.bySemanticsLabel('Dictate'), findsNothing);
      expect(rig.controller.text, '');
    });

    testWidgets(
      'an engine that ends by itself leaves the words for the check',
      (t) async {
        final rig = _Rig();
        await rig.pump(t);
        await rig.startDictation(t);
        rig.fake.endByItself('that is all');
        await t.pump();
        await t.pump();
        expect(find.text('Microphone off'), findsOneWidget);
        await _check(t);
        expect(rig.controller.text, 'that is all');
      },
    );
  });

  group('the crew-wide switch', () {
    testWidgets('voice off in the crew: no mic', (t) async {
      final rig = _Rig(voice: false);
      await rig.pump(t);
      expect(find.bySemanticsLabel('Dictate'), findsNothing);
      expect(
        find.bySemanticsLabel('Send'),
        findsNothing,
        reason: 'the empty composer has no send dot, as before',
      );
    });

    testWidgets('voice on: the mic is there, labelled', (t) async {
      final rig = _Rig();
      await rig.pump(t);
      expect(find.bySemanticsLabel('Dictate'), findsOneWidget);
    });
  });

  group('which recognizer, and who decides', () {
    testWidgets('on-device: the engine is named all the time', (t) async {
      final rig = _Rig(
        fake: FakeDictation(
          supports: const DictationSupport(onDevice: DictationEngine.apple),
        ),
      );
      await rig.pump(t);
      await rig.startDictation(t);
      expect(find.text('On this iPhone'), findsOneWidget);
      expect(rig.fake.started.single.online, isFalse);
    });

    testWidgets('Android on-device is named as such', (t) async {
      final rig = _Rig(
        fake: FakeDictation(
          supports: const DictationSupport(onDevice: DictationEngine.android),
        ),
      );
      await rig.pump(t);
      await rig.startDictation(t);
      expect(find.text('Android on-device'), findsOneWidget);
    });

    testWidgets(
      'no on-device recognizer: it says so and offers the system recognizer only as a labelled opt-in',
      (t) async {
        final rig = _Rig(
          fake: FakeDictation(supports: const DictationSupport(online: true)),
        );
        await rig.pump(t);
        await t.tap(find.bySemanticsLabel('Dictate'));
        await t.pumpAndSettle();

        expect(
          find.text('Dictation is not available on this phone'),
          findsOneWidget,
        );
        expect(find.text('Use the system recognizer (online)'), findsOneWidget);
        expect(find.text('May send audio to Apple or Google'), findsOneWidget);
        expect(
          rig.fake.started,
          isEmpty,
          reason: 'nothing records before the person chooses',
        );
        expect(rig.fake.micLive, isFalse);

        // Declining leaves the phone exactly as it was.
        await t.tap(find.text('Close'));
        await t.pumpAndSettle();
        expect(rig.fake.started, isEmpty);
        expect(find.bySemanticsLabel('Dictate'), findsOneWidget);
      },
    );

    testWidgets(
      'choosing the opt-in starts the online recognizer and labels it so',
      (t) async {
        final rig = _Rig(
          fake: FakeDictation(supports: const DictationSupport(online: true)),
        );
        await rig.pump(t);
        await t.tap(find.bySemanticsLabel('Dictate'));
        await t.pumpAndSettle();
        await t.tap(find.text('Use the system recognizer (online)'));
        await _flow(t);

        expect(rig.fake.started.single.online, isTrue);
        expect(find.text('System recognizer (online)'), findsOneWidget);
      },
    );

    testWidgets('the opt-in is per session: the next tap asks again', (
      t,
    ) async {
      final rig = _Rig(
        fake: FakeDictation(supports: const DictationSupport(online: true)),
      );
      await rig.pump(t);
      await t.tap(find.bySemanticsLabel('Dictate'));
      await t.pumpAndSettle();
      await t.tap(find.text('Use the system recognizer (online)'));
      await _flow(t);
      await t.tap(find.bySemanticsLabel('Discard dictation'));
      await _flow(t);

      await t.tap(find.bySemanticsLabel('Dictate'));
      await t.pumpAndSettle();
      expect(find.text('Use the system recognizer (online)'), findsOneWidget);
    });

    testWidgets(
      'a language pack still downloading says so, with the same opt-in',
      (t) async {
        final rig = _Rig(
          fake: FakeDictation(
            supports: const DictationSupport(online: true, downloading: true),
          ),
        );
        await rig.pump(t);
        await t.tap(find.bySemanticsLabel('Dictate'));
        await t.pumpAndSettle();
        expect(find.textContaining('still downloading'), findsOneWidget);
        expect(find.text('Use the system recognizer (online)'), findsOneWidget);
      },
    );

    testWidgets(
      'a phone with no recognizer at all gets the explanation and nothing to opt into',
      (t) async {
        final rig = _Rig(fake: FakeDictation(supports: DictationSupport.none));
        await rig.pump(t);
        await t.tap(find.bySemanticsLabel('Dictate'));
        await t.pumpAndSettle();
        expect(
          find.text('Dictation is not available on this phone'),
          findsOneWidget,
        );
        expect(
          find.textContaining('no speech recognizer at all'),
          findsOneWidget,
        );
        expect(find.text('Use the system recognizer (online)'), findsNothing);
        expect(rig.fake.started, isEmpty);
      },
    );

    testWidgets('the speech language follows the app: pt-BR and en-US', (
      t,
    ) async {
      final pt = _Rig(locale: const Locale('pt'));
      await pt.pump(t);
      await t.tap(find.bySemanticsLabel('Ditar'));
      await t.pump();
      await t.pump();
      expect(pt.fake.supportAsked, ['pt-BR']);
      expect(pt.fake.started.single.locale, 'pt-BR');
      expect(find.text('Neste iPhone'), findsOneWidget);
      await t.pumpWidget(const SizedBox());

      final en = _Rig(locale: const Locale('en'));
      await en.pump(t);
      await en.startDictation(t);
      expect(en.fake.started.single.locale, 'en-US');
    });
  });

  group('permissions', () {
    testWidgets(
      'not asked yet: explained first, then the system prompt, then it listens',
      (t) async {
        final perms = FakePermissions(
          states: {AtlasPermission.microphone: PermissionState.notAsked},
        );
        final rig = _Rig(perms: perms);
        await rig.pump(t);
        await t.tap(find.bySemanticsLabel('Dictate'));
        await t.pumpAndSettle();

        expect(find.text('Allow the microphone'), findsOneWidget);
        expect(
          perms.requested,
          isEmpty,
          reason: 'the system prompt comes after the explanation',
        );
        expect(rig.fake.started, isEmpty);

        await t.tap(find.text('Continue'));
        await _flow(t);
        expect(perms.requested, [AtlasPermission.microphone]);
        expect(rig.fake.started, hasLength(1));
      },
    );

    testWidgets(
      'a recognizer that needs Speech Recognition (iOS before 26) asks for it after the microphone',
      (t) async {
        final perms = FakePermissions(
          states: {
            AtlasPermission.microphone: PermissionState.notAsked,
            AtlasPermission.speech: PermissionState.notAsked,
          },
        );
        final rig = _Rig(
          perms: perms,
          fake: FakeDictation(
            supports: const DictationSupport(
              onDevice: DictationEngine.apple,
              needsSpeechPermission: true,
            ),
          ),
        );
        await rig.pump(t);
        await t.tap(find.bySemanticsLabel('Dictate'));
        await t.pumpAndSettle();
        await t.tap(find.text('Continue'));
        await _flow(t);
        expect(find.text('Allow speech recognition'), findsOneWidget);
        await t.tap(find.text('Continue'));
        await _flow(t);
        expect(perms.requested, [
          AtlasPermission.microphone,
          AtlasPermission.speech,
        ]);
        expect(rig.fake.started, hasLength(1));
      },
    );

    testWidgets(
      'SpeechAnalyzer (iOS 26) never asks for Speech Recognition, only the microphone',
      (t) async {
        final perms = FakePermissions(
          states: {AtlasPermission.microphone: PermissionState.notAsked},
        );
        final rig = _Rig(perms: perms);
        await rig.pump(t);
        await t.tap(find.bySemanticsLabel('Dictate'));
        await t.pumpAndSettle();
        await t.tap(find.text('Continue'));
        await _flow(t);
        expect(perms.requested, [AtlasPermission.microphone]);
      },
    );

    testWidgets(
      'denied: an explained state with a way to Settings, and nothing records',
      (t) async {
        final perms = FakePermissions(
          states: {AtlasPermission.microphone: PermissionState.blocked},
        );
        final rig = _Rig(perms: perms);
        await rig.pump(t);
        await t.tap(find.bySemanticsLabel('Dictate'));
        await t.pumpAndSettle();

        expect(find.text('Microphone access is off'), findsOneWidget);
        expect(
          find.text('Atlas cannot dictate your messages.'),
          findsOneWidget,
        );
        expect(perms.requested, isEmpty);
        expect(rig.fake.started, isEmpty);

        await t.tap(find.text('Open Settings'));
        await t.pump();
        expect(perms.settingsOpened, 1);
      },
    );

    testWidgets(
      'restricted by a device policy: explained, no Settings button',
      (t) async {
        final perms = FakePermissions(
          states: {AtlasPermission.microphone: PermissionState.restricted},
        );
        final rig = _Rig(perms: perms);
        await rig.pump(t);
        await t.tap(find.bySemanticsLabel('Dictate'));
        await t.pumpAndSettle();
        expect(find.text('Restricted on this phone'), findsOneWidget);
        expect(find.text('Open Settings'), findsNothing);
      },
    );

    testWidgets(
      'the system prompt answered No lands in the explained off state',
      (t) async {
        final perms = FakePermissions(
          states: {AtlasPermission.microphone: PermissionState.notAsked},
          answers: {AtlasPermission.microphone: PermissionState.blocked},
        );
        final rig = _Rig(perms: perms);
        await rig.pump(t);
        await t.tap(find.bySemanticsLabel('Dictate'));
        await t.pumpAndSettle();
        await t.tap(find.text('Continue'));
        await _flow(t);
        expect(find.text('Microphone access is off'), findsOneWidget);
        expect(rig.fake.started, isEmpty);
      },
    );
  });

  group('failures say what happened', () {
    testWidgets('another app holds the microphone: said, nothing recording', (
      t,
    ) async {
      final rig = _Rig();
      rig.fake.refuseStart = const DictationException(DictationFailure.busy);
      await rig.pump(t);
      await rig.startDictation(t);
      expect(
        find.text('The microphone is in use by another app.'),
        findsOneWidget,
      );
      expect(find.bySemanticsLabel('Insert dictated text'), findsNothing);
      expect(rig.fake.micLive, isFalse);
    });

    testWidgets(
      'a language the phone has no model for: said, never retried online',
      (t) async {
        final rig = _Rig();
        rig.fake.refuseStart = const DictationException(
          DictationFailure.language,
        );
        await rig.pump(t);
        await rig.startDictation(t);
        expect(
          find.text(
            'This language is not available for dictation on this phone.',
          ),
          findsOneWidget,
        );
        expect(rig.fake.started.where((s) => s.online), isEmpty);
      },
    );

    testWidgets('a call interrupting keeps what was said for the check', (
      t,
    ) async {
      final rig = _Rig();
      await rig.pump(t);
      await rig.startDictation(t);
      rig.fake.say('before the call');
      rig.fake.failWith(DictationFailure.interrupted);
      await t.pump();
      await t.pump();
      await _check(t);
      expect(rig.controller.text, 'before the call');
    });

    testWidgets('nothing recognised: said, the draft untouched', (t) async {
      final rig = _Rig(draft: 'keep');
      await rig.pump(t);
      await rig.startDictation(t);
      await t.tap(find.bySemanticsLabel('Insert dictated text'));
      await t.pump();
      await t.pump();
      expect(find.textContaining('Nothing was recognised'), findsOneWidget);
      expect(rig.controller.text, 'keep');
    });
  });

  group('accessibility', () {
    testWidgets(
      'every control is labelled; the live mic has an indicator a screen reader reads',
      (t) async {
        final handle = t.ensureSemantics();
        final rig = _Rig();
        await rig.pump(t);
        expect(find.bySemanticsLabel('Dictate'), findsOneWidget);
        await rig.startDictation(t);
        expect(find.bySemanticsLabel('Discard dictation'), findsOneWidget);
        expect(find.bySemanticsLabel('Insert dictated text'), findsOneWidget);
        expect(
          find.bySemanticsLabel('Listening. Microphone is on'),
          findsOneWidget,
        );
        expect(
          find.bySemanticsLabel('Recognizer: On this iPhone'),
          findsOneWidget,
        );
        handle.dispose();
      },
    );

    testWidgets('start, stop, discard and auto-stop are announced', (t) async {
      final said = <String>[];
      t.binding.defaultBinaryMessenger.setMockDecodedMessageHandler<dynamic>(
        SystemChannels.accessibility,
        (m) async {
          final data = (m as Map)['data'];
          if (m['type'] == 'announce' && data is Map) {
            said.add(data['message'] as String);
          }
          return null;
        },
      );
      addTearDown(
        () => t.binding.defaultBinaryMessenger
            .setMockDecodedMessageHandler<dynamic>(
              SystemChannels.accessibility,
              null,
            ),
      );
      final rig = _Rig();
      await t.pumpWidget(
        MediaQuery(
          data: const MediaQueryData(supportsAnnounce: true),
          child: rig.app(),
        ),
      );
      await t.pump();

      await rig.startDictation(t);
      expect(said.last, 'Listening. Dictation started.');

      rig.fake.say('one');
      rig.fake.finalOnStop = 'one';
      await _check(t);
      expect(said, contains('Dictation stopped.'));
      expect(said.last, 'Dictated text inserted.');

      await rig.startDictation(t);
      await t.tap(find.bySemanticsLabel('Discard dictation'));
      await t.pump();
      await t.pump();
      expect(said.last, 'Dictation discarded.');

      await rig.startDictation(t);
      rig.fake.say('two');
      await t.pump(dictationCap + const Duration(seconds: 1));
      await t.pump();
      expect(said.last, startsWith('Five-minute limit reached.'));
    });

    testWidgets(
      'reduced motion: the live dot does not pulse (the screen settles)',
      (t) async {
        final rig = _Rig();
        await t.pumpWidget(
          MediaQuery(
            data: const MediaQueryData(disableAnimations: true),
            child: rig.app(),
          ),
        );
        await t.pump();
        await rig.startDictation(t);
        // Settling means no animation is running: a pulsing dot would never let it.
        await t.pumpAndSettle(
          const Duration(milliseconds: 100),
          EnginePhase.sendSemanticsUpdate,
          const Duration(seconds: 2),
        );
        expect(find.text('Listening'), findsOneWidget);
      },
    );

    testWidgets(
      'with motion allowed the live dot pulses (so the previous test can tell)',
      (t) async {
        final rig = _Rig();
        await rig.pump(t);
        await rig.startDictation(t);
        var settled = true;
        try {
          await t.pumpAndSettle(
            const Duration(milliseconds: 100),
            EnginePhase.sendSemanticsUpdate,
            const Duration(seconds: 2),
          );
        } on FlutterError {
          settled = false;
        }
        expect(settled, isFalse);
      },
    );
  });
}
