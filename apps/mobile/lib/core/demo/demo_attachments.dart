/// The images of the demo crew server: the pieces of an upload on their way, the bytes of
/// every image sent with a task, and hostd's rules for taking them.
library;

import 'dart:convert';
import 'dart:typed_data';

import 'package:atlas_mobile/core/demo/demo_crew.dart';
import 'package:atlas_mobile/core/protocol/image_rules.dart';

/// A real 1x1 PNG: the capture the demo agent "took", which the thread and the cards decode and draw.
final demoCapture = base64.decode(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
);

class DemoAttachments {
  DemoAttachments(this.crewmates);

  /// The crewmates whose tasks carry the attachments.
  final Map<String, DemoCrewmate> crewmates;

  /// Pieces of images on their way to a task (`crewmate_attachment_put`), by upload id.
  final _uploads = <String, BytesBuilder>{};

  /// The next piece number each upload expects.
  final _uploadSeq = <String, int>{};

  /// The bytes of every image sent with a task, by `<task id>/<name>`.
  final images = <String, ({String mime, Uint8List bytes})>{};

  /// The images of a `crewmate_task`, checked with hostd's rules: at most 4, 5 MiB each, an image by its bytes
  /// (never the declared mime), inline or from an upload staged by `crewmate_attachment_put`. Returns the task's
  /// records and keeps the bytes by task and name, or the refusal.
  ({List<Map<String, dynamic>> records, String? error}) takeImages(
    String taskId,
    List<dynamic> sent, {
    List<dynamic> existing = const [],
  }) {
    if (sent.length > maxAttachments) {
      return (
        records: const [],
        error:
            'a task takes at most $maxAttachments images, got ${sent.length}',
      );
    }
    final records = <Map<String, dynamic>>[];
    for (final raw in sent) {
      final s = (raw as Map).cast<String, dynamic>();
      final name = (s['name'] as String? ?? '').split(RegExp(r'[/\\]')).last;
      final Uint8List bytes;
      if (s['data_b64'] is String && s['upload'] == null) {
        try {
          bytes = base64.decode(s['data_b64'] as String);
        } on FormatException {
          return (records: const [], error: '"$name" is not valid base64');
        }
      } else if (s['upload'] is String && s['data_b64'] == null) {
        final staged = _uploads.remove(s['upload']);
        _uploadSeq.remove(s['upload']);
        if (staged == null) {
          return (
            records: const [],
            error:
                'upload ${s['upload']} was not found (send its pieces first)',
          );
        }
        bytes = staged.takeBytes();
      } else {
        return (
          records: const [],
          error: '"$name" needs data_b64 or upload, not both or neither',
        );
      }
      if (bytes.isEmpty) return (records: const [], error: '"$name" is empty');
      if (bytes.length > maxImageBytes) {
        return (records: const [], error: '"$name" is over 5 MiB');
      }
      final mime = sniffImage(bytes);
      if (mime == null) {
        return (
          records: const [],
          error: '"$name" is not a png, jpeg, webp or gif image',
        );
      }
      final safe = name.replaceAll(RegExp(r'[^A-Za-z0-9._-]'), '_');
      final base = safe.isEmpty || safe.startsWith('.')
          ? 'image.${imageExt[mime]}'
          : safe;
      var unique = base;
      bool taken(String n) =>
          records.any((r) => r['name'] == n) ||
          existing.any((r) => (r as Map)['name'] == n);
      for (var n = 2; taken(unique); n++) {
        final dot = base.lastIndexOf('.');
        unique = dot < 0
            ? '$base-$n'
            : '${base.substring(0, dot)}-$n${base.substring(dot)}';
      }
      images['$taskId/$unique'] = (mime: mime, bytes: bytes);
      records.add({
        'name': unique,
        'mime': mime,
        'bytes_len': bytes.length,
        'sha256': digest(bytes),
        'path': '.atlas/attachments/$taskId/$unique',
      });
    }
    return (records: records, error: null);
  }

  /// `crewmate_attachment_put`: stage the next piece of an upload. In order from 0, each at most one chunk, the
  /// whole at most 5 MiB (an upload over it is dropped), like hostd.
  String? putPiece(Map<String, dynamic> args) {
    final upload = args['upload'] as String? ?? '';
    if (!RegExp(r'^[A-Za-z0-9_-]{8,64}$').hasMatch(upload)) {
      return 'upload must be 8 to 64 letters, digits, - or _';
    }
    final seq = (args['seq'] as num?)?.toInt() ?? -1;
    final want = _uploadSeq[upload] ?? 0;
    if (seq != want) return 'expected piece $want, got $seq';
    final Uint8List piece;
    try {
      piece = base64.decode(args['data_b64'] as String? ?? '');
    } on FormatException {
      return 'piece is not valid base64';
    }
    if (piece.length > chunkBytes) {
      return 'a piece is at most ${chunkBytes ~/ 1024} KiB';
    }
    final have = _uploads.putIfAbsent(upload, BytesBuilder.new);
    if (have.length + piece.length > maxImageBytes) {
      _uploads.remove(upload);
      _uploadSeq.remove(upload);
      return 'upload is over 5 MiB';
    }
    have.add(piece);
    _uploadSeq[upload] = seq + 1;
    return null;
  }

  /// `crewmate_attachment`: a piece of a stored image by the name recorded on its task, never by a path.
  (Map<String, Object?>?, String?) attachmentPiece(Map<String, dynamic> args) {
    final taskId = args['task'] as String? ?? '';
    final name = args['name'] as String? ?? '';
    final img = images['$taskId/$name'];
    final recorded =
        crewmates[args['id'] as String?]?.tasks.any(
          (t) =>
              t['id'] == taskId &&
              ((t['attachments'] as List?) ?? const []).any(
                (a) => (a as Map)['name'] == name,
              ),
        ) ??
        false;
    if (img == null || !recorded) {
      return (null, 'task $taskId has no attachment "$name"');
    }
    final offset = ((args['offset'] as num?)?.toInt() ?? 0).clamp(
      0,
      img.bytes.length,
    );
    final limit = ((args['limit'] as num?)?.toInt() ?? chunkBytes).clamp(
      1,
      chunkBytes,
    );
    final end = (offset + limit).clamp(0, img.bytes.length);
    return (
      {
        'name': name,
        'mime': img.mime,
        'bytes_len': img.bytes.length,
        'offset': offset,
        'data_b64': base64.encode(img.bytes.sublist(offset, end)),
        'last': end >= img.bytes.length,
      },
      null,
    );
  }

  /// 64 hex characters from the bytes: the demo's stand-in for hostd's SHA-256 (nothing verifies it).
  static String digest(Uint8List bytes) {
    var h = 0xcbf29ce484222325;
    final out = StringBuffer();
    for (var round = 0; round < 4; round++) {
      for (final b in bytes) {
        h = (h ^ b) * 0x100000001b3;
      }
      h ^= round;
      out.write(h.toUnsigned(64).toRadixString(16).padLeft(16, '0'));
    }
    return out.toString();
  }
}
