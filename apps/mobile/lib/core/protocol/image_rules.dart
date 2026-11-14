/// The rules for images on a crewmate task (#209), the phone's copy of
/// `crates/atlas-host/src/crew/attachments.rs`: caps and the by-the-bytes type check. Shared by the composer
/// (`features/crew/chat/attachments.dart`) and the demo computer, which enforces them like hostd.
library;

import 'dart:convert';
import 'dart:typed_data';

/// Images a task takes.
const maxAttachments = 4;

/// The largest image, in bytes (5 MiB).
const maxImageBytes = 5 * 1024 * 1024;

/// Raw bytes per protocol line (hostd's `attachments::CHUNK`): a line is at most 1 MiB, both ways.
const chunkBytes = 512 * 1024;

/// The image kind of [b] by its signature; null for anything else (an SVG, a PDF, a renamed script).
String? sniffImage(Uint8List b) {
  bool at(List<int> sig, [int from = 0]) {
    if (b.length < from + sig.length) return false;
    for (var i = 0; i < sig.length; i++) {
      if (b[from + i] != sig[i]) return false;
    }
    return true;
  }

  if (at([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])) return 'image/png';
  if (at([0xff, 0xd8, 0xff])) return 'image/jpeg';
  if (at(ascii.encode('GIF87a')) || at(ascii.encode('GIF89a'))) {
    return 'image/gif';
  }
  if (b.length >= 12 &&
      at(ascii.encode('RIFF')) &&
      at(ascii.encode('WEBP'), 8)) {
    return 'image/webp';
  }
  return null;
}

/// The extension a kind gets when a name has none that fits.
const imageExt = {
  'image/png': 'png',
  'image/jpeg': 'jpg',
  'image/gif': 'gif',
  'image/webp': 'webp',
};
