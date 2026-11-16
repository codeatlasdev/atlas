import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat/attachments.dart';
import 'package:atlas_mobile/features/crew/chat/composer.dart';
import 'package:atlas_mobile/features/crew/chat/crewmate_chat_screen.dart';

import 'support/demo_harness.dart';

// A real 1x1 PNG: the thread decodes what the demo serves back, so the bytes must be an image Flutter can draw.
final _png = base64.decode(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
);

/// The system pickers are the one thing replaced: they open OS UI. Everything above them is real.
class _FakePicker implements ImagePickerPort {
  _FakePicker({this.cameraShots = const [], this.galleryShots = const []});
  final List<PickedImage> cameraShots;
  final List<PickedImage> galleryShots;
  int cameraCalls = 0;
  int galleryCalls = 0;

  @override
  Future<List<PickedImage>> camera() async {
    cameraCalls++;
    return cameraShots;
  }

  @override
  Future<List<PickedImage>> gallery() async {
    galleryCalls++;
    return galleryShots;
  }
}

PickedImage _shot(String name, [Uint8List? bytes]) =>
    PickedImage(name: name, bytes: bytes ?? _png);

Widget _composer(
  _FakePicker picker, {
  required List<(String, List<PendingImage>)> sent,
  TextEditingController? controller,
}) => ProviderScope(
  overrides: [imagePickerProvider.overrideWithValue(picker)],
  child: MaterialApp(
    theme: atlasTheme(),
    home: Scaffold(
      body: Align(
        alignment: Alignment.bottomCenter,
        child: ChatComposer(
          controller: controller ?? TextEditingController(),
          sending: false,
          online: true,
          queued: 0,
          onSend: (images) async {
            sent.add(('send', images));
            return true;
          },
        ),
      ),
    ),
  ),
);

/// A computer whose phone was not granted `read`: the gateway refuses `crewmate_attachment` with
/// `forbidden: read` (hostd `mobile/session.rs`), everything else is the demo's.
class _WithoutRead implements Transport {
  _WithoutRead(this._inner) {
    _inner.incomingLines.listen(_lines.add, onDone: _lines.close);
  }
  final Transport _inner;
  final _lines = StreamController<String>.broadcast();

  @override
  Stream<String> get incomingLines => _lines.stream;

  @override
  Future<void> sendLine(String line) async {
    final req = jsonDecode(line) as Map<String, dynamic>;
    if (req['cmd'] == 'crewmate_attachment') {
      _lines.add(
        jsonEncode({'id': req['id'], 'ok': false, 'error': 'forbidden: read'}),
      );
      return;
    }
    await _inner.sendLine(line);
  }

  @override
  Future<void> close() async {
    await _lines.close();
    await _inner.close();
  }
}

