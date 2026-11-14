// What a push carries to the phone. `fixtures/push_agent_payload.json` is the data hostd builds for a
// plain agent's push (written by the Rust test `push_payload_fixture_is_the_real_serializer_output`;
// the relay copies those keys into the FCM data). Never hand-edit it.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/push/push_payload.dart';

void main() {
  test('parses the data the real hostd serializer produced', () {
    final data = jsonDecode(
      File('test/fixtures/push_agent_payload.json').readAsStringSync(),
    ) as Map<String, dynamic>;
    final p = PushPayload.fromData(data)!;
    expect(p.kind, PushKind.agentNeedsYou);
    expect(p.pane, data['pane']);
    expect(p.title, data['title']);
    expect(p.opensAgent, isTrue);
  });

  test('every agent kind hostd can send is understood', () {
    for (final (kind, want) in [
      ('agent_done', PushKind.agentDone),
      ('agent_needs_you', PushKind.agentNeedsYou),
    ]) {
      final p = PushPayload.fromData({
        'kind': kind,
        'pane': 'p1',
        'title': 'omp',
      });
      expect(p!.kind, want);
    }
  });

  // Crew pushes carry only the kind: they open the Crew tab, not an agent.
  test('crew kinds parse without a pane and open no agent', () {
    for (final (kind, want) in [
      ('task_done', PushKind.taskDone),
      ('needs_you', PushKind.needsYou),
    ]) {
      final p = PushPayload.fromData({'kind': kind});
      expect(p!.kind, want);
      expect(p.opensAgent, isFalse);
    }
  });

  // FCM data is Map<String, dynamic> of strings and may hold anything: bad input is ignored, never thrown.
  test(
    'unknown kinds, missing kind and an agent push without a pane are null',
    () {
      expect(
        PushPayload.fromData({'kind': 'from_the_future', 'pane': 'p'}),
        isNull,
      );
      expect(PushPayload.fromData({}), isNull);
      expect(PushPayload.fromData({'kind': 7}), isNull);
      expect(PushPayload.fromData({'kind': 'agent_done'}), isNull);
      expect(PushPayload.fromData({'kind': 'agent_done', 'pane': ''}), isNull);
      expect(PushPayload.fromData({'kind': 'agent_done', 'pane': 3}), isNull);
    },
  );

  test('a missing or non-string title is empty, not an error', () {
    expect(
      PushPayload.fromData({'kind': 'agent_done', 'pane': 'p'})!.title,
      '',
    );
    expect(
      PushPayload.fromData({'kind': 'agent_done', 'pane': 'p', 'title': 5})!
          .title,
      '',
    );
  });
}
