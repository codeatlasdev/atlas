// Contract: the phone's built-in agent list is generated from `crates/atlas-core/data/agents.json`, the file
// Rust embeds (its own test proves the parsed Rust list serializes back to exactly this file). This reads that
// real file, never a hand-written list, and checks what the phone shows is what the file says.
// Incident: the agent list was typed nine times across Rust, TS and Dart and the copies disagreed on OpenCode.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/agent_registry.dart';
import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';

List<Map<String, dynamic>> _file() => (jsonDecode(
  File('../../crates/atlas-core/data/agents.json').readAsStringSync(),
) as List).cast<Map<String, dynamic>>();

void main() {
  test('the generated list is the registry file: ids, labels, hooks, capabilities, order', () {
    final file = _file();
    expect(builtinAgents.map((a) => a.id), file.map((a) => a['id']));
    for (var i = 0; i < file.length; i++) {
      final a = builtinAgents[i];
      final j = file[i];
      expect(a.label, j['label']);
      expect(a.bin, j['bin']);
      expect(a.setup, j['setup']);
      expect(a.canAsk, j['can_ask']);
      expect(a.writesCommits, j['one_shot'] != null);
    }
  });

  test('OpenCode is a built-in on the phone too: labelled, has a mark, signs in, no commit messages', () {
    expect(agentLabel('opencode'), 'OpenCode');
    expect(knownAgents, contains('opencode'));
    expect(engineLabels['opencode'], 'OpenCode');
    expect(builtinAgentOf('opencode')!.writesCommits, isFalse);
  });

  test('every agent of the registry has a label, a mark and a sign-in row; unknown ids fall back', () {
    for (final a in _file()) {
      final id = a['id'] as String;
      expect(agentLabel(id), a['label']);
      expect(knownAgents, contains(id));
      expect(engineLabels[id], a['label']);
    }
    expect(agentLabel('mystery'), 'mystery');
    expect(agentLabel(''), 'Agent');
    expect(builtinAgentOf('mystery'), isNull);
    expect(knownAgents.contains('mystery'), isFalse);
  });
}
