import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/crew_task.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/features/crew/chat/task_thread.dart';

import 'support/golden_frame.dart';

// The thread's queued line says why a task is not starting (#296): a full server read as a
// bare "Queued…". The task is the one hostd's own serializer wrote (the shared fixture).

Future<void> _pump(WidgetTester tester, CrewmateTask task) async {
  await tester.pumpWidget(
    goldenFrame(
      SingleChildScrollView(
        child: CrewmateTaskPair(
          task: task,
          crewmateId: 'c1',
          device: const MacDevice(nodeId: 'n1', name: 'My Mac'),
          look: null,
          expanded: false,
          onToggle: () {},
        ),
      ),
    ),
  );
  await tester.pump();
}

void main() {
  late CrewmateTask busy;

  setUpAll(() {
    final fixture = jsonDecode(
      File('test/fixtures/crewmate_tasks.json').readAsStringSync(),
    ) as Map<String, dynamic>;
    busy = CrewmateTask.fromJson(
      ((fixture['tasks'] as List).firstWhere(
        (t) => t['state'] == 'queued',
      ) as Map).cast<String, dynamic>(),
    );
  });

  testWidgets('a task waiting for a server slot says "Queued · server busy"', (
    tester,
  ) async {
    await _pump(tester, busy);
    expect(find.text('Queued · server busy…'), findsOneWidget);
  });

  testWidgets('a plain queued task stays "Queued…"', (tester) async {
    await _pump(
      tester,
      CrewmateTask(
        id: 'q',
        ts: 1761213200,
        source: 'desktop',
        prompt: 'run the linter',
        state: 'queued',
      ),
    );
    expect(find.text('Queued…'), findsOneWidget);
    expect(find.textContaining('server busy'), findsNothing);
  });
}
