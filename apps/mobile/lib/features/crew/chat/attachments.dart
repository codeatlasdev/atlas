/// Images on a crewmate task (#209), the phone's half of `crates/atlas-host/src/crew/attachments.rs`: what the
/// composer accepts (the same caps and by-the-bytes type check as hostd, so a refusal shows before the upload),
/// how an image goes on the wire, and how the thread gets its bytes back.
///
/// A protocol line is at most 1 MiB, so an image too big for one rides in pieces (`crewmate_attachment_put`) and
/// is named by its upload id in `crewmate_task`; a small one goes inline. Fetching is in pieces too. Over the
/// paired Mac this is `crew_call`; the push relay never sees an image.
library;

import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:image_picker/image_picker.dart';

import 'package:atlas_mobile/core/protocol/image_rules.dart';

export 'package:atlas_mobile/core/protocol/image_rules.dart'
    show chunkBytes, maxAttachments, maxImageBytes, sniffImage;

/// An image at most this big goes inline in `crewmate_task`; a bigger one is uploaded first.
const _inlineMax = 256 * 1024;

/// A host command call: `mux.call` in the app (`{ok, data|error}`), a fake in tests.
typedef HostCall = Future<Map<String, dynamic>> Function(
  String cmd,
  Map<String, dynamic> args,
);

/// What a picker handed over, before the checks.
class PickedImage {
  const PickedImage({required this.name, required this.bytes});
  final String name;
  final Uint8List bytes;
}

/// An image waiting in the composer: checked, with its type from the bytes.
class PendingImage {
  const PendingImage({
    required this.name,
    required this.mime,
    required this.bytes,
  });
  final String name;
  final String mime;
  final Uint8List bytes;
}

/// [picked] added to the [pending] images: the ones that pass, and a message for each that did not (not an
/// image by its bytes, empty, over 5 MiB, or past the fourth). Order is kept; nothing is dropped silently.
({List<PendingImage> images, List<String> errors}) addImages(
  List<PendingImage> pending,
  List<PickedImage> picked,
) {
  final images = [...pending];
  final errors = <String>[];
  for (final p in picked) {
    final shown = p.name.isEmpty ? 'image' : p.name;
    if (images.length >= maxAttachments) {
      errors.add(
        'A task takes at most $maxAttachments images: $shown was not added',
      );
      continue;
    }
    if (p.bytes.length > maxImageBytes) {
      errors.add('$shown is over ${maxImageBytes ~/ (1024 * 1024)} MiB');
      continue;
    }
    final mime = sniffImage(p.bytes);
    if (mime == null) {
      errors.add('$shown is not a png, jpeg, webp or gif image');
      continue;
    }
    // A camera photo can come back without a usable name.
    final name = p.name.isEmpty
        ? 'photo-${DateTime.now().millisecondsSinceEpoch}-${images.length}.${imageExt[mime]}'
        : p.name;
    images.add(PendingImage(name: name, mime: mime, bytes: p.bytes));
  }
  return (images: images, errors: errors);
}

/// A failed reply as the error it is: the server's own words.
StateError _refused(Map<String, dynamic> resp) =>
    StateError(resp['error'] as String? ?? 'the server refused');

/// The `attachments` of a `crewmate_task`. An image of at most 256 KiB goes inline; a bigger one is uploaded
/// first, piece by piece in order, and named by the upload id [newId] gave it. Throws a [StateError] with the
/// server's message when a piece is refused.
Future<List<Map<String, dynamic>>> toSent(
  List<PendingImage> images,
  HostCall call,
  String Function() newId, {
  required String crewmateId,
}) async {
  final out = <Map<String, dynamic>>[];
  for (final im in images) {
    if (im.bytes.length <= _inlineMax) {
      out.add({
        'name': im.name,
        'mime': im.mime,
        'data_b64': base64.encode(im.bytes),
      });
      continue;
    }
    final upload = newId();
    final pieces = (im.bytes.length / chunkBytes).ceil();
    for (var seq = 0; seq < pieces; seq++) {
      final end = ((seq + 1) * chunkBytes).clamp(0, im.bytes.length);
      final resp = await call('crewmate_attachment_put', {
        'id': crewmateId,
        'upload': upload,
        'seq': seq,
        'data_b64': base64.encode(im.bytes.sublist(seq * chunkBytes, end)),
        'last': seq == pieces - 1,
      });
      if (resp['ok'] != true) throw _refused(resp);
    }
    out.add({'name': im.name, 'mime': im.mime, 'upload': upload});
  }
  return out;
}

/// A fetched image: its type (from hostd, which checked the bytes) and the bytes.
class LoadedImage {
  const LoadedImage({required this.mime, required this.bytes});
  final String mime;
  final Uint8List bytes;
}

/// The bytes of a stored image: `crewmate_attachment` until `last`, the offset advancing by what arrived.
/// Throws a [StateError] with the server's message when it refuses (a phone without `read`), and when it
/// never finishes.
Future<LoadedImage> fetchAttachment(
  HostCall call, {
  required String crewmateId,
  required String taskId,
  required String name,
}) async {
  final parts = BytesBuilder(copy: false);
  var mime = '';
  // A piece is up to chunkBytes and an image at most maxImageBytes: more reads than that is a server that never ends.
  for (var reads = 0; reads <= maxImageBytes ~/ chunkBytes + 1; reads++) {
    final resp = await call('crewmate_attachment', {
      'id': crewmateId,
      'task': taskId,
      'name': name,
      'offset': parts.length,
    });
    if (resp['ok'] != true) throw _refused(resp);
    final data = (resp['data'] as Map).cast<String, dynamic>();
    final piece = base64.decode(data['data_b64'] as String);
    parts.add(piece);
    mime = data['mime'] as String? ?? mime;
    if (data['last'] == true) {
      return LoadedImage(mime: mime, bytes: parts.takeBytes());
    }
    if (piece.isEmpty || parts.length > maxImageBytes) break;
  }
  throw StateError('$name: the server never finished sending it');
}

// ── the platform boundary ────────────────────────────────────────────────────

/// Where an image comes from: the camera or the photo library. The one thing tests replace, because it opens
/// system UI; everything above it is the real code.
abstract class ImagePickerPort {
  Future<List<PickedImage>> camera();
  Future<List<PickedImage>> gallery();
}

/// The system pickers (`image_picker`). Images are scaled to at most 2048 px at 85 % quality: a phone photo is
/// 3 to 8 MB, over the 5 MiB cap, and 2048 px is plenty to read a screen.
class SystemImagePicker implements ImagePickerPort {
  SystemImagePicker([ImagePicker? picker]) : _picker = picker ?? ImagePicker();
  final ImagePicker _picker;

  Future<PickedImage?> _one(XFile? f) async => f == null
      ? null
      : PickedImage(name: f.name, bytes: await f.readAsBytes());

  @override
  Future<List<PickedImage>> camera() async {
    final f = await _picker.pickImage(
      source: ImageSource.camera,
      maxWidth: 2048,
      maxHeight: 2048,
      imageQuality: 85,
    );
    final one = await _one(f);
    return [?one];
  }

  @override
  Future<List<PickedImage>> gallery() async {
    final fs = await _picker.pickMultiImage(
      maxWidth: 2048,
      maxHeight: 2048,
      imageQuality: 85,
      limit: maxAttachments,
    );
    return [
      for (final f in fs)
        PickedImage(name: f.name, bytes: await f.readAsBytes()),
    ];
  }
}

/// The picker the composer uses; a test overrides it with a fake.
final imagePickerProvider = Provider<ImagePickerPort>(
  (ref) => SystemImagePicker(),
);
