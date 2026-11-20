import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';

/// Flutter rasterises text a few pixels differently on macOS and Linux, so one image cannot match both (0.4–3%
/// diffs on every golden with text). Each OS keeps its own recording and is compared exactly against it:
/// `goldens/x.png` is read from `goldens/macos/x.png` on a Mac and `goldens/linux/x.png` on Linux (CI). Record
/// with `flutter test --update-goldens` on each OS, or take CI's `mobile-golden-failures` artifact: a mismatch
/// and a golden missing for this OS both leave the image this run drew in `test/failures/<name>_testImage.png`.
/// docs/mobile.md, "Goldens".
class PerPlatformGoldens extends GoldenFileComparator {
  PerPlatformGoldens(this._inner, {String? os})
    : _os = os ?? Platform.operatingSystem;

  final GoldenFileComparator _inner;
  final String _os;

  Uri _forPlatform(Uri golden) {
    final segments = [...golden.pathSegments];
    segments.insert(segments.length - 1, _os);
    return golden.replace(pathSegments: segments);
  }

  @override
  Future<bool> compare(Uint8List imageBytes, Uri golden) async {
    final platform = _forPlatform(golden);
    final inner = _inner;
    if (inner is LocalFileComparator &&
        !File.fromUri(inner.basedir.resolveUri(platform)).existsSync()) {
      final name = golden.pathSegments.last.replaceFirst(RegExp(r'\.png$'), '');
      final drawn = File.fromUri(
        inner.basedir.resolve('failures/${name}_testImage.png'),
      );
      drawn.parent.createSync(recursive: true);
      drawn.writeAsBytesSync(imageBytes);
      throw FlutterError(
        'No $_os golden at ${platform.path}. The image this run drew is ${drawn.path}: check that it shows the '
        "same as the other OS's recording, then copy it to ${platform.path}.",
      );
    }
    return inner.compare(imageBytes, platform);
  }

  @override
  Future<void> update(Uri golden, Uint8List imageBytes) =>
      _inner.update(_forPlatform(golden), imageBytes);

  @override
  Uri getTestUri(Uri key, int? version) => _inner.getTestUri(key, version);
}
