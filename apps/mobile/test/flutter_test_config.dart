import 'dart:async';

import 'package:flutter_test/flutter_test.dart';

import 'support/per_platform_goldens.dart';

/// Every golden is compared against the recording of the OS the test runs on (`support/per_platform_goldens.dart`).
Future<void> testExecutable(FutureOr<void> Function() testMain) async {
  goldenFileComparator = PerPlatformGoldens(goldenFileComparator);
  await testMain();
}
