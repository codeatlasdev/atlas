import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/permissions/permission_ui.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat/attachments.dart';

/// Where a picked image comes from.
enum PhotoSource { camera, gallery }

/// Opens the picker for [source]; throws what the picker throws (no camera, a picker that failed). The
/// library needs no permission. The camera is asked for in context: explained first, and when it is off the
/// person gets what to do (Open Settings) and the library instead. Null: nothing was picked (Not now, the
/// sheet closed).
Future<List<PickedImage>?> pickPhotos(
  BuildContext context,
  WidgetRef ref,
  PhotoSource source,
) async {
  final picker = ref.read(imagePickerProvider);
  if (source == PhotoSource.gallery) return picker.gallery();
  final state = await askPermission(
    context,
    ref,
    AtlasPermission.camera,
    why: 'Atlas can photograph a screen or an error to send to your crewmate.',
  );
  switch (state) {
    case PermissionState.granted:
      return picker.camera();
    case PermissionState.notAsked:
      return null;
    case PermissionState.blocked ||
        PermissionState.restricted ||
        PermissionState.unavailable:
      if (!context.mounted) return null;
      final library = await showPermissionOff<bool>(
        context,
        AtlasPermission.camera,
        state,
        extra: (sheet) => [
          OutlinedButton(
            onPressed: () => Navigator.of(sheet).pop(true),
            child: const Text('Choose from the library instead'),
          ),
        ],
      );
      return library == true ? picker.gallery() : null;
  }
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
