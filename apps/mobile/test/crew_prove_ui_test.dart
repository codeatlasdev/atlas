import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/crew_task.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/features/crew/chat/action_card.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/list/needs_you_card.dart';

import 'support/demo_harness.dart';

// A real 1x1 PNG the demo can serve back and Flutter can draw.
final _png = base64.decode(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
);

Map<String, dynamic> _fixture() => jsonDecode(
  File('test/fixtures/crewmate_task_prove_ui.json').readAsStringSync(),
) as Map<String, dynamic>;

/// One request/reply with the demo computer, the way the app's mux does it.
class _Client {
  _Client(this.t) {
    t.incomingLines.listen((l) {
      final m = jsonDecode(l) as Map<String, dynamic>;
      final id = m['id'];
      if (id is int) _waiting.remove(id)?.complete(m);
    });
  }
  final Transport t;
  int _id = 0;
  final _waiting = <int, Completer<Map<String, dynamic>>>{};

  Future<Map<String, dynamic>> call(String cmd, Map<String, dynamic> args) {
    final id = ++_id;
    final c = Completer<Map<String, dynamic>>();
    _waiting[id] = c;
    t.sendLine(jsonEncode({'id': id, 'cmd': cmd, 'args': args}));
    return c.future.timeout(const Duration(seconds: 5));
  }
}

