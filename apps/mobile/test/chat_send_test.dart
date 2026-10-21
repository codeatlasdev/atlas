import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/models.dart';

AgentStatus _status(String state) => AgentStatus(
  state: state,
  agent: 'test-agent',
  updatedAt: 0,
  stateStartedAt: 0,
);

void main() {
  group('canSendChat', () {
    test('returns false for null status', () {
      expect(canSendChat(null), isFalse);
    });

    test('returns true when state is idle', () {
      expect(canSendChat(_status('idle')), isTrue);
    });

    test('returns true when state is done', () {
      expect(canSendChat(_status('done')), isTrue);
    });

    test('returns false when state is working', () {
      expect(canSendChat(_status('working')), isFalse);
    });

    test('returns false when state is blocked', () {
      expect(canSendChat(_status('blocked')), isFalse);
    });

    test('returns false when state is waiting', () {
      expect(canSendChat(_status('waiting')), isFalse);
    });
  });
}
