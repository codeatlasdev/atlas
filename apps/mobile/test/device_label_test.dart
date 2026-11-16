// Product review 2026-11-15: Atlas Desktop listed every paired phone as "Atlas Mobile", so two phones could not
// be told apart (or one revoked). Pairing now names the phone by model and system; these are the cases that
// would make two phones look alike again or show a broken name.
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/device_label.dart';

void main() {
  test('an iPhone is its model and system', () {
    expect(
      phoneLabel(model: 'iPhone 15 Pro', system: 'iOS 26.1'),
      'iPhone 15 Pro · iOS 26.1',
    );
  });

  test('an Android model without its maker gets it in front, once', () {
    expect(
      phoneLabel(model: 'SM-S918B', system: 'Android 15', maker: 'samsung'),
      'Samsung SM-S918B · Android 15',
    );
    expect(
      phoneLabel(model: 'Pixel 8', system: 'Android 15', maker: 'Google'),
      'Google Pixel 8 · Android 15',
    );
    expect(
      phoneLabel(
        model: 'motorola edge 50',
        system: 'Android 14',
        maker: 'motorola',
      ),
      'motorola edge 50 · Android 14',
    );
  });

  test('blank parts are left out, and nothing at all is the fallback name', () {
    expect(phoneLabel(model: '  ', system: 'Android 15'), 'Android 15');
    expect(phoneLabel(model: '', system: ''), fallbackDeviceLabel);
  });
}
