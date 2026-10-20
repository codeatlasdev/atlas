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
}
