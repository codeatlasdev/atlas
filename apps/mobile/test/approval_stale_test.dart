import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/transport/fake_transport.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';

void main() {
  group('stale approval', () {
    late FakeTransport ft;
    late Multiplexer mux;

    setUp(() {
      ft = FakeTransport();
      mux = Multiplexer(ft);
    });

    tearDown(() async {
      await mux.close();
    });

    test('agent_approval push is received on pushEvents', () async {
      final eventFuture = mux.pushEvents.first;
      ft.injectLine(
        '{"ev":"agent_approval","pane_key":"p1","agent":"a",'
        '"req":"r1","tool_name":"bash","tool_input":"ls -la"}',
      );
      final event = await eventFuture;
      expect(event['ev'], 'agent_approval');
      expect(event['req'], 'r1');
    });

    test('agent_approval_done push is received after approval', () async {
      final pushes = <Map<String, dynamic>>[];
      final sub = mux.pushEvents.listen(pushes.add);

      ft.injectLine(
        '{"ev":"agent_approval","pane_key":"p1","agent":"a",'
        '"req":"r1","tool_name":"bash","tool_input":"ls"}',
      );
      ft.injectLine('{"ev":"agent_approval_done","pane_key":"p1","req":"r1"}');
      await Future.delayed(Duration.zero);

      expect(pushes, hasLength(2));
      expect(pushes[0]['ev'], 'agent_approval');
      expect(pushes[1]['ev'], 'agent_approval_done');
      expect(pushes[1]['req'], 'r1');

      await sub.cancel();
    });

    test('stale approve (after agent_approval_done) returns '
        '"no such pending request" error', () async {
      final pushes = <Map<String, dynamic>>[];
      final sub = mux.pushEvents.listen(pushes.add);

      // Simulate the server pushing approval then immediately done.
      ft.injectLine(
        '{"ev":"agent_approval","pane_key":"p1","agent":"a",'
        '"req":"r1","tool_name":"bash","tool_input":"ls"}',
      );
      ft.injectLine('{"ev":"agent_approval_done","pane_key":"p1","req":"r1"}');
      await Future.delayed(Duration.zero);
      await sub.cancel();

      // Client calls agent_approve after the request is already done.
      final approveFuture = mux.call('agent_approve', {
        'pane': 'p1',
        'req': 'r1',
        'allow': true,
      });
      // Server replies with an error (id=1, first call on this mux).
      ft.injectLine('{"id":1,"ok":false,"error":"no such pending request"}');

      final resp = await approveFuture;
      expect(resp['ok'], isFalse);
      expect(resp['error'], 'no such pending request');
    });
  });
}
