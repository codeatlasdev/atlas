import 'package:flutter/material.dart';

import 'atlas_theme.dart';
import 'backdrop.dart';

/// Both tab stacks stay alive (each keeps its navigation and scroll); a switch shows exactly one page on every
/// frame: the page being left is gone on the first frame, the page being entered fades in over the app ground
/// in [Motion.fast] (instantly with Reduce motion). Never a cross-fade, never a slide: two superimposed pages
/// read as a glitch.
class FadeBranches extends StatefulWidget {
  const FadeBranches({super.key, required this.index, required this.children});
  final int index;
  final List<Widget> children;

  @override
  State<FadeBranches> createState() => _FadeBranchesState();
}

class _FadeBranchesState extends State<FadeBranches>
    with SingleTickerProviderStateMixin {
  // Settled (1) except while a page is fading in.
  late final AnimationController _fade = AnimationController(
    vsync: this,
    duration: Motion.fast,
    value: 1,
  );
  late final Animation<double> _in = CurvedAnimation(
    parent: _fade,
    curve: Motion.ease,
  );

  @override
  void didUpdateWidget(FadeBranches old) {
    super.didUpdateWidget(old);
    if (old.index == widget.index) return;
    if (Motion.reduced(context)) {
      _fade.value = 1;
    } else {
      _fade.forward(from: 0);
    }
  }

  @override
  void dispose() {
    _fade.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AtlasBackdrop(
      child: Stack(
        fit: StackFit.expand,
        children: [
          for (var i = 0; i < widget.children.length; i++)
            // Offstage keeps the page's state and scroll while it paints nothing and takes no touch; its
            // tickers stop with it. The wrapper chain is the same for every page, so a page never rebuilds
            // its state when it becomes the current one.
            Offstage(
              offstage: i != widget.index,
              child: TickerMode(
                enabled: i == widget.index,
                child: FadeTransition(
                  opacity: i == widget.index ? _in : kAlwaysCompleteAnimation,
                  child: widget.children[i],
                ),
              ),
            ),
        ],
      ),
    );
  }
}
