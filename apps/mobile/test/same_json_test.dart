// `sameJson` is how the chat tells a poll that brought nothing new (hostd sends no revision):
// equal by value, never by identity, so an unchanged payload costs no parse and no rebuild.
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/features/agents/chat/same_json.dart';

void main() {
  test('equal by value across nested maps and lists, not by identity', () {
    Object? payload() => {
      'title': 'fix',
      'items': [
        {'kind': 'user', 'text': 'hi'},
        {'kind': 'tool', 'running': true, 'n': 3},
      ],
      'truncated': false,
      'none': null,
    };
    expect(sameJson(payload(), payload()), isTrue);
  });

  test('a changed leaf, key, length or order is a difference', () {
    expect(
      sameJson(
        {
          'a': [1, 2],
        },
        {
          'a': [1, 3],
        },
      ),
      isFalse,
    );
    expect(sameJson({'a': 1}, {'b': 1}), isFalse);
    expect(sameJson({'a': 1}, {'a': 1, 'b': 2}), isFalse);
    expect(sameJson([1, 2], [1, 2, 3]), isFalse);
    expect(sameJson([1, 2], [2, 1]), isFalse);
  });

  test('a missing key is not a null value', () {
    expect(sameJson({'a': null}, {'b': null}), isFalse);
    expect(sameJson({'a': null}, <String, Object?>{}), isFalse);
  });

  test('different shapes and scalars', () {
    expect(sameJson({'a': 1}, [1]), isFalse);
    expect(sameJson(null, {}), isFalse);
    expect(sameJson(1, 1.0), isTrue);
    expect(sameJson('1', 1), isFalse);
    expect(sameJson(null, null), isTrue);
  });
}
