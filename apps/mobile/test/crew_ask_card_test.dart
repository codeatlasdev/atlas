// `ask_user` on the phone (PhoneCrewAudit gap 3): the agent is blocked on a question; the phone renders
// it from the real task (fixtures/crewmate_task_ask.json) and answers with `crewmate_answer`, Act scope.
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/features/crew/chat/ask_card.dart';

import 'support/demo_harness.dart';

class _Recording implements Transport {
  _Recording(this.inner);
  final Transport inner;
  final sent = <Map<String, dynamic>>[];
  @override
  Stream<String> get incomingLines => inner.incomingLines;
  @override
  Future<void> sendLine(String line) {
    sent.add(jsonDecode(line) as Map<String, dynamic>);
    return inner.sendLine(line);
  }

  @override
  Future<void> close() => inner.close();

  List<Map<String, dynamic>> answers() => [
    for (final m in sent)
      if (m['cmd'] == 'crewmate_answer') m,
  ];
}

const _mac = MacDevice(nodeId: 'demo', name: 'Demo MacBook');

CrewmateTask _task({bool allowText = true}) {
  final fixture = jsonDecode(
    File('test/fixtures/crewmate_task_ask.json').readAsStringSync(),
  ) as Map<String, dynamic>;
  final raw = ((fixture['tasks'] as List).first as Map).cast<String, dynamic>();
  raw['ask'] = {
    ...(raw['ask'] as Map<String, dynamic>),
    'allow_text': allowText,
  };
  return CrewmateTask.fromJson(raw);
}

Future<_Recording> _pump(WidgetTester t, CrewmateTask task) async {
  final rec = _Recording(demoTransport());
  await t.pumpWidget(
    demoWrap(
      Scaffold(
        body: SingleChildScrollView(
          child: Consumer(
            builder: (context, ref, _) {
              ref.watch(connectionNotifierProvider(_mac));
              return TaskAskCard(
                task: task,
                device: _mac,
                crewmateId: 'crewmate-1',
              );
            },
          ),
        ),
      ),
      [_mac],
      transport: () => rec,
    ),
  );
  await settle(t);
  return rec;
}

void main() {
  testWidgets('shows the question and every option with its subtitle', (
    t,
  ) async {
    await _pump(t, _task());
    expect(find.text('Deploy to which environment?'), findsOneWidget);
    expect(find.text('Staging'), findsOneWidget);
    expect(find.text('safe'), findsOneWidget);
    expect(find.text('Production'), findsOneWidget);
  });

  testWidgets('tapping an option answers with its key', (t) async {
    final rec = await _pump(t, _task());
    await t.tap(find.text('Production'));
    await settle(t);
    expect(rec.answers().single['args'], {
      'id': 'crewmate-1',
      'ask_id': 'ask-fixture-1',
      'key': 'prod',
    });
  });

  testWidgets('free text answers with text, and an empty field sends nothing', (
    t,
  ) async {
    final rec = await _pump(t, _task());
    await t.tap(find.text('Send'));
    await settle(t);
    expect(rec.answers(), isEmpty);
    await t.enterText(find.byType(TextField), '  canary  ');
    await t.tap(find.text('Send'));
    await settle(t);
    expect(rec.answers().single['args'], {
      'id': 'crewmate-1',
      'ask_id': 'ask-fixture-1',
      'text': 'canary',
    });
  });

  testWidgets('a question that takes no free text has no field', (t) async {
    await _pump(t, _task(allowText: false));
    expect(find.byType(TextField), findsNothing);
  });

  testWidgets('a refused answer stays on screen with the server\'s reason', (
    t,
  ) async {
    // The demo computer knows no such ask: it refuses, the way hostd does for a closed one.
    final rec = await _pump(t, _task());
    await t.tap(find.text('Staging'));
    await settle(t);
    expect(rec.answers(), hasLength(1));
    expect(find.textContaining('ask-fixture-1'), findsWidgets);
  });
}
