// Contract: `crew_voice_status` and `crewmate_task.dictated` as hostd really sends and expects them
// (test/fixtures/crew_voice.json is written by hostd's serializer: UPDATE_FIXTURES=1 cargo test -p atlas-host
// --lib crew::tests::voice, never by hand). The phone recognises speech itself and reads one field, `enabled`.
import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/crew_voice.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/voice/platform_dictation.dart';
import 'package:atlas_mobile/features/crew/chat/crewmate_chat_screen.dart';
import 'package:flutter_riverpod/misc.dart' show Override;
import 'package:atlas_mobile/core/permissions/permissions.dart';

import 'support/demo_harness.dart';
import 'support/fake_dictation.dart';
import 'support/fake_permissions.dart';

final _fixture = jsonDecode(
  File('test/fixtures/crew_voice.json').readAsStringSync(),
) as Map<String, dynamic>;

Map<String, dynamic> _status(String key) =>
    (_fixture[key] as Map).cast<String, dynamic>();

/// The demo computer, with `crew_voice_status` answered as hostd would ([status], null: an older hostd that has no
/// such command) and every `crewmate_task` recorded.
class _VoiceHost implements Transport {
  _VoiceHost(this._inner, this.status) {
    _inner.incomingLines.listen(_lines.add, onDone: _lines.close);
  }
  final Transport _inner;
  final Map<String, dynamic>? status;
  final tasks = <Map<String, dynamic>>[];
  final _lines = StreamController<String>.broadcast();

  @override
  Stream<String> get incomingLines => _lines.stream;

  @override
  Future<void> sendLine(String line) async {
    final req = jsonDecode(line) as Map<String, dynamic>;
    if (req['cmd'] == 'crew_voice_status') {
      _lines.add(
        jsonEncode(
          status == null
              ? {
                  'id': req['id'],
                  'ok': false,
                  'error': 'unknown command: crew_voice_status',
                }
              : {'id': req['id'], 'ok': true, 'data': status},
        ),
      );
      return;
    }
    if (req['cmd'] == 'crewmate_task') {
      tasks.add(
        (req['args'] as Map?)?.cast<String, dynamic>() ??
            {for (final e in req.entries) e.key: e.value},
      );
    }
    await _inner.sendLine(line);
  }

  @override
  Future<void> close() async {
    await _lines.close();
    await _inner.close();
  }
}

void main() {
  const mac = MacDevice(nodeId: 'demo', name: 'Demo MacBook');

  group('crew_voice_status, hostd\'s real replies', () {
    test('the crew-wide switch is read as is', () {
      expect(CrewVoice.fromJson(_status('status_on')).enabled, isTrue);
      expect(CrewVoice.fromJson(_status('status_off')).enabled, isFalse);
    });

    test('no engine on the crew server does not turn the phone\'s dictation off: the switch is separate', () {
      final s = _status('status_enabled_no_engine');
      expect(s['engine'], isNull);
      expect(CrewVoice.fromJson(s).enabled, isTrue);
    });

    test('a reply with no switch (an older hostd) reads as off', () {
      expect(CrewVoice.fromJson(const {}).enabled, isFalse);
      expect(CrewVoice.fromJson({'enabled': 'yes'}).enabled, isFalse);
    });
  });

  group('the chat against a host that speaks voice', () {
    Future<_VoiceHost> pump(
      WidgetTester t,
      Map<String, dynamic>? status,
      FakeDictation fake,
    ) async {
      await t.binding.setSurfaceSize(const Size(393, 852));
      late _VoiceHost host;
      await t.pumpWidget(
        demoWrap(
          const CrewmateChatScreen(device: mac, crewmateId: 'crewmate-1'),
          [mac],
          overrides: <Override>[
            dictationProvider.overrideWithValue(fake),
            permissionsProvider.overrideWithValue(FakePermissions()),
          ],
          transport: () => host = _VoiceHost(demoTransport(), status),
        ),
      );
      await settle(t);
      return host;
    }

    testWidgets('voice off in the crew hides the mic', (t) async {
      await pump(t, _status('status_off'), FakeDictation());
      expect(find.bySemanticsLabel('Dictate'), findsNothing);
      await t.pumpWidget(const SizedBox());
      await settle(t);
    });

    testWidgets('an older hostd without voice shows no mic either', (t) async {
      await pump(t, null, FakeDictation());
      expect(find.bySemanticsLabel('Dictate'), findsNothing);
      await t.pumpWidget(const SizedBox());
      await settle(t);
    });

    testWidgets(
      'voice on: the mic shows, and a dictated task is sent with dictated: true',
      (t) async {
        final fake = FakeDictation();
        final host = await pump(t, _status('status_on'), fake);
        expect(find.bySemanticsLabel('Dictate'), findsOneWidget);

        await t.tap(find.bySemanticsLabel('Dictate'));
        await t.pump();
        await t.pump();
        fake.say('rename foo underscore bar');
        fake.finalOnStop = 'rename foo_bar';
        await t.tap(find.bySemanticsLabel('Insert dictated text'));
        await t.pump(const Duration(milliseconds: 300));
        await t.pump(const Duration(milliseconds: 600));
        expect(host.tasks, isEmpty, reason: 'dictating never sends');

        await t.tap(find.bySemanticsLabel('Send'));
        await settle(t);
        expect(host.tasks.single['prompt'], 'rename foo_bar');
        expect(host.tasks.single['dictated'], isTrue);
        expect(host.tasks.single['source'], 'mobile');
        expect(
          fake.started.single.online,
          isFalse,
          reason: 'the audio never goes to hostd',
        );
        await t.pumpWidget(const SizedBox());
        await settle(t);
      },
    );

    testWidgets('a typed task carries no dictated flag at all', (t) async {
      final host = await pump(t, _status('status_on'), FakeDictation());
      await t.enterText(find.byType(TextField), 'typed task');
      await settle(t);
      await t.tap(find.bySemanticsLabel('Send'));
      await settle(t);
      expect(host.tasks.single.containsKey('dictated'), isFalse);
      await t.pumpWidget(const SizedBox());
      await settle(t);
    });
  });
}
