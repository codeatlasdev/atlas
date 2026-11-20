// Reading replies aloud, below the screens: the controller's promises (one voice, silence when a microphone opens,
// when asked, and when the app leaves the screen), the rule for which finished tasks are spoken, and the channel's
// wire. The real synthesizers need a device; they are checked by hand on one.
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/crew_task.dart';
import 'package:atlas_mobile/core/voice/platform_speech.dart';
import 'package:atlas_mobile/core/voice/read_aloud.dart';
import 'package:atlas_mobile/core/voice/speech.dart';

import 'support/fake_speech.dart';

CrewmateTask _task(String id, String state, {String spoken = '', int ts = 0}) =>
    CrewmateTask(
      id: id,
      ts: ts,
      source: 'mobile',
      prompt: 'p',
      state: state,
      spoken: spoken,
    );

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  group('SpeechController', () {
    late FakeSpeech fake;
    late SpeechController speech;
    setUp(() {
      fake = FakeSpeech();
      speech = SpeechController(fake);
    });
    tearDown(() => speech.dispose());

    test(
      'speaks in the language it is given, and says nothing for empty text',
      () async {
        expect(await speech.speak('  ', 'en-US'), isFalse);
        expect(fake.spoken, isEmpty);
        expect(await speech.speak('  all done  ', 'pt-BR'), isTrue);
        expect(fake.spoken.single, (text: 'all done', locale: 'pt-BR'));
        expect(speech.speaking, isTrue);
      },
    );

    test('an utterance that ends by itself clears Speaking; a failure is remembered until the next one', () async {
      await speech.speak('one', 'en-US');
      fake.finish();
      expect(speech.speaking, isFalse);
      expect(speech.failed, isFalse);

      await speech.speak('two', 'en-US');
      fake.fail();
      expect(speech.speaking, isFalse);
      expect(speech.failed, isTrue);

      await speech.speak('three', 'en-US');
      expect(speech.failed, isFalse, reason: 'a new utterance starts clean');
    });

    test(
      'a synthesizer that refuses to start is a failure, never a throw',
      () async {
        fake.refuseSpeak = StateError('no engine');
        expect(await speech.speak('x', 'en-US'), isFalse);
        expect(speech.speaking, isFalse);
        expect(speech.failed, isTrue);
      },
    );

    test(
      'stop silences at once, and does nothing when nothing speaks',
      () async {
        await speech.stop();
        expect(fake.stops, 0);
        await speech.speak('x', 'en-US');
        await speech.stop();
        expect(fake.audible, isFalse);
        expect(speech.speaking, isFalse);
      },
    );

    test('opening a microphone silences the voice at once, and nothing is spoken while it is open', () async {
      final owner = Object();
      await speech.speak('reading', 'en-US');
      expect(speech.claimMic(owner), isTrue);
      await Future<void>.delayed(Duration.zero);
      expect(fake.audible, isFalse);
      expect(speech.speaking, isFalse);

      expect(await speech.speak('late reply', 'en-US'), isFalse);
      expect(fake.spoken.map((s) => s.text), ['reading']);

      speech.releaseMic(owner);
      expect(await speech.speak('after', 'en-US'), isTrue);
    });

    test('two microphones never run together; a release by a stranger changes nothing', () {
      final a = Object(), b = Object();
      expect(speech.claimMic(a), isTrue);
      expect(speech.claimMic(b), isFalse);
      speech.releaseMic(b);
      expect(speech.micOpen, isTrue);
      speech.releaseMic(a);
      expect(speech.micOpen, isFalse);
    });

    test('the app leaving the screen silences the voice at once; coming back does not resume it', () async {
      await speech.speak('reading', 'en-US');
      speech.didChangeAppLifecycleState(AppLifecycleState.inactive);
      expect(fake.audible, isTrue, reason: 'a system prompt is not leaving');
      speech.didChangeAppLifecycleState(AppLifecycleState.paused);
      await Future<void>.delayed(Duration.zero);
      expect(fake.audible, isFalse);
      speech.didChangeAppLifecycleState(AppLifecycleState.resumed);
      expect(fake.audible, isFalse);
    });

    test(
      'a reply that arrives while the app is in the background is not read',
      () async {
        TestWidgetsFlutterBinding.instance.handleAppLifecycleStateChanged(
          AppLifecycleState.paused,
        );
        addTearDown(
          () => TestWidgetsFlutterBinding.instance
              .handleAppLifecycleStateChanged(AppLifecycleState.resumed),
        );
        expect(await speech.speak('hello', 'en-US'), isFalse);
        expect(fake.spoken, isEmpty);
      },
    );
  });

  group('SpokenReplies: which finished tasks are read', () {
    test('the first thread is history and says nothing', () {
      final r = SpokenReplies();
      expect(r.observe([_task('a', 'done', spoken: 'old news')]), isEmpty);
    });

    test('a task seen running and then done is spoken once', () {
      final r = SpokenReplies()..observe([_task('a', 'running')]);
      expect(r.observe([_task('a', 'done', spoken: 'Fixed it.')]), 'Fixed it.');
      expect(
        r.observe([_task('a', 'done', spoken: 'Fixed it.')]),
        isEmpty,
        reason: 'the same push twice is not read twice',
      );
    });

    test(
      'a task that turns up already done after the first thread is spoken',
      () {
        final r = SpokenReplies()..observe([_task('a', 'done', spoken: 'old')]);
        expect(
          r.observe([
            _task('a', 'done', spoken: 'old'),
            _task('b', 'done', spoken: 'Brand new.'),
          ]),
          'Brand new.',
        );
      },
    );

    test('an empty spoken line says nothing, and failed or cancelled tasks are never read', () {
      final r = SpokenReplies()
        ..observe([_task('a', 'running'), _task('b', 'running')]);
      expect(
        r.observe([_task('a', 'done'), _task('b', 'failed', spoken: 'x')]),
        isEmpty,
      );
    });

    test('two replies landing together are read oldest first', () {
      final r = SpokenReplies()..observe(const []);
      expect(
        r.observe([
          _task('new', 'done', spoken: 'Second.', ts: 20),
          _task('old', 'done', spoken: 'First.', ts: 10),
        ]),
        'First. Second.',
      );
    });
  });

  group('the channel', () {
    test('reads the native events and ignores a shape it does not know', () {
      expect(parseSpeechEvent({'type': 'done'}), SpeechEvent.done);
      expect(parseSpeechEvent({'type': 'error'}), SpeechEvent.failed);
      expect(parseSpeechEvent({'type': 'mystery'}), isNull);
      expect(parseSpeechEvent('done'), isNull);
    });
  });

  group('CrewmateTask.spoken', () {
    test('an older hostd that sends none reads as nothing to say', () {
      expect(CrewmateTask.fromJson(const {'id': 't'}).spoken, '');
      expect(
        CrewmateTask.fromJson(const {'id': 't', 'spoken': 'Done.'}).spoken,
        'Done.',
      );
    });
  });
}
