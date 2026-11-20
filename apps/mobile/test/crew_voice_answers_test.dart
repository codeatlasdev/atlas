// #648 on the phone, against hostd's real replies (test/fixtures/crew_voice_answers.json, written by its serializer)
// and fakes behind the platform interfaces (recognizer, text-to-speech): replies read aloud, and approval / choice
// cards answered by voice. The rules held here: the app sends ONLY what was heard and shows hostd's verdict; it never
// turns words into an allow; the voice stops on a tap, on dictation and in the background; the toggle sets the
// crewmate's flag.
import 'package:flutter_riverpod/misc.dart' show Override;
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/voice_verdict.dart';
import 'package:atlas_mobile/core/voice/platform_dictation.dart';
import 'package:atlas_mobile/core/voice/platform_speech.dart';
import 'package:atlas_mobile/core/voice/read_aloud.dart';
import 'package:atlas_mobile/features/crew/chat/crewmate_chat_screen.dart';

import 'support/demo_harness.dart';
import 'support/fake_dictation.dart';
import 'support/fake_permissions.dart';
import 'support/fake_speech.dart';
import 'support/voice_host.dart';

const _mac = MacDevice(nodeId: 'demo', name: 'Demo MacBook');

class _Rig {
  _Rig(this.t, this.host, {FakeSpeech? speech, bool readAloud = false})
    : speech = speech ?? FakeSpeech(),
      store = MemoryReadAloudStore(on: readAloud);

  final WidgetTester t;
  final VoiceHost host;
  final FakeSpeech speech;
  final MemoryReadAloudStore store;
  final dictation = FakeDictation();

  Future<void> pump() async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    await t.pumpWidget(
      demoWrap(
        const CrewmateChatScreen(device: _mac, crewmateId: VoiceHost.id),
        [_mac],
        overrides: <Override>[
          dictationProvider.overrideWithValue(dictation),
          permissionsProvider.overrideWithValue(FakePermissions()),
          speechSynthesizerProvider.overrideWithValue(speech),
          readAloudStoreProvider.overrideWithValue(store),
        ],
        transport: () => host,
      ),
    );
    await settle(t);
  }

  Future<void> end() async {
    await t.pumpWidget(const SizedBox());
    await settle(t);
  }

  /// A reply finishes while the chat is open.
  Future<void> replyArrives(String spoken, {String id = 'new'}) async {
    host.thread.insert(
      0,
      VoiceHost.task(id, 'done', spoken: spoken, ts: 1762000999),
    );
    host.pushTask(id);
    await settle(t);
  }

  /// Taps Answer by voice, says [words], taps the check, and lets hostd's verdict land.
  Future<void> answerByVoice(String words) async {
    await t.tap(find.bySemanticsLabel('Answer by voice'));
    await t.pump();
    await t.pump();
    dictation.say(words);
    dictation.finalOnStop = words;
    await t.tap(find.bySemanticsLabel('Send answer'));
    await t.pump(const Duration(milliseconds: 300));
    await settle(t);
  }
}

VoiceHost _approvalHost({bool voice = true}) => VoiceHost(voiceEnabled: voice)
  ..thread.add(VoiceHost.task('task-1', 'running'))
  ..approval = {
    'req': VoiceHost.approvalReq,
    'tool_name': 'Bash',
    'tool_input': {'command': 'bun test'},
    'pane_key': VoiceHost.approvalPane,
  };

VoiceHost _choiceHost() => VoiceHost()
  ..thread.add(
    VoiceHost.task(
      'task-1',
      'waiting_action',
      action: {
        'kind': 'choose',
        'question': 'Which branch?',
        'options': ['use main', 'use develop'],
      },
    ),
  );

