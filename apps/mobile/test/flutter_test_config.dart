import 'dart:async';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';

/// Flutter rasterises text a few pixels differently on macOS and Linux, so one image
/// cannot match both (0.4–3% diffs on every golden with text). Each platform keeps
/// its own recording and is compared exactly against it: `goldens/x.png` is read from
/// `goldens/macos/x.png` on a Mac and `goldens/linux/x.png` on Linux (CI). Record with
/// `flutter test --update-goldens` on each; the Linux set can also be taken from the
/// `mobile-golden-failures` artifact of a failed CI run. docs/mobile.md, "Goldens".
Future<void> testExecutable(FutureOr<void> Function() testMain) async {
  goldenFileComparator = _PerPlatformGoldens(goldenFileComparator);
  await testMain();
}

class _PerPlatformGoldens extends GoldenFileComparator {
  _PerPlatformGoldens(this._inner);
  final GoldenFileComparator _inner;

  static final String _os = Platform.operatingSystem;

  Uri _forPlatform(Uri golden) {
    final segments = [...golden.pathSegments];
    segments.insert(segments.length - 1, _os);
    return golden.replace(pathSegments: segments);
  }

  @override
  Future<bool> compare(Uint8List imageBytes, Uri golden) =>
      _inner.compare(imageBytes, _forPlatform(golden));

  @override
  Future<void> update(Uri golden, Uint8List imageBytes) =>
      _inner.update(_forPlatform(golden), imageBytes);

  @override
  Uri getTestUri(Uri key, int? version) => _inner.getTestUri(key, version);
}
