// Dictation, the parts with no widgets: where dictated words land in the draft, the states of one session
// (the cap, the background, an engine that ends by itself, a slow start) and the native channel's wire.
import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart' show Locale;
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/voice/dictation.dart';
import 'package:atlas_mobile/core/voice/dictation_controller.dart';
import 'package:atlas_mobile/core/voice/platform_dictation.dart';

import 'support/fake_dictation.dart';

TextEditingValue _at(String text, int base, [int? extent]) => TextEditingValue(
  text: text,
  selection: TextSelection(baseOffset: base, extentOffset: extent ?? base),
);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  group('insertDictation: where the words go', () {
    test('at the cursor, with one space on each side that needs it', () {
      final out = insertDictation(_at('fix the bug', 8), 'broken');
      expect(out.text, 'fix the broken bug');
      expect(out.selection, const TextSelection.collapsed(offset: 15));
    });

    test('in place of the selection', () {
      final out = insertDictation(_at('fix the bug now', 8, 11), 'race');
      expect(out.text, 'fix the race now');
    });

    test(
      'a reversed selection (extent before base) replaces the same span',
      () {
        final out = insertDictation(_at('fix the bug now', 11, 8), 'race');
        expect(out.text, 'fix the race now');
      },
    );

    test('never doubles a space the draft already has', () {
      expect(insertDictation(_at('hello ', 6), 'world').text, 'hello world');
      expect(insertDictation(_at(' world', 0), 'hello').text, 'hello world');
      expect(insertDictation(_at('a\nb', 2), 'x').text, 'a\nx b');
    });

    test('into an empty draft: just the words, trimmed', () {
      final out = insertDictation(TextEditingValue.empty, '  open main  ');
      expect(out.text, 'open main');
      expect(out.selection, const TextSelection.collapsed(offset: 9));
    });

    test('punctuation attaches to the word before it', () {
      expect(insertDictation(_at('ready', 5), '. Next').text, 'ready. Next');
      expect(insertDictation(_at('ready go', 5), ',').text, 'ready, go');
    });

    test('a draft with no cursor takes the words at its end', () {
      final out = insertDictation(
        const TextEditingValue(
          text: 'fix',
          selection: TextSelection.collapsed(offset: -1),
        ),
        'it',
      );
      expect(out.text, 'fix it');
    });

    test('empty or blank dictation changes nothing', () {
      final draft = _at('keep', 2);
      expect(insertDictation(draft, '   '), draft);
      expect(insertDictation(draft, ''), draft);
    });

    test('splice reports exactly the run it inserted, spaces included', () {
      final s = spliceDictation(_at('fix the bug', 8), 'broken');
      expect(s.value.text.substring(s.from, s.to), 'broken ');
    });
  });

  group('DictationController', () {
    late FakeDictation fake;
    late DictationController c;
    setUp(() {
      fake = FakeDictation();
      c = DictationController(fake);
    });
    tearDown(() => c.dispose());

    Future<void> listen() async {
      await c.begin(localeTag: 'en-US', engine: DictationEngine.apple);
      expect(c.phase, DictationPhase.listening);
    }

    test(
      'a session shows partials as the transcript so far, never a delta',
      () async {
        await listen();
        fake.say('open');
        fake.say('open src slash main');
        expect(c.text, 'open src slash main');
        expect(fake.micLive, isTrue);
      },
    );

    test('finish asks the recognizer to stop and returns its final text; the mic is released', () async {
      await listen();
      fake.say('open mai');
      fake.finalOnStop = 'open main';
      expect(await c.finish(), 'open main');
      expect(c.phase, DictationPhase.idle);
      expect(fake.micLive, isFalse);
    });

    test('a recognizer that never answers stop still returns the last partial (bounded wait)', () async {
      final slow = DictationController(_SilentOnStop(fake));
      await slow.begin(localeTag: 'en-US', engine: DictationEngine.apple);
      fake.say('almost');
      expect(await slow.finish(), 'almost');
      expect(fake.micLive, isFalse);
      slow.dispose();
    });

    test(
      'the cap stops the session, keeps what was said and releases the mic',
      () async {
        final short = DictationController(
          fake,
          cap: const Duration(milliseconds: 50),
        );
        await short.begin(localeTag: 'en-US', engine: DictationEngine.apple);
        fake.say('one two three');
        final notes = <DictationNote>[];
        short.notes.listen(notes.add);
        await Future<void>.delayed(const Duration(milliseconds: 120));
        expect(short.phase, DictationPhase.review);
        expect(short.text, 'one two three');
        expect(fake.micLive, isFalse);
        expect(notes, [DictationNote.capReached]);
        expect(short.take(), 'one two three');
        short.dispose();
      },
    );

    test('the default cap is five minutes, hostd\'s own limit', () {
      expect(dictationCap, const Duration(minutes: 5));
      expect(c.cap, dictationCap);
    });

    test(
      'background releases the mic at once and keeps the words for review',
      () async {
        await listen();
        fake.say('half a sentence');
        await c.background();
        expect(fake.micLive, isFalse);
        expect(c.phase, DictationPhase.review);
        expect(c.text, 'half a sentence');
        // Not awaited: stop() was never asked, the release is cancel().
        expect(fake.stops, 0);
      },
    );

    test('background with nothing said ends the session quietly', () async {
      await listen();
      await c.background();
      expect(c.phase, DictationPhase.idle);
      expect(c.nothingHeard, isTrue);
      expect(fake.micLive, isFalse);
    });

    test(
      'an engine that ends by itself leaves the words waiting, mic off',
      () async {
        await listen();
        fake.endByItself('that was it');
        await Future<void>.delayed(Duration.zero);
        expect(c.phase, DictationPhase.review);
        expect(c.text, 'that was it');
        expect(fake.micLive, isFalse);
      },
    );

    test('a failure keeps the words said before it', () async {
      await listen();
      fake.say('before the call');
      fake.failWith(DictationFailure.interrupted);
      await Future<void>.delayed(Duration.zero);
      expect(c.failure, DictationFailure.interrupted);
      expect(c.phase, DictationPhase.review);
      expect(c.text, 'before the call');
    });

    test(
      'a refused start is a failure, not a throw, and nothing holds the mic',
      () async {
        fake.refuseStart = const DictationException(DictationFailure.busy);
        await c.begin(localeTag: 'en-US', engine: DictationEngine.apple);
        expect(c.phase, DictationPhase.idle);
        expect(c.failure, DictationFailure.busy);
        expect(fake.micLive, isFalse);
      },
    );

    test('discard during a slow start releases the mic when the recognizer finally starts', () async {
      fake.startGate = Completer();
      final start = c.begin(localeTag: 'en-US', engine: DictationEngine.apple);
      await Future<void>.delayed(Duration.zero);
      expect(c.phase, DictationPhase.starting);
      await c.discard();
      fake.startGate!.complete();
      await start;
      expect(c.phase, DictationPhase.idle);
      expect(
        fake.micLive,
        isFalse,
        reason: 'a started recognizer the person already discarded must be cancelled',
      );
    });

    test('discard drops the text and a late event changes nothing', () async {
      await listen();
      fake.say('private');
      await c.discard();
      fake.say('late partial');
      expect(c.text, '');
      expect(c.phase, DictationPhase.idle);
      expect(fake.micLive, isFalse);
    });

    test('nothing heard: finish returns empty and says so', () async {
      await listen();
      expect(await c.finish(), '');
      expect(c.nothingHeard, isTrue);
    });

    test(
      'disposing the controller releases the mic (leaving the chat)',
      () async {
        await listen();
        c.dispose();
        expect(fake.micLive, isFalse);
        c = DictationController(fake); // so tearDown has something to dispose
      },
    );
  });

  group('PlatformDictation wire', () {
    const methods = MethodChannel('test/dictation');
    late List<MethodCall> calls;
    setUp(() {
      calls = [];
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(methods, (call) async {
            calls.add(call);
            return switch (call.method) {
              'support' => {
                'on_device': 'apple',
                'online': true,
                'speech_permission': true,
                'downloading': false,
              },
              'start' => throw PlatformException(
                code: 'language',
                message: 'no pack',
              ),
              _ => null,
            };
          });
    });
    tearDown(
      () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(methods, null),
    );

    PlatformDictation make() => PlatformDictation(
      methods: methods,
      eventChannel: const EventChannel('test/dictation/events'),
    );

    test(
      'support is read from the native answer, with the locale it asked',
      () async {
        final s = await make().support('pt-BR');
        expect(calls.single.arguments, {'locale': 'pt-BR'});
        expect(s.onDevice, DictationEngine.apple);
        expect(s.online, isTrue);
        expect(s.needsSpeechPermission, isTrue);
      },
    );

    test('a native refusal keeps its reason', () async {
      await expectLater(
        make().start(localeTag: 'en-US', online: false),
        throwsA(
          isA<DictationException>().having(
            (e) => e.code,
            'code',
            DictationFailure.language,
          ),
        ),
      );
      expect(calls.single.arguments, {'locale': 'en-US', 'online': false});
    });

    test(
      'support the platform does not answer means nothing works, never a guess',
      () {
        expect(parseDictationSupport(null).onDevice, isNull);
        expect(parseDictationSupport({'on_device': 'cloud'}).onDevice, isNull);
        expect(parseDictationSupport({'online': 'yes'}).online, isFalse);
      },
    );

    test(
      'events: partial, final and an error; an unknown shape is ignored',
      () {
        expect(
          parseDictationEvent({'type': 'partial', 'text': 'a'}),
          isA<DictationPartial>(),
        );
        expect(
          (parseDictationEvent({
            'type': 'final',
            'text': 'b',
          }) as DictationFinal).text,
          'b',
        );
        expect(
          (parseDictationEvent({
            'type': 'error',
            'code': 'busy',
          }) as DictationFailureEvent).code,
          DictationFailure.busy,
        );
        expect(parseDictationEvent({'type': 'wat'}), isNull);
        expect(parseDictationEvent('partial'), isNull);
      },
    );

    test('an unknown error code is a plain failure', () {
      expect(DictationFailure.byCode('something new'), DictationFailure.failed);
    });
  });

  test(
    'dictationLocaleTag: Portuguese is Brazilian, everything else English',
    () {
      expect(dictationLocaleTag(const Locale('pt')), 'pt-BR');
      expect(dictationLocaleTag(const Locale('pt', 'PT')), 'pt-BR');
      expect(dictationLocaleTag(const Locale('en')), 'en-US');
      expect(dictationLocaleTag(const Locale('de')), 'en-US');
    },
  );

  group('DictationSupport.engine: never a silent online fallback', () {
    test('on-device wins, online only when allowed and there', () {
      const both = DictationSupport(
        onDevice: DictationEngine.android,
        online: true,
      );
      expect(both.engine(allowOnline: false), DictationEngine.android);
      expect(both.engine(allowOnline: true), DictationEngine.android);
      const onlineOnly = DictationSupport(online: true);
      expect(onlineOnly.engine(allowOnline: false), isNull);
      expect(onlineOnly.engine(allowOnline: true), DictationEngine.online);
      expect(DictationSupport.none.engine(allowOnline: true), isNull);
    });
  });
}

/// A recognizer whose `stop` never produces the final text.
class _SilentOnStop extends FakeDictation {
  _SilentOnStop(FakeDictation inner) : _inner = inner;
  final FakeDictation _inner;

  @override
  Stream<DictationEvent> get events => _inner.events;
  @override
  Future<void> start({required String localeTag, required bool online}) =>
      _inner.start(localeTag: localeTag, online: online);
  @override
  Future<void> stop() async {
    _inner.stops++;
    _inner.micLive = false;
  }

  @override
  Future<void> cancel() => _inner.cancel();
}
