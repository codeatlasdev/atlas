import 'package:flutter_test/flutter_test.dart';
import 'package:image_picker_android/image_picker_android.dart';
import 'package:image_picker_platform_interface/image_picker_platform_interface.dart';
import 'package:plugin_platform_interface/plugin_platform_interface.dart';

import 'package:atlas_mobile/features/crew/chat/attachments.dart';

// Photos need no permission: the system pickers run out of process and hand over only what the person
// picks. `requestFullMetadata` (image_picker's default) makes iOS ask for the whole photo library to read
// asset metadata, which is exactly the prompt Atlas has no reason to show.

class _Platform extends ImagePickerPlatform with MockPlatformInterfaceMixin {
  final fullMetadata = <bool>[];

  @override
  Future<List<XFile>> getMultiImageWithOptions({
    MultiImagePickerOptions options = const MultiImagePickerOptions(),
  }) async {
    fullMetadata.add(options.imageOptions.requestFullMetadata);
    return [];
  }

  @override
  Future<XFile?> getImageFromSource({
    required ImageSource source,
    ImagePickerOptions options = const ImagePickerOptions(),
  }) async {
    fullMetadata.add(options.requestFullMetadata);
    return null;
  }
}

void main() {
  test(
    'the library and the camera never ask for the whole photo library',
    () async {
      final platform = _Platform();
      ImagePickerPlatform.instance = platform;
      final picker = SystemImagePicker();
      await picker.gallery();
      await picker.camera();
      expect(platform.fullMetadata, [false, false]);
    },
  );

  test('on Android the system Photo Picker is used (no media permission)', () {
    final android = ImagePickerAndroid();
    ImagePickerPlatform.instance = android;
    SystemImagePicker();
    expect(android.useAndroidPhotoPicker, isTrue);
  });
}