void main() {
  group('the contract: what hostd really answers', () {
    test('every approval verdict reads as its kind, with its reason', () {
      VoiceVerdict v(String k) =>
          VoiceVerdict.fromJson(voiceVerdict('approve', k));
      expect(v('allow').kind, VoiceVerdictKind.allow);
      expect(v('deny').kind, VoiceVerdictKind.deny);
      expect(v('unclear').kind, VoiceVerdictKind.unclear);
      expect(v('comment').kind, VoiceVerdictKind.comment);
      expect(
        v('comment').comment,
        voiceVerdict('approve', 'comment')['comment'],
      );
      expect(v('needs_tap_bypass').reason, VoiceRefusal.bypass);
      expect(v('needs_tap_rail').reason, VoiceRefusal.rail);
      expect(v('needs_tap_destructive').reason, VoiceRefusal.destructive);
      expect(v('needs_tap_untrusted').reason, VoiceRefusal.untrusted);
      for (final k in [
        'needs_tap_bypass',
        'needs_tap_rail',
        'needs_tap_destructive',
        'needs_tap_untrusted',
      ]) {
        expect(v(k).kind, VoiceVerdictKind.needsTap);
      }
    });

    test('only allow, deny and choice are decisions; everything else decided nothing', () {
      expect(
        [
          for (final k in VoiceVerdictKind.values)
            if (k.decided) k,
        ],
        [
          VoiceVerdictKind.allow,
          VoiceVerdictKind.deny,
          VoiceVerdictKind.choice,
        ],
      );
    });

    test('choice cards: the option text is hostd\'s, a card voice cannot answer has no reason', () {
      final c = VoiceVerdict.fromJson(voiceVerdict('answer', 'choice'));
      expect(c.kind, VoiceVerdictKind.choice);
      expect(c.choice, voiceVerdict('answer', 'choice')['choice']);
      final tap = VoiceVerdict.fromJson(
        voiceVerdict('answer', 'needs_tap_card'),
      );
      expect(tap.kind, VoiceVerdictKind.needsTap);
      expect(tap.reason, isNull);
      expect(
        VoiceVerdict.fromJson(voiceVerdict('answer', 'unclear')).kind,
        VoiceVerdictKind.unclear,
      );
    });

    test('a reply of another shape, or a verdict of another name, is unclear: never an allow', () {
      for (final bad in [
        null,
        'allow',
        42,
        <String, dynamic>{},
        {'verdict': 'ALLOW'},
        {'verdict': 'allow_always'},
        {'allow': true},
      ]) {
        expect(
          VoiceVerdict.fromJson(bad).kind,
          VoiceVerdictKind.unclear,
          reason: '$bad',
        );
      }
    });

    test('the crewmate row carries spoken_replies, and an older hostd reads as off', () {
      final row = (voiceAnswers['crewmate'] as Map).cast<String, dynamic>();
      expect(CrewmateEntry.fromJson(row).spokenReplies, isTrue);
      expect(
        CrewmateEntry.fromJson({...row}..remove('spoken_replies'))
            .spokenReplies,
        isFalse,
      );
    });

    test('the table is chosen by the app language only: pt for Portuguese, en for everything else', () {
      expect(voiceLang('pt'), 'pt');
      expect(voiceLang('en'), 'en');
      expect(voiceLang('de'), 'en');
    });
  });

  group('replies read aloud', () {
    testWidgets('off by default: a finished reply is not spoken', (t) async {
      final rig = _Rig(
        t,
        VoiceHost()..thread.add(VoiceHost.task('a', 'running')),
      );
      await rig.pump();
      await rig.replyArrives('Fixed it.');
      expect(rig.speech.spoken, isEmpty);
      await rig.end();
    });

    testWidgets(
      'on: task.spoken is read when a task finishes, in the app language; history is not',
      (t) async {
        final host = VoiceHost(spokenReplies: true)
          ..thread.add(VoiceHost.task('old', 'done', spoken: 'Old news.'));
        final rig = _Rig(t, host, readAloud: true);
        await rig.pump();
        expect(
          rig.speech.spoken,
          isEmpty,
          reason: 'opening the chat reads nothing',
        );

        await rig.replyArrives('Fixed the bug in two files.');
        expect(rig.speech.spoken.single, (
          text: 'Fixed the bug in two files.',
          locale: 'en-US',
        ));
        await rig.end();
      },
    );

    testWidgets(
      "hostd's own lines are what is read: the agent's line and the first sentences",
      (t) async {
        final rig = _Rig(
          t,
          VoiceHost()..thread.add(VoiceHost.task('a', 'running')),
          readAloud: true,
        );
        await rig.pump();
        for (final key in ['task_agent_line', 'task_first_sentences']) {
          final row = (voiceAnswers[key] as List).first as Map<String, dynamic>;
          rig.host.thread.insert(0, {...row, 'id': key});
          rig.host.pushTask(key);
          await settle(t);
          expect(rig.speech.spoken.last.text, row['spoken']);
        }
        await rig.end();
      },
    );

    testWidgets('an empty spoken line says nothing', (t) async {
      final rig = _Rig(
        t,
        VoiceHost()..thread.add(VoiceHost.task('a', 'running')),
        readAloud: true,
      );
      await rig.pump();
      await rig.replyArrives('');
      expect(rig.speech.spoken, isEmpty);
      expect(find.bySemanticsLabel('Speaking. Tap to stop.'), findsNothing);
      await rig.end();
    });

    testWidgets(
      'Speaking is shown while it speaks and is not the live-microphone indicator',
      (t) async {
        final rig = _Rig(
          t,
          VoiceHost()..thread.add(VoiceHost.task('a', 'running')),
          readAloud: true,
        );
        await rig.pump();
        await rig.replyArrives('Done.');
        expect(find.bySemanticsLabel('Speaking. Tap to stop.'), findsOneWidget);
        expect(find.text('Speaking'), findsOneWidget);
        expect(find.text('Listening'), findsNothing);
        expect(find.bySemanticsLabel('Microphone is on'), findsNothing);

        rig.speech.finish();
        await t.pump();
        expect(find.text('Speaking'), findsNothing);
        await rig.end();
      },
    );

    testWidgets('any tap silences it at once; the Speaking pill stops it too', (
      t,
    ) async {
      final rig = _Rig(
        t,
        VoiceHost()..thread.add(VoiceHost.task('a', 'running')),
        readAloud: true,
      );
      await rig.pump();
      await rig.replyArrives('First.');
      expect(rig.speech.audible, isTrue);
      await t.tapAt(const Offset(200, 300));
      await t.pump();
      expect(rig.speech.audible, isFalse, reason: 'a tap on the thread');

      await rig.replyArrives('Second.', id: 'two');
      expect(rig.speech.audible, isTrue);
      await t.tap(find.bySemanticsLabel('Speaking. Tap to stop.'));
      await t.pump();
      expect(rig.speech.audible, isFalse, reason: 'the Speaking pill');
      expect(find.text('Speaking'), findsNothing);
      await rig.end();
    });

    testWidgets(
      'starting dictation silences it, and a reply arriving during dictation is not read',
      (t) async {
        final rig = _Rig(
          t,
          VoiceHost()..thread.add(VoiceHost.task('a', 'running')),
          readAloud: true,
        );
        await rig.pump();
        await rig.replyArrives('Reading now.');
        expect(rig.speech.audible, isTrue);

        await t.tap(find.bySemanticsLabel('Dictate'));
        await t.pump();
        await t.pump();
        expect(rig.dictation.micLive, isTrue);
        expect(rig.speech.audible, isFalse);

        await rig.replyArrives('Too late.', id: 'late');
        expect(rig.speech.spoken.map((s) => s.text), ['Reading now.']);
        await t.tap(find.bySemanticsLabel('Discard dictation'));
        await t.pump();
        await rig.end();
      },
    );

    testWidgets('the app going to the background silences it', (t) async {
      final rig = _Rig(
        t,
        VoiceHost()..thread.add(VoiceHost.task('a', 'running')),
        readAloud: true,
      );
      await rig.pump();
      await rig.replyArrives('Reading now.');
      expect(rig.speech.audible, isTrue);
      t.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      t.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
      t.binding.handleAppLifecycleStateChanged(AppLifecycleState.paused);
      await t.pump();
      expect(rig.speech.audible, isFalse);
      for (final s in [
        AppLifecycleState.hidden,
        AppLifecycleState.inactive,
        AppLifecycleState.resumed,
      ]) {
        t.binding.handleAppLifecycleStateChanged(s);
      }
      await rig.end();
    });

    testWidgets(
      'the toggle sets spoken_replies on the crewmate; turning it off does not touch the flag',
      (t) async {
        final rig = _Rig(
          t,
          VoiceHost()..thread.add(VoiceHost.task('a', 'done')),
        );
        await rig.pump();
        expect(rig.store.on, isFalse);

        await t.tap(find.bySemanticsLabel('Read replies aloud'));
        await settle(t);
        expect(rig.store.on, isTrue);
        expect(rig.host.sent('crewmate_update').single, {
          'id': VoiceHost.id,
          'spoken_replies': true,
        });

        await t.tap(find.bySemanticsLabel('Read replies aloud'));
        await settle(t);
        expect(rig.store.on, isFalse);
        expect(
          rig.host.sent('crewmate_update'),
          hasLength(1),
          reason: 'the flag stays until the crewmate settings',
        );
        await rig.end();
      },
    );

    testWidgets(
      'a crewmate that already writes spoken lines is not asked again',
      (t) async {
        final rig = _Rig(
          t,
          VoiceHost(spokenReplies: true)
            ..thread.add(VoiceHost.task('a', 'done')),
        );
        await rig.pump();
        await t.tap(find.bySemanticsLabel('Read replies aloud'));
        await settle(t);
        expect(rig.store.on, isTrue);
        expect(rig.host.sent('crewmate_update'), isEmpty);
        await rig.end();
      },
    );

    testWidgets(
      'a phone with no voice for the language is not offered the switch',
      (t) async {
        final rig = _Rig(
          t,
          VoiceHost()..thread.add(VoiceHost.task('a', 'done')),
          speech: FakeSpeech(hasVoice: false),
        );
        await rig.pump();
        expect(find.bySemanticsLabel('Read replies aloud'), findsNothing);
        await rig.end();
      },
    );
  });

  group('approval by voice: hostd decides, the app shows', () {
    testWidgets(
      'the words go to hostd as a transcript with the pane, the request and the language, nothing else',
      (t) async {
        final host = _approvalHost()
          ..approveVerdict = voiceVerdict('approve', 'allow');
        final rig = _Rig(t, host);
        await rig.pump();
        await rig.answerByVoice('allow');

        expect(host.sent('agent_approve_voice').single, {
          'pane': VoiceHost.approvalPane,
          'req': VoiceHost.approvalReq,
          'transcript': 'allow',
          'lang': 'en',
        });
        await rig.end();
      },
    );

    for (final (key, snack) in [
      ('allow', 'Allowed once by voice.'),
      ('deny', 'Denied by voice.'),
    ]) {
      testWidgets(
        'verdict $key: hostd already answered, the app says so and sends no answer of its own',
        (t) async {
          final host = _approvalHost()
            ..approveVerdict = voiceVerdict('approve', key);
          final rig = _Rig(t, host);
          await rig.pump();
          await rig.answerByVoice('whatever was said');
          expect(find.text(snack), findsOneWidget);
          expect(host.sent('agent_approve'), isEmpty);
          await rig.end();
        },
      );
    }

    testWidgets(
      'comment: nothing is decided; the words open the note, and only a tap decides',
      (t) async {
        final host = _approvalHost()
          ..approveVerdict = voiceVerdict('approve', 'comment');
        final rig = _Rig(t, host);
        await rig.pump();
        await rig.answerByVoice('allow but only this file');

        expect(host.sent('agent_approve'), isEmpty);
        final note = find.byKey(const Key('approval-comment'));
        expect(note, findsOneWidget);
        expect(
          t.widget<TextField>(note).controller!.text,
          voiceVerdict('approve', 'comment')['comment'],
        );

        await t.tap(find.byKey(const Key('approval-sheet-allow-once')));
        await settle(t);
        final sent = host.sent('agent_approve').single;
        expect(sent['allow'], isTrue);
        expect(sent['comment'], voiceVerdict('approve', 'comment')['comment']);
        await rig.end();
      },
    );

    for (final (key, fragment) in [
      ('needs_tap_bypass', 'Bypass'),
      ('needs_tap_rail', 'protection'),
      ('needs_tap_destructive', 'destructive'),
      ('needs_tap_untrusted', 'source'),
    ]) {
      testWidgets(
        '$key: the reason is said, nothing is decided and the buttons stay',
        (t) async {
          final host = _approvalHost()
            ..approveVerdict = voiceVerdict('approve', key);
          final rig = _Rig(t, host);
          await rig.pump();
          await rig.answerByVoice('allow');

          expect(find.textContaining(fragment), findsOneWidget);
          expect(
            host.sent('agent_approve'),
            isEmpty,
            reason: 'hostd said tap: the app never allows by itself',
          );
          expect(find.text('Allow once'), findsOneWidget);
          await rig.end();
        },
      );
    }

    testWidgets('unclear: said, nothing decided', (t) async {
      final host = _approvalHost()
        ..approveVerdict = voiceVerdict('approve', 'unclear');
      final rig = _Rig(t, host);
      await rig.pump();
      await rig.answerByVoice('mumble');
      expect(find.textContaining('Did not catch an answer'), findsOneWidget);
      expect(host.sent('agent_approve'), isEmpty);
      await rig.end();
    });

    testWidgets(
      'a verdict this app does not know is unclear, never a decision',
      (t) async {
        final host = _approvalHost()
          ..approveVerdict = {'verdict': 'allow_always'};
        final rig = _Rig(t, host);
        await rig.pump();
        await rig.answerByVoice('allow');
        expect(find.textContaining('Did not catch an answer'), findsOneWidget);
        expect(host.sent('agent_approve'), isEmpty);
        await rig.end();
      },
    );

    testWidgets(
      'a phone paired without the approve scope hears hostd say so; nothing is decided',
      (t) async {
        final host = _approvalHost()..voiceRefusal = 'forbidden: approve';
        final rig = _Rig(t, host);
        await rig.pump();
        await rig.answerByVoice('allow');
        expect(
          find.textContaining("isn't allowed to answer approvals"),
          findsOneWidget,
        );
        expect(host.sent('agent_approve'), isEmpty);
        expect(find.text('Allow once'), findsOneWidget);
        await rig.end();
      },
    );

    testWidgets('speech that recognised nothing is not sent', (t) async {
      final host = _approvalHost();
      final rig = _Rig(t, host);
      await rig.pump();
      await rig.answerByVoice('');
      expect(host.sent('agent_approve_voice'), isEmpty);
      expect(find.textContaining('Nothing was recognised'), findsOneWidget);
      await rig.end();
    });

    testWidgets('the crew has voice off: no voice answer is offered', (
      t,
    ) async {
      final rig = _Rig(t, _approvalHost(voice: false));
      await rig.pump();
      expect(find.bySemanticsLabel('Answer by voice'), findsNothing);
      expect(find.text('Allow once'), findsOneWidget);
      await rig.end();
    });

    testWidgets(
      'a voice answer keeps the voice that reads replies quiet while the microphone is open',
      (t) async {
        final host = _approvalHost();
        final rig = _Rig(t, host, readAloud: true);
        await rig.pump();
        await rig.replyArrives('Reading.');
        expect(rig.speech.audible, isTrue);
        await t.tap(find.bySemanticsLabel('Answer by voice'));
        await t.pump();
        await t.pump();
        expect(rig.dictation.micLive, isTrue);
        expect(rig.speech.audible, isFalse);
        await t.tap(find.bySemanticsLabel('Discard dictation'));
        await t.pump();
        await rig.end();
      },
    );
  });

  group('choice by voice', () {
    testWidgets(
      'the transcript goes to hostd; a choice verdict is shown and the app answers nothing itself',
      (t) async {
        final host = _choiceHost()
          ..answerVerdict = voiceVerdict('answer', 'choice');
        final rig = _Rig(t, host);
        await rig.pump();
        await rig.answerByVoice('the first one');

        expect(host.sent('crewmate_action_answer_voice').single, {
          'id': VoiceHost.id,
          'task': 'task-1',
          'transcript': 'the first one',
          'lang': 'en',
        });
        expect(find.text('Chose "use main" by voice.'), findsOneWidget);
        expect(host.sent('crewmate_action_answer'), isEmpty);
        await rig.end();
      },
    );

    testWidgets('unclear: said, the options stay', (t) async {
      final host = _choiceHost()
        ..answerVerdict = voiceVerdict('answer', 'unclear');
      final rig = _Rig(t, host);
      await rig.pump();
      await rig.answerByVoice('hmm');
      expect(find.textContaining('Did not catch an option'), findsOneWidget);
      expect(find.text('use main'), findsOneWidget);
      expect(host.sent('crewmate_action_answer'), isEmpty);
      await rig.end();
    });

    testWidgets('a card voice cannot answer says it needs a tap', (t) async {
      final host = _choiceHost()
        ..answerVerdict = voiceVerdict('answer', 'needs_tap_card');
      final rig = _Rig(t, host);
      await rig.pump();
      await rig.answerByVoice('use main');
      expect(find.textContaining('needs a tap'), findsOneWidget);
      expect(host.sent('crewmate_action_answer'), isEmpty);
      await rig.end();
    });
  });
}
