import 'package:flutter_test/flutter_test.dart';
import 'package:atlas_mobile/core/push/push_registration.dart';

void main() {
  test('hostHasCrewCap returns false for null data', () {
    expect(hostHasCrewCap(null), isFalse);
  });

  test('hostHasCrewCap returns false when caps absent', () {
    expect(hostHasCrewCap({'version': '1.0'}), isFalse);
  });

  test('hostHasCrewCap returns false when caps is empty list', () {
    expect(hostHasCrewCap({'caps': <String>[]}), isFalse);
  });

  test('hostHasCrewCap returns false when caps lacks crew', () {
    expect(
      hostHasCrewCap({
        'caps': ['mobile', 'agents'],
      }),
      isFalse,
    );
  });

  test('hostHasCrewCap returns true when caps includes crew', () {
    expect(
      hostHasCrewCap({
        'caps': ['mobile', 'crew', 'agents'],
      }),
      isTrue,
    );
  });

  test('hostHasCrewCap returns true when crew is the only cap', () {
    expect(
      hostHasCrewCap({
        'caps': ['crew'],
      }),
      isTrue,
    );
  });
}
