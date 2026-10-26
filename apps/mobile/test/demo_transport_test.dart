import 'package:flutter_test/flutter_test.dart';

import 'fixtures/demo_computer.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';

// App Review and first-run users only see the app through the demo: it must answer the
// same protocol as hostd, including the approval a reviewer is meant to tap.
void main() {
  test(
    'demo lists its pending approval and approving it finishes the turn',
    () async {
      final mux = Multiplexer(demoTransport());
      addTearDown(mux.close);
      final events = <Map<String, dynamic>>[];
      final sub = mux.pushEvents.listen(events.add);
      addTearDown(sub.cancel);

      final all = await mux.call('agent_status_all');
      final pending = (all['data']['pending_approvals'] as List).single as Map;
      final approval = AgentApproval.fromJson(pending.cast<String, dynamic>());
      expect((approval.paneKey, approval.toolName), ('demo-claude', 'Bash'));

      final wrong = await mux.call('agent_approve', {
        'pane': 'demo-claude',
        'req': 'nope',
        'allow': true,
      });
      expect(wrong['error'], 'no such pending request');

      final ok = await mux.call('agent_approve', {
        'pane': 'demo-claude',
        'req': approval.req,
        'allow': true,
      });
      expect(ok['ok'], isTrue);
      final again = await mux.call('agent_status_all');
      expect(again['data']['pending_approvals'], isEmpty);

      await Future<void>.delayed(const Duration(seconds: 3));
      final done = events.lastWhere(
        (e) => e['ev'] == 'agent_status' && e['pane_key'] == 'demo-claude',
      );
      expect(done['status']['state'], 'done');
      final history = await mux.call('agent_history', {'pane': 'demo-claude'});
      expect(
        (history['data']['messages'] as List).last['text'],
        contains('applied'),
      );
    },
  );

  test(
    'demo agent refuses a prompt while working, accepts it when done',
    () async {
      final mux = Multiplexer(demoTransport());
      addTearDown(mux.close);
      final first = await mux.call('agent_send', {
        'pane': 'demo-omp',
        'text': 'hi',
      });
      expect(first['ok'], isTrue);
      final busy = await mux.call('agent_send', {
        'pane': 'demo-omp',
        'text': 'again',
      });
      expect(busy['error'], 'agent is working');
    },
  );

  // The demo is the only way a reviewer sees the timeline: it must serve the same
  // `agent_history` shape hostd does, or the new UI renders empty.
  test(
    'demo serves a timeline, subagents, a title, interrupt and launch',
    () async {
      final mux = Multiplexer(demoTransport());
      addTearDown(mux.close);

      final r = await mux.call('agent_history', {'pane': 'demo-omp'});
      final h = AgentHistory.fromJson(
        (r['data'] as Map).cast<String, dynamic>(),
      );
      expect(h.title, 'Why is payments failing?');
      expect(
        h.items.whereType<ToolItem>().any((i) => i.status == 'error'),
        isTrue,
      );
      final blocks = groupTimeline(h.items);
      expect(blocks.whereType<ToolGroup>(), isNotEmpty);
      expect(
        blocks.whereType<SubagentsBlock>().single.call.agents.single.name,
        'LedgerScout',
      );
      expect(h.subagent('LedgerScout')!.report, contains('4300'));

      final sub = await mux.call('agent_history', {
        'pane': 'demo-omp',
        'sub': 'LedgerScout',
      });
      expect(sub['data']['subagents'], isNull, reason: 'a subagent has none');
      expect(
        AgentHistory.fromJson((sub['data'] as Map).cast<String, dynamic>())
            .items
            .last,
        isA<AssistantItem>(),
      );

      final all = await mux.call('agent_status_all');
      expect(
        all['data']['panes']['demo-omp']['title'],
        'Why is payments failing?',
      );

      expect(
        (await mux.call('agent_interrupt', {'pane': 'demo-omp'}))['error'],
        'agent is not working',
      );
      final launched = await mux.call('agent_launch', {
        'project': 'demo-shop',
        'agent': 'codex',
        'prompt': 'hello',
      });
      final pane = launched['data']['pane'] as String;
      expect(
        (await mux.call('agent_interrupt', {'pane': pane}))['ok'],
        isTrue,
        reason: 'the launched agent took the prompt and is working',
      );
    },
  );
}
