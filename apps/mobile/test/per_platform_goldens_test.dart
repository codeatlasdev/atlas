import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';

import 'support/per_platform_goldens.dart';

// A new golden has no recording for the other OS yet. The comparator used to fail without writing the image it
// drew, so CI's `mobile-golden-failures` artifact was empty and a Linux golden could only be recorded on Linux
// (#658: six dictation goldens). A missing golden now leaves this run's image where Flutter leaves a mismatched
// one, and the failure says where to copy it.
void main() {
  late Directory dir;
  setUp(() => dir = Directory.systemTemp.createTempSync('goldens'));
  tearDown(() => dir.deleteSync(recursive: true));

  PerPlatformGoldens comparator() => PerPlatformGoldens(
    LocalFileComparator(dir.uri.resolve('some_test.dart')),
    os: 'linux',
  );

  final image = Uint8List.fromList(List.generate(64, (i) => i));

  test('a golden missing for this OS leaves the drawn image in failures/ and fails saying where it goes', () async {
    Object? error;
    try {
      await comparator().compare(image, Uri.parse('goldens/crew_x.png'));
    } catch (e) {
      error = e;
    }
    final drawn = File('${dir.path}/failures/crew_x_testImage.png');
    expect(drawn.readAsBytesSync(), image);
    expect('$error', contains('goldens/linux/crew_x.png'));
    expect('$error', contains(drawn.path));
  });

  test("a golden present for this OS is compared, not replaced by this run's image", () async {
    final golden = File('${dir.path}/goldens/linux/crew_x.png')
      ..createSync(recursive: true);
    golden.writeAsBytesSync([1, 2, 3]);
    await expectLater(
      comparator().compare(image, Uri.parse('goldens/crew_x.png')),
      throwsA(anything),
    );
    expect(golden.readAsBytesSync(), [1, 2, 3]);
  });

  test('--update-goldens writes the recording of this OS', () async {
    await comparator().update(Uri.parse('goldens/crew_x.png'), image);
    expect(
      File('${dir.path}/goldens/linux/crew_x.png').readAsBytesSync(),
      image,
    );
  });
}
