import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat/attachments.dart';

/// Where a picked image comes from.
enum PhotoSource { camera, gallery }

/// Opens the picker for [source]; throws what the picker throws (a permission refused, no camera).
Future<List<PickedImage>> pickPhotos(WidgetRef ref, PhotoSource source) {
  final picker = ref.read(imagePickerProvider);
  return source == PhotoSource.camera ? picker.camera() : picker.gallery();
}

/// Asks, in a sheet, whether the image comes from the camera or the library. Null when
/// the sheet is dismissed.
Future<PhotoSource?> askPhotoSource(
  BuildContext context, {
  String libraryLabel = 'Photo library',
}) => showModalBottomSheet<PhotoSource>(
  context: context,
  backgroundColor: AtlasColors.of(context).overlay,
  builder: (ctx) => SafeArea(
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        ListTile(
          leading: const Icon(Icons.photo_camera_outlined),
          title: const Text('Take photo'),
          onTap: () => Navigator.of(ctx).pop(PhotoSource.camera),
        ),
        ListTile(
          leading: const Icon(Icons.photo_library_outlined),
          title: Text(libraryLabel),
          onTap: () => Navigator.of(ctx).pop(PhotoSource.gallery),
        ),
      ],
    ),
  ),
);

/// Takes what a picker handed over into [images] (the caps and type checks of
/// [addImages]). Returns the line to show for what was refused, null when all went in.
String? takePicked(List<PendingImage> images, List<PickedImage> picked) {
  final r = addImages(images, picked);
  images
    ..clear()
    ..addAll(r.images);
  return r.errors.isEmpty ? null : r.errors.join('\n');
}