void main() {
  group('the contract: what the real serializer wrote', () {
    test(
      'a parked ui task parses with its card, ui flag and image sources',
      () {
        final t = CrewmateTask.fromJson(
          (_fixture()['task'] as Map).cast<String, dynamic>(),
        );
        expect(t.ui, isTrue);
        expect(t.state, 'waiting_action');
        expect(t.action?.kind, 'prove_ui');
        expect(t.action?.reason, contains('Could not prove it'));
        expect(t.ownerImages.map((a) => a.name), ['mock.png']);
        expect(t.agentImages.map((a) => a.name), ['chromium-390-light.png']);
        expect(t.agentImages.single.fromAgent, isTrue);
      },
    );

    test('an owner image has no source and a task without ui reads false', () {
      final j = (_fixture()['task'] as Map).cast<String, dynamic>();
      expect((j['attachments'] as List).first, isNot(contains('source')));
      j.remove('ui');
      expect(CrewmateTask.fromJson(j).ui, isFalse);
    });

    test('the action fixture carries prove_ui with a reason', () {
      final all = jsonDecode(
        File('test/fixtures/task_actions.json').readAsStringSync(),
      ) as List;
      final a = TaskAction.fromJson(
        (all.firstWhere((e) => e['kind'] == 'prove_ui') as Map)
            .cast<String, dynamic>(),
      );
      expect(a.reason, isNotEmpty);
    });

    test('a list entry with waiting_task parses and is retryable', () {
      final task = (_fixture()['task'] as Map).cast<String, dynamic>();
      final e = CrewmateEntry.fromJson({
        'id': 'c1',
        'name': 'Ana',
        'role': 'r',
        'agent': 'claude',
        'approval': 'auto',
        'state': 'idle',
        'waiting_task': {
          'id': task['id'],
          'action': task['action'],
          'result': task['result'],
          'attachments': [
            for (final a in task['attachments'] as List)
              if ((a as Map)['source'] == 'agent') a,
          ],
        },
      });
      expect(e.waitingTask?.retryable, isTrue);
      expect(e.waitingTask?.attachments.single.name, 'chromium-390-light.png');
    });
  });

  group('the demo computer answers the new commands', () {
    Future<(_Client, String)> uiTask() async {
      final c = _Client(demoTransport());
      final sent = await c.call('crewmate_task', {
        'id': 'crewmate-1',
        'prompt': 'restyle the header',
        'source': 'mobile',
        'ui': true,
      });
      expect(sent['ok'], true, reason: '$sent');
      final id = (sent['data']['task']['id']) as String;
      // The demo agent works for a few seconds, then parks the unproven task.
      for (var i = 0; i < 100; i++) {
        final tasks = await c.call('crewmate_tasks', {'id': 'crewmate-1'});
        final t = (tasks['data']['tasks'] as List).cast<Map>().firstWhere(
          (t) => t['id'] == id,
        );
        if (t['state'] == 'waiting_action') return (c, id);
        await Future<void>.delayed(const Duration(milliseconds: 100));
      }
      fail('the ui task never parked');
    }

    test('a ui task parks on prove_ui and the list carries its card', () async {
      final (c, id) = await uiTask();
      final tasks = await c.call('crewmate_tasks', {'id': 'crewmate-1'});
      final t = (tasks['data']['tasks'] as List).cast<Map>().firstWhere(
        (t) => t['id'] == id,
      );
      expect(t['action']['kind'], 'prove_ui');
      expect(t['ui'], true);
      final list = await c.call('crewmate_list', {});
      final e = ((list['data']['crewmates']) as List).cast<Map>().firstWhere(
        (e) => e['id'] == 'crewmate-1',
      );
      expect(e['waiting_task']['id'], id);
    }, timeout: const Timeout(Duration(seconds: 30)));

    test('override finishes it; a wrong answer or images on an override are refused', () async {
      final (c, id) = await uiTask();
      final bad = await c.call('crewmate_action_answer', {
        'id': 'crewmate-1',
        'task': id,
        'value': 'yes',
      });
      expect(bad['ok'], false);
      final img = {
        'name': 'm.png',
        'mime': 'image/png',
        'data_b64': base64.encode(_png),
      };
      final onOverride = await c.call('crewmate_action_answer', {
        'id': 'crewmate-1',
        'task': id,
        'value': 'override',
        'attachments': [img],
      });
      expect(onOverride['ok'], false);
      final ok = await c.call('crewmate_action_answer', {
        'id': 'crewmate-1',
        'task': id,
        'value': 'override',
      });
      expect(ok['ok'], true, reason: '$ok');
      final tasks = await c.call('crewmate_tasks', {'id': 'crewmate-1'});
      final t = (tasks['data']['tasks'] as List).cast<Map>().firstWhere(
        (t) => t['id'] == id,
      );
      expect(t['state'], 'done');
    }, timeout: const Timeout(Duration(seconds: 30)));

    test(
      'a retry with an image stores it on the task and runs it again',
      () async {
        final (c, id) = await uiTask();
        final r = await c.call('crewmate_action_answer', {
          'id': 'crewmate-1',
          'task': id,
          'value': 'retry',
          'attachments': [
            {
              'name': 'm.png',
              'mime': 'image/png',
              'data_b64': base64.encode(_png),
            },
          ],
        });
        expect(r['ok'], true, reason: '$r');
        final tasks = await c.call('crewmate_tasks', {'id': 'crewmate-1'});
        final t = (tasks['data']['tasks'] as List).cast<Map>().firstWhere(
          (t) => t['id'] == id,
        );
        expect(
          (t['attachments'] as List).map((a) => (a as Map)['name']),
          contains('m.png'),
        );
      },
      timeout: const Timeout(Duration(seconds: 30)),
    );
  });

  const mac = MacDevice(nodeId: 'demo', name: 'Demo MacBook');

  group('the cards', () {
    testWidgets(
      'the action card shows the capture, the reason and both answers',
      (t) async {
        final task = CrewmateTask.fromJson(
          (_fixture()['task'] as Map).cast<String, dynamic>(),
        );
        await t.pumpWidget(
          demoWrap(
            Scaffold(
              body: SingleChildScrollView(
                child: TaskActionCard(
                  task: task,
                  device: mac,
                  crewmateId: 'crewmate-1',
                ),
              ),
            ),
            [mac],
          ),
        );
        await settle(t);
        expect(find.text('Could not prove this UI work'), findsOneWidget);
        expect(find.textContaining('Could not prove it'), findsWidgets);
        expect(find.text('Retry'), findsOneWidget);
        expect(find.text('Override'), findsOneWidget);
        expect(find.text('Add a screenshot'), findsOneWidget);
        // The agent's capture is requested by name, the owner's mock is not.
        expect(
          find.bySemanticsLabel(RegExp('chromium-390-light.png')),
          findsWidgets,
        );
        expect(find.bySemanticsLabel(RegExp('mock.png')), findsNothing);
      },
    );

    testWidgets('the refused card also shows what the agent captured', (
      t,
    ) async {
      final j = (_fixture()['task'] as Map).cast<String, dynamic>();
      j['action'] = {
        'kind': 'refused',
        'reasons': ['removes 1 assertion(s) from src/tests.rs'],
      };
      await t.pumpWidget(
        demoWrap(
          Scaffold(
            body: SingleChildScrollView(
              child: TaskActionCard(
                task: CrewmateTask.fromJson(j),
                device: mac,
                crewmateId: 'crewmate-1',
              ),
            ),
          ),
          [mac],
        ),
      );
      await settle(t);
      expect(find.text('This work was refused'), findsOneWidget);
      expect(find.textContaining('removes 1 assertion'), findsOneWidget);
      expect(
        find.bySemanticsLabel(RegExp('chromium-390-light.png')),
        findsWidgets,
      );
    });

    testWidgets(
      'the Captain\'s call card shows the parked task and answers in place',
      (t) async {
        final task = (_fixture()['task'] as Map).cast<String, dynamic>();
        final entry = CrewmateEntry.fromJson({
          'id': 'crewmate-1',
          'name': 'Ana',
          'role': 'r',
          'agent': 'claude',
          'approval': 'auto',
          'state': 'idle',
          'waiting_task': {
            'id': task['id'],
            'action': task['action'],
            'attachments': [
              for (final a in task['attachments'] as List)
                if ((a as Map)['source'] == 'agent') a,
            ],
          },
        });
        final CrewEntry e = (
          crewmate: entry,
          device: mac,
          hostName: 'demo',
          online: true,
        );
        expect(needsCaptain(e), isTrue);
        final answers = <String>[];
        await t.pumpWidget(
          demoWrap(
            Scaffold(
              body: SingleChildScrollView(
                child: NeedsYouCard(
                  entry: e,
                  onAnswer: (_) {},
                  onAnswerTask: answers.add,
                  onTap: () {},
                ),
              ),
            ),
            [mac],
          ),
        );
        await settle(t);
        expect(find.text('Could not prove this UI work'), findsOneWidget);
        expect(
          find.bySemanticsLabel(RegExp('chromium-390-light.png')),
          findsWidgets,
        );
        await t.tap(find.text('Retry'));
        await t.tap(find.text('Override'));
        expect(answers, ['retry', 'override']);
      },
    );

    test('a crewmate with neither an approval nor a retryable card is not for the Captain', () {
      final e = (
        crewmate: CrewmateEntry.fromJson({
          'id': 'c',
          'name': 'n',
          'role': 'r',
          'agent': 'a',
          'approval': 'auto',
          'state': 'idle',
          'waiting_task': {
            'id': 't',
            'action': {
              'kind': 'choose',
              'question': 'q',
              'options': ['a'],
            },
          },
        }),
        device: mac,
        hostName: 'h',
        online: true,
      );
      expect(needsCaptain(e), isFalse);
    });
  });
}
