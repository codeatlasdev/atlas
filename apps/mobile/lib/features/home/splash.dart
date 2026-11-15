import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// The launch screen: the forming logo with the current stage under it.
class HomeSplash extends StatelessWidget {
  const HomeSplash({super.key, this.caption});
  final String? caption;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const AtlasLogo(size: 96, animated: true),
          const SizedBox(height: Space.xl),
          SizedBox(
            height: 20,
            child: AnimatedSwitcher(
              duration: Motion.normal,
              child: Text(
                caption ?? '',
                key: ValueKey(caption),
                style: TextStyle(color: c.muted),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
