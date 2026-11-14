// Goldens: flutter test --update-goldens test/floating_tab_bar_test.dart
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';

/// Loads MaterialIcons and Roboto from the Flutter SDK so goldens show real glyphs.
Future<void> _loadFonts() async {
  final candidates = [
    Platform.environment['FLUTTER_ROOT'],
    '/opt/homebrew/share/flutter',
    '/usr/local/share/flutter',
    '${Platform.environment['HOME']}/.flutter',
  ];
  String? root;
  for (final c in candidates) {
    if (c == null) continue;
    if (Directory('$c/bin/cache/artifacts/material_fonts').existsSync()) {
      root = c;
      break;
    }
  }
  if (root == null) return;

  Future<void> load(String family, String path) async {
    final f = File(path);
    if (!f.existsSync()) return;
    final loader = FontLoader(family)
      ..addFont(Future.value(f.readAsBytesSync().buffer.asByteData()));
    await loader.load();
  }

  final fonts = '$root/bin/cache/artifacts/material_fonts';
  await load('MaterialIcons', '$fonts/MaterialIcons-Regular.otf');
  await load('Roboto', '$fonts/Roboto-Regular.ttf');
  await load('Roboto', '$fonts/Roboto-Medium.ttf');
}

/// The bar alone on the app's ground, as it sits at the bottom of a screen.
class _BarFrame extends StatelessWidget {
  const _BarFrame({required this.bar});
  final FloatingTabBar bar;

  @override
  Widget build(BuildContext context) => ColoredBox(
    color: AtlasColors.of(context).ground,
    child: Center(child: bar),
  );
}

Future<void> _pump(WidgetTester t, FloatingTabBar bar) async {
  await t.binding.setSurfaceSize(const Size(420, 140));
  await t.pumpWidget(
    MaterialApp(
      theme: atlasTheme(),
      debugShowCheckedModeBanner: false,
      home: Scaffold(body: _BarFrame(bar: bar)),
    ),
  );
  await t.pumpAndSettle();
}

void main() {
  setUpAll(_loadFonts);

  testWidgets('each tab reports its own index', (t) async {
    final tapped = <int>[];
    await _pump(t, FloatingTabBar(currentIndex: 0, onTabSelected: tapped.add));
    for (final label in ['Computers', 'Atlas Crew', 'Inbox', 'Settings']) {
      await t.tap(find.bySemanticsLabel(RegExp('^$label')));
    }
    expect(tapped, [
      AtlasTab.computers.index,
      AtlasTab.crew.index,
      AtlasTab.inbox.index,
      AtlasTab.settings.index,
    ]);
  });

  testWidgets('the inbox count shows only while the inbox is not open', (
    t,
  ) async {
    await _pump(
      t,
      FloatingTabBar(currentIndex: 0, onTabSelected: (_) {}, inboxCount: 3),
    );
    expect(find.text('3'), findsOneWidget);

    // Open: it's the normal selected square, the count is gone.
    await _pump(
      t,
      FloatingTabBar(
        currentIndex: AtlasTab.inbox.index,
        onTabSelected: (_) {},
        inboxCount: 3,
      ),
    );
    expect(find.text('3'), findsNothing);

    // Nothing waiting: no pill on any tab.
    await _pump(t, FloatingTabBar(currentIndex: 0, onTabSelected: (_) {}));
    expect(find.text('0'), findsNothing);
  });

  for (final (name, index, count) in [
    ('computers', 0, 0),
    ('crew_inbox3', 1, 3),
    ('inbox_open', 2, 3),
  ]) {
    testWidgets('golden: $name', (t) async {
      await _pump(
        t,
        FloatingTabBar(
          currentIndex: index,
          onTabSelected: (_) {},
          inboxCount: count,
        ),
      );
      await expectLater(
        find.byType(_BarFrame),
        matchesGoldenFile('goldens/floating_tab_bar_$name.png'),
      );
    });
  }
}
