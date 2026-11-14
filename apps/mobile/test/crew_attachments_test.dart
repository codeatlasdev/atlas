import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/features/crew/chat/attachments.dart';

final _png = Uint8List.fromList([
  0x89,
  0x50,
  0x4e,
  0x47,
  0x0d,
  0x0a,
  0x1a,
  0x0a,
  0,
  0,
  0,
  0x0d,
]);
final _jpg = Uint8List.fromList([0xff, 0xd8, 0xff, 0xe0, 0, 0x10]);

Map<String, dynamic> _fixture() => jsonDecode(
  File('test/fixtures/crewmate_task_attachments.json').readAsStringSync(),
) as Map<String, dynamic>;

PickedImage _pick(String name, Uint8List bytes) =>
    PickedImage(name: name, bytes: bytes);

void main() {
  group('sniffImage: the type comes from the bytes', () {
    test('each allowed kind', () {
      expect(sniffImage(_png), 'image/png');
      expect(sniffImage(_jpg), 'image/jpeg');
      expect(
        sniffImage(Uint8List.fromList(ascii.encode('GIF89a\x01\x00'))),
        'image/gif',
      );
      expect(
        sniffImage(Uint8List.fromList(ascii.encode('GIF87a\x01\x00'))),
        'image/gif',
      );
      expect(
        sniffImage(
          Uint8List.fromList([
            ...ascii.encode('RIFF'),
            0x24,
            0,
            0,
            0,
            ...ascii.encode('WEBPVP8 '),
          ]),
        ),
        'image/webp',
      );
    });

    test('everything else is refused', () {
      expect(
        sniffImage(Uint8List.fromList(ascii.encode("<svg xmlns='x'/>"))),
        isNull,
      );
      expect(sniffImage(Uint8List.fromList(ascii.encode('%PDF-1.7'))), isNull);
      expect(
        sniffImage(
          Uint8List.fromList([
            ...ascii.encode('RIFF'),
            0x24,
            0,
            0,
            0,
            ...ascii.encode('WAVEfmt '),
          ]),
        ),
        isNull,
        reason: 'RIFF is not WebP',
      );
      expect(sniffImage(Uint8List(0)), isNull);
    });
  });

  group('addImages: the composer pending list', () {
    test('keeps good images in order, type from the bytes', () {
      final r = addImages([], [_pick('a.png', _png), _pick('b.jpg', _jpg)]);
      expect(r.errors, isEmpty);
      expect(r.images.map((i) => (i.name, i.mime)), [
        ('a.png', 'image/png'),
        ('b.jpg', 'image/jpeg'),
      ]);
    });

    test('a file that is not an image is refused by name, the rest kept', () {
      final r = addImages([], [
        _pick(
          'shell.png',
          Uint8List.fromList(utf8.encode('#!/bin/sh\nrm -rf ~')),
        ),
        _pick('ok.png', _png),
      ]);
      expect(r.images.map((i) => i.name), ['ok.png']);
      expect(r.errors.single, contains('shell.png'));
    });

    test('four fit, the fifth is refused counting what is pending', () {
      final four = addImages([], [
        for (var i = 1; i <= 4; i++) _pick('$i.png', _png),
      ]);
      expect(four.images, hasLength(maxAttachments));
      final more = addImages(four.images, [_pick('5.png', _png)]);
      expect(more.images, hasLength(maxAttachments));
      expect(more.errors.single, contains('at most 4'));
    });

    test('5 MiB exactly passes, one byte more is refused', () {
      final at = Uint8List(maxImageBytes)..setAll(0, _png);
      expect(addImages([], [_pick('at.png', at)]).images, hasLength(1));
      final over = Uint8List(maxImageBytes + 1)..setAll(0, _png);
      final r = addImages([], [_pick('over.png', over)]);
      expect(r.images, isEmpty);
      expect(r.errors.single, contains('over 5 MiB'));
    });

    test('an empty file is refused', () {
      final r = addImages([], [_pick('e.png', Uint8List(0))]);
      expect(r.images, isEmpty);
      expect(r.errors, hasLength(1));
    });

    test('a camera photo with no usable name gets one', () {
      final r = addImages([], [_pick('', _png)]);
      expect(r.images.single.name, matches(RegExp(r'^photo-.*\.png$')));
    });
  });

  group('toSent: what goes on the wire', () {
    test('a small image rides inline in crewmate_task', () async {
      final calls = <(String, Map<String, dynamic>)>[];
      final sent = await toSent(
        [PendingImage(name: 'a.png', mime: 'image/png', bytes: _png)],
        (cmd, args) async {
          calls.add((cmd, args));
          return {'ok': true};
        },
        () => 'u',
        crewmateId: 'c1',
      );
      expect(calls, isEmpty);
      expect(sent, [
        {'name': 'a.png', 'mime': 'image/png', 'data_b64': base64.encode(_png)},
      ]);
    });

    test('a big image goes up in pieces of at most one chunk, in order, then is named by its upload', () async {
      final big = Uint8List(chunkBytes * 2 + 5)
        ..fillRange(0, chunkBytes * 2 + 5, 7);
      big.setAll(0, _png);
      final calls = <(String, Map<String, dynamic>)>[];
      final sent = await toSent(
        [PendingImage(name: 'big.png', mime: 'image/png', bytes: big)],
        (cmd, args) async {
          calls.add((cmd, args));
          return {'ok': true};
        },
        () => 'up-abcdef123456',
        crewmateId: 'c1',
      );
      expect(calls.map((c) => c.$1), List.filled(3, 'crewmate_attachment_put'));
      expect(calls.map((c) => c.$2['seq']), [0, 1, 2]);
      expect(calls.map((c) => c.$2['last']), [false, false, true]);
      expect(
        calls.every(
          (c) => c.$2['upload'] == 'up-abcdef123456' && c.$2['id'] == 'c1',
        ),
        isTrue,
      );
      final total = calls.fold<int>(
        0,
        (n, c) => n + base64.decode(c.$2['data_b64'] as String).length,
      );
      expect(total, big.length);
      for (final c in calls) {
        expect(
          base64.decode(c.$2['data_b64'] as String).length,
          lessThanOrEqualTo(chunkBytes),
        );
      }
      expect(sent, [
        {'name': 'big.png', 'mime': 'image/png', 'upload': 'up-abcdef123456'},
      ]);
    });

    test('a refused piece stops the upload and says why', () async {
      final big = Uint8List(chunkBytes + 1)..setAll(0, _png);
      var n = 0;
      final f = toSent(
        [PendingImage(name: 'b.png', mime: 'image/png', bytes: big)],
        (cmd, args) async => ++n == 2
            ? {'ok': false, 'error': 'upload is over 5 MiB'}
            : {'ok': true},
        () => 'up-abcdef123456',
        crewmateId: 'c1',
      );
      await expectLater(
        f,
        throwsA(
          isA<StateError>().having(
            (e) => e.message,
            'message',
            contains('over 5 MiB'),
          ),
        ),
      );
    });
  });

  group('fetchAttachment: the bytes of a stored image, piece by piece', () {
    test('reads until last, advancing the offset by what arrived', () async {
      final whole = Uint8List.fromList(List.generate(1000, (i) => i % 251));
      final asked = <int>[];
      Future<Map<String, dynamic>> call(
        String cmd,
        Map<String, dynamic> args,
      ) async {
        expect(cmd, 'crewmate_attachment');
        final offset = args['offset'] as int;
        asked.add(offset);
        final end = (offset + 400).clamp(0, whole.length);
        return {
          'ok': true,
          'data': {
            'name': args['name'],
            'mime': 'image/png',
            'bytes_len': whole.length,
            'offset': offset,
            'data_b64': base64.encode(whole.sublist(offset, end)),
            'last': end >= whole.length,
          },
        };
      }

      final r = await fetchAttachment(
        call,
        crewmateId: 'c',
        taskId: 't',
        name: 'a.png',
      );
      expect(asked, [0, 400, 800]);
      expect(r.mime, 'image/png');
      expect(r.bytes, whole);
    });

    test(
      'a refusal (a phone without read) throws with the server message',
      () async {
        final f = fetchAttachment(
          (cmd, args) async => {'ok': false, 'error': 'forbidden: read'},
          crewmateId: 'c',
          taskId: 't',
          name: 'a.png',
        );
        await expectLater(
          f,
          throwsA(
            isA<StateError>().having(
              (e) => e.message,
              'message',
              contains('forbidden'),
            ),
          ),
        );
      },
    );

    test('a server that never says last cannot make it read forever', () async {
      final f = fetchAttachment(
        (cmd, args) async => {
          'ok': true,
          'data': {
            'name': 'a.png',
            'mime': 'image/png',
            'bytes_len': 10,
            'offset': args['offset'],
            'data_b64': base64.encode(utf8.encode('xxxxxxxxxx')),
            'last': false,
          },
        },
        crewmateId: 'c',
        taskId: 't',
        name: 'a.png',
      );
      await expectLater(f, throwsA(isA<StateError>()));
    });
  });

  group('contract: the real serializer', () {
    test('a task parses with its attachment records, never bytes', () {
      final j = _fixture();
      final task = CrewmateTask.fromJson((j['task'] as Map).cast());
      expect(task.attachments, hasLength(2));
      final a = task.attachments.first;
      expect(a.name, 'IMG_0412.png');
      expect(a.mime, 'image/png');
      expect(a.bytesLen, greaterThan(0));
      expect(a.sha256, hasLength(64));
      expect(a.path, '.atlas/attachments/${task.id}/IMG_0412.png');
      expect(task.attachments.last.mime, 'image/jpeg');
    });

    test('the push event carries the same records', () {
      final j = _fixture();
      final fromEvent = CrewmateTask.fromJson(
        ((j['event'] as Map)['task'] as Map).cast(),
      );
      final fromTask = CrewmateTask.fromJson((j['task'] as Map).cast());
      expect(
        fromEvent.attachments.map((a) => a.sha256),
        fromTask.attachments.map((a) => a.sha256),
      );
    });

    test('a task without images has none (older servers send no key)', () {
      final t = CrewmateTask.fromJson({
        'id': 't',
        'ts': 1,
        'source': 'mobile',
        'prompt': 'p',
        'state': 'done',
      });
      expect(t.attachments, isEmpty);
    });

    test(
      'fetching the real crewmate_attachment reply gives the file bytes',
      () async {
        final j = _fixture();
        final task = CrewmateTask.fromJson((j['task'] as Map).cast());
        final r = await fetchAttachment(
          (cmd, args) async => {'ok': true, 'data': j['attachment']},
          crewmateId: 'c',
          taskId: task.id,
          name: task.attachments.first.name,
        );
        expect(r.mime, 'image/png');
        expect(r.bytes.length, task.attachments.first.bytesLen);
        expect(sniffImage(r.bytes), 'image/png');
      },
    );
  });
}