void main() {
  const mac = MacDevice(nodeId: 'demo', name: 'Demo MacBook');

  group('composer: attaching', () {
    testWidgets('the attach button offers the camera and the photo library', (
      t,
    ) async {
      final picker = _FakePicker();
      await t.pumpWidget(_composer(picker, sent: []));
      await t.tap(find.bySemanticsLabel('Attach image'));
      await t.pumpAndSettle();
      expect(find.text('Take photo'), findsOneWidget);
      expect(find.text('Photo library'), findsOneWidget);
    });

    testWidgets(
      'a photo from the library shows as a thumbnail with a remove button',
      (t) async {
        final picker = _FakePicker(
          galleryShots: [_shot('a.png'), _shot('b.png')],
        );
        await t.pumpWidget(_composer(picker, sent: []));
        await t.tap(find.bySemanticsLabel('Attach image'));
        await t.pumpAndSettle();
        await t.tap(find.text('Photo library'));
        await t.pumpAndSettle();
        expect(picker.galleryCalls, 1);
        expect(find.bySemanticsLabel('Remove a.png'), findsOneWidget);
        expect(find.bySemanticsLabel('Remove b.png'), findsOneWidget);
        await t.tap(find.bySemanticsLabel('Remove a.png'));
        await t.pumpAndSettle();
        expect(find.bySemanticsLabel('Remove a.png'), findsNothing);
        expect(find.bySemanticsLabel('Remove b.png'), findsOneWidget);
      },
    );

    testWidgets('a camera photo is taken through the camera', (t) async {
      final picker = _FakePicker(cameraShots: [_shot('')]);
      await t.pumpWidget(_composer(picker, sent: []));
      await t.tap(find.bySemanticsLabel('Attach image'));
      await t.pumpAndSettle();
      await t.tap(find.text('Take photo'));
      await t.pumpAndSettle();
      expect(picker.cameraCalls, 1);
      expect(find.bySemanticsLabel(RegExp(r'^Remove photo-')), findsOneWidget);
    });

    testWidgets(
      'a file that is not an image is refused with its name and nothing is added',
      (t) async {
        final picker = _FakePicker(
          galleryShots: [
            _shot(
              'shell.png',
              Uint8List.fromList(utf8.encode('#!/bin/sh\nrm -rf ~')),
            ),
          ],
        );
        await t.pumpWidget(_composer(picker, sent: []));
        await t.tap(find.bySemanticsLabel('Attach image'));
        await t.pumpAndSettle();
        await t.tap(find.text('Photo library'));
        await t.pumpAndSettle();
        expect(
          find.textContaining('shell.png is not a png, jpeg, webp or gif'),
          findsOneWidget,
        );
        expect(find.bySemanticsLabel('Remove shell.png'), findsNothing);
      },
    );

    testWidgets('a fifth image is refused, four stay', (t) async {
      final picker = _FakePicker(
        galleryShots: [for (var i = 1; i <= 5; i++) _shot('$i.png')],
      );
      await t.pumpWidget(_composer(picker, sent: []));
      await t.tap(find.bySemanticsLabel('Attach image'));
      await t.pumpAndSettle();
      await t.tap(find.text('Photo library'));
      await t.pumpAndSettle();
      for (var i = 1; i <= 4; i++) {
        expect(find.bySemanticsLabel('Remove $i.png'), findsOneWidget);
      }
      expect(find.bySemanticsLabel('Remove 5.png'), findsNothing);
      expect(find.textContaining('at most 4'), findsOneWidget);
    });

    testWidgets('cancelling the picker changes nothing', (t) async {
      final picker = _FakePicker();
      await t.pumpWidget(_composer(picker, sent: []));
      await t.tap(find.bySemanticsLabel('Attach image'));
      await t.pumpAndSettle();
      await t.tap(find.text('Photo library'));
      await t.pumpAndSettle();
      expect(find.byType(Image), findsNothing);
    });
  });

  group('composer: sending', () {
    testWidgets(
      'send hands the text and the pending images over, and clears the images',
      (t) async {
        final sent = <(String, List<PendingImage>)>[];
        final controller = TextEditingController(text: 'the header is cut off');
        final picker = _FakePicker(galleryShots: [_shot('a.png')]);
        await t.pumpWidget(
          _composer(picker, sent: sent, controller: controller),
        );
        await t.tap(find.bySemanticsLabel('Attach image'));
        await t.pumpAndSettle();
        await t.tap(find.text('Photo library'));
        await t.pumpAndSettle();
        await t.tap(find.bySemanticsLabel('Send'));
        await t.pumpAndSettle();
        expect(sent, hasLength(1));
        expect(sent.single.$2.single.name, 'a.png');
        expect(sent.single.$2.single.mime, 'image/png');
        expect(find.bySemanticsLabel('Remove a.png'), findsNothing);
      },
    );

    testWidgets('an image alone cannot be sent: a task needs a prompt', (
      t,
    ) async {
      final sent = <(String, List<PendingImage>)>[];
      final picker = _FakePicker(galleryShots: [_shot('a.png')]);
      await t.pumpWidget(_composer(picker, sent: sent));
      await t.tap(find.bySemanticsLabel('Attach image'));
      await t.pumpAndSettle();
      await t.tap(find.text('Photo library'));
      await t.pumpAndSettle();
      expect(find.bySemanticsLabel('Send'), findsNothing);
      expect(sent, isEmpty);
    });
  });

  group('the chat against the demo computer', () {
    testWidgets(
      'an image sent from the phone shows in the thread and is fetched back from the computer',
      (t) async {
        await t.binding.setSurfaceSize(const Size(393, 852));
        final picker = _FakePicker(galleryShots: [_shot('screen.png')]);
        await t.pumpWidget(
          demoWrap(
            const CrewmateChatScreen(device: mac, crewmateId: 'crewmate-1'),
            [mac],
            overrides: [imagePickerProvider.overrideWithValue(picker)],
          ),
        );
        await settle(t);
        await t.tap(find.bySemanticsLabel('Attach image'));
        await settle(t);
        await t.tap(find.text('Photo library'));
        await settle(t);
        await t.enterText(find.byType(TextField), 'fix this header');
        await settle(t);
        await t.tap(find.bySemanticsLabel('Send'));
        await settle(t);
        // The prompt is in the thread, and so is the image: not a placeholder, an image the computer returned.
        expect(find.text('fix this header'), findsOneWidget);
        expect(find.bySemanticsLabel('screen.png'), findsOneWidget);
        expect(find.byType(Image), findsWidgets);
        await t.pumpWidget(const SizedBox());
        await settle(t);
      },
    );

    testWidgets('a plain task without images shows no image at all', (t) async {
      await t.binding.setSurfaceSize(const Size(393, 852));
      final picker = _FakePicker(galleryShots: [_shot('screen.png')]);
      await t.pumpWidget(
        ProviderScope(
          overrides: [imagePickerProvider.overrideWithValue(picker)],
          child: demoWrap(
            CrewmateChatScreen(device: mac, crewmateId: 'crewmate-1'),
            [mac],
          ),
        ),
      );
      await settle(t);
      // No attach: a plain task has no images and no image widgets at all.
      await t.enterText(find.byType(TextField), 'plain task');
      await settle(t);
      await t.tap(find.bySemanticsLabel('Send'));
      await settle(t);
      expect(find.text('plain task'), findsOneWidget);
      expect(find.bySemanticsLabel('screen.png'), findsNothing);
      await t.pumpWidget(const SizedBox());
      await settle(t);
    });

    // A phone granted `act` but not `read` can send an image (crewmate_task is Act) and then cannot fetch it
    // back: hostd answers `forbidden: read`. The thread must say so on that image, not break or hang.
    testWidgets(
      'a phone without read sees its image as not loadable, with a retry, and the rest of the thread works',
      (t) async {
        await t.binding.setSurfaceSize(const Size(393, 852));
        final picker = _FakePicker(galleryShots: [_shot('screen.png')]);
        await t.pumpWidget(
          demoWrap(
            const CrewmateChatScreen(device: mac, crewmateId: 'crewmate-1'),
            [mac],
            overrides: [imagePickerProvider.overrideWithValue(picker)],
            transport: () => _WithoutRead(demoTransport()),
          ),
        );
        await settle(t);
        await t.tap(find.bySemanticsLabel('Attach image'));
        await settle(t);
        await t.tap(find.text('Photo library'));
        await settle(t);
        await t.enterText(find.byType(TextField), 'look at this');
        await settle(t);
        await t.tap(find.bySemanticsLabel('Send'));
        await settle(t);
        expect(find.text('look at this'), findsOneWidget);
        expect(
          find.bySemanticsLabel('screen.png could not be loaded, tap to retry'),
          findsOneWidget,
        );
        expect(find.byType(Image), findsNothing);
        await t.pumpWidget(const SizedBox());
        await settle(t);
      },
    );
  });
}
