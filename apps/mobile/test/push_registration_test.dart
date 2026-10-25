import 'package:flutter_test/flutter_test.dart';
import 'package:atlas_mobile/core/push/push_registration.dart';

void main() {
  test('hostHasBotsCap returns false for null data', () {
    expect(hostHasBotsCap(null), isFalse);
  });

  test('hostHasBotsCap returns false when caps absent', () {
    expect(hostHasBotsCap({'version': '1.0'}), isFalse);
  });

  test('hostHasBotsCap returns false when caps is empty list', () {
    expect(hostHasBotsCap({'caps': <String>[]}), isFalse);
  });

  test('hostHasBotsCap returns false when caps lacks bots', () {
    expect(hostHasBotsCap({'caps': ['mobile', 'agents']}), isFalse);
  });

  test('hostHasBotsCap returns true when caps includes bots', () {
    expect(hostHasBotsCap({'caps': ['mobile', 'bots', 'agents']}), isTrue);
  });

  test('hostHasBotsCap returns true when bots is the only cap', () {
    expect(hostHasBotsCap({'caps': ['bots']}), isTrue);
  });
}
