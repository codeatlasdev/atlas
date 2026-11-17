// Regression: a link the phone could not open (no app for the scheme, or the system refused) was
// swallowed: the user tapped it and nothing happened. It now says so.
import 'package:flutter/gestures.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/markdown/atlas_markdown.dart';

const _channel = MethodChannel('plugins.flutter.io/url_launcher');

/// The system's answer to `launch`: `true` opened, `false` refused, an exception for a platform error.
void _launcher(WidgetTester t, Future<Object?> Function() answer) {
  t.binding.defaultBinaryMessenger.setMockMethodCallHandler(_channel, (call) {
    return call.method == 'launch' ? answer() : Future.value(true);
  });
  addTearDown(
    () => t.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      _channel,
      null,
    ),
  );
}

Future<void> _tapLink(WidgetTester t, String markdown) async {
  await t.pumpWidget(
    MaterialApp(
      theme: atlasTheme(),
      home: Scaffold(body: AtlasMarkdown(markdown, selectable: false)),
    ),
  );
  final link = find.byWidgetPredicate(
    (w) => w is RichText && w.text.toPlainText().contains('docs'),
  );
  TapGestureRecognizer? tap;
  void walk(InlineSpan s) {
    if (s is! TextSpan) return;
    if (s.recognizer is TapGestureRecognizer) {
      tap = s.recognizer! as TapGestureRecognizer;
    }
    s.children?.forEach(walk);
  }

  walk(t.widget<RichText>(link).text);
  expect(tap, isNotNull, reason: 'the link has a tap recognizer');
  tap!.onTap!();
  await t.pump();
  await t.pump();
}

void main() {
  testWidgets('a link the system refuses to open says so', (t) async {
    _launcher(t, () async => false);
    await _tapLink(t, 'See the [docs](https://example.com/guide).');
    expect(find.byType(SnackBar), findsOneWidget);
    expect(find.textContaining("Couldn't open"), findsOneWidget);
    expect(find.textContaining('example.com'), findsOneWidget);
  });

  testWidgets('a link whose launch throws says so', (t) async {
    _launcher(
      t,
      () async => throw PlatformException(code: 'ACTIVITY_NOT_FOUND'),
    );
    await _tapLink(t, 'See the [docs](mailto:a@b.co).');
    expect(find.byType(SnackBar), findsOneWidget);
    expect(find.textContaining("Couldn't open"), findsOneWidget);
  });

  testWidgets('a link inside a table cell says so too', (t) async {
    _launcher(t, () async => false);
    await _tapLink(
      t,
      '| a | b |\n|---|---|\n| [docs](https://example.com/guide) | x |\n',
    );
    expect(find.byType(SnackBar), findsOneWidget);
    expect(find.textContaining("Couldn't open"), findsOneWidget);
  });

  testWidgets('a link that opens shows nothing', (t) async {
    _launcher(t, () async => true);
    await _tapLink(t, 'See the [docs](https://example.com/guide).');
    expect(find.byType(SnackBar), findsNothing);
  });
}
