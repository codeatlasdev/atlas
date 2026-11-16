import 'package:material_ui/material_ui.dart';

/// A column of fixed-height strips that scrolls when the room runs out: it never takes
/// more than [share] of [bodyHeight], the room the chat body really has (the screen
/// less the app bar, the keyboard and the insets), so the timeline and the composer
/// keep the rest.
class CappedStrips extends StatelessWidget {
  const CappedStrips({
    super.key,
    required this.bodyHeight,
    required this.children,
  });
  final double bodyHeight;
  final List<Widget> children;

  /// Share of the body the strips may take.
  static const share = .25;

  @override
  Widget build(BuildContext context) {
    if (children.isEmpty) return const SizedBox.shrink();
    // Loose flex: it takes its natural height up to the cap, and gives way (scrolling)
    // when the approval and the composer below it need the room.
    return ConstrainedBox(
      constraints: BoxConstraints(maxHeight: bodyHeight * share),
      child: SingleChildScrollView(child: Column(children: children)),
    );
  }
}
