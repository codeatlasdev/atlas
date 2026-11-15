import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

/// What one tracked widget looked like on one frame.
class Seen {
  const Seen(this.rect, this.opacity);

  /// Where it was drawn, in global coordinates; `null` when it was not in the tree.
  final Rect? rect;

  /// Its effective opacity: the product of every opacity layer above it (0 when absent).
  final double opacity;

  /// Drawn and not transparent, at least partly on [screen].
  bool visibleOn(Rect screen) =>
      rect != null && opacity > 0.01 && rect!.overlaps(screen);
}

/// Frame-by-frame record of a transition, for flicker and layout-shift tests (the kind of thing a screen
/// recording shows and a final-state test never does: a page that is half visible under another, content that
/// lands and then moves, a list that shows its top before it jumps to the end).
///
/// ```dart
/// final probe = FrameProbe(t, {'crew': find.byType(CrewScreen), 'inbox': find.byType(InboxScreen)});
/// await t.tap(find.byIcon(Icons.notifications));
/// await probe.record(const Duration(milliseconds: 600));
/// probe.expectNeverBothVisible('crew', 'inbox');
/// ```
///
/// Every frame is one `pump(step)`; the first recorded frame is the one right after the action that started the
/// transition (`pump(Duration.zero)`), which is the frame a user sees first.
class FrameProbe {
  FrameProbe(this.tester, this.track);

  final WidgetTester tester;

  /// Name → finder; each must match at most one widget (the first match is taken otherwise).
  final Map<String, Finder> track;

  /// One map per recorded frame: name → what was seen.
  final List<Map<String, Seen>> frames = [];

  /// Extra values sampled per frame (a scroll offset, a count), by name.
  final List<Map<String, double>> samples = [];

  Rect get _screen {
    final view = tester.view;
    return Offset.zero & (view.physicalSize / view.devicePixelRatio);
  }

  /// Pump frames for [duration] in [step] increments, recording each one (the first at zero elapsed time).
  /// [sample] adds per-frame numbers such as a scroll offset.
  Future<void> record(
    Duration duration, {
    Duration step = const Duration(milliseconds: 16),
    Map<String, double Function()> sample = const {},
  }) async {
    var elapsed = Duration.zero;
    await tester.pump(Duration.zero);
    while (true) {
      frames.add({for (final e in track.entries) e.key: _see(e.value)});
      samples.add({for (final e in sample.entries) e.key: e.value()});
      if (elapsed >= duration) break;
      await tester.pump(step);
      elapsed += step;
    }
  }

  Seen _see(Finder finder) {
    final found = finder.evaluate();
    if (found.isEmpty) return const Seen(null, 0);
    final render = found.first.renderObject;
    if (render is! RenderBox || !render.attached || !render.hasSize) {
      return const Seen(null, 0);
    }
    final rect = render.localToGlobal(Offset.zero) & render.size;
    return Seen(rect, effectiveOpacity(render));
  }

  /// The opacity [render] is painted with: every opacity layer above it multiplied, 0 under an offstage or
  /// zero-opacity ancestor. Covers `Opacity`, `AnimatedOpacity`, `FadeTransition`, `Visibility(maintain…)` and
  /// `Offstage`.
  static double effectiveOpacity(RenderObject render) {
    var o = 1.0;
    RenderObject? node = render;
    while (node != null) {
      if (node is RenderOpacity) o *= node.opacity;
      if (node is RenderAnimatedOpacity) o *= node.opacity.value;
      if (node is RenderOffstage && node.offstage) return 0;
      node = node.parent;
    }
    return o;
  }

  /// Indexes of frames where [name] was visible.
  List<int> visibleFrames(String name) => [
    for (var i = 0; i < frames.length; i++)
      if (frames[i][name]!.visibleOn(_screen)) i,
  ];

  /// No frame shows [a] and [b] at the same time (two pages cross-fading over each other).
  void expectNeverBothVisible(String a, String b) {
    final both = [
      for (var i = 0; i < frames.length; i++)
        if (frames[i][a]!.visibleOn(_screen) &&
            frames[i][b]!.visibleOn(_screen))
          i,
    ];
    expect(
      both,
      isEmpty,
      reason:
          '"$a" and "$b" were both visible on frames $both '
          '(opacities ${[for (final i in both) '${frames[i][a]!.opacity.toStringAsFixed(2)}/${frames[i][b]!.opacity.toStringAsFixed(2)}']})',
    );
  }

  /// [name] is fully opaque on every frame it is in the tree (content that is already there must not fade in).
  void expectOpaqueWhenPresent(String name) {
    final faded = [
      for (var i = 0; i < frames.length; i++)
        if (frames[i][name]!.rect != null && frames[i][name]!.opacity < 0.99)
          '$i:${frames[i][name]!.opacity.toStringAsFixed(2)}',
    ];
    expect(
      faded,
      isEmpty,
      reason: '"$name" was not opaque on frames (frame:opacity) $faded',
    );
  }

  /// From its first frame on screen, [name] never moves or resizes by more than [tolerance] logical pixels
  /// (a layout shift: something above it arriving late, a list jumping). Frames where it is absent are ignored.
  void expectNoShift(String name, {double tolerance = 0.5}) {
    Rect? first;
    final moved = <String>[];
    for (var i = 0; i < frames.length; i++) {
      final r = frames[i][name]!.rect;
      if (r == null) continue;
      first ??= r;
      if ((r.left - first.left).abs() > tolerance ||
          (r.top - first.top).abs() > tolerance ||
          (r.width - first.width).abs() > tolerance ||
          (r.height - first.height).abs() > tolerance) {
        moved.add('$i:$r');
      }
    }
    expect(
      moved,
      isEmpty,
      reason: '"$name" first at $first, then moved on frames $moved',
    );
  }

  /// The sampled value [name] stays within [lo]..[hi] on every frame (a scroll offset that must stay at the end).
  void expectSampleWithin(String name, double lo, double hi) {
    final out = [
      for (var i = 0; i < samples.length; i++)
        if (samples[i][name]! < lo || samples[i][name]! > hi)
          '$i:${samples[i][name]!.toStringAsFixed(1)}',
    ];
    expect(out, isEmpty, reason: '"$name" left [$lo, $hi] on frames $out');
  }
}
