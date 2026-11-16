import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// Pill tabs; the selected one slides under the labels.
class ProjectTabs extends StatelessWidget {
  const ProjectTabs({
    super.key,
    required this.index,
    required this.labels,
    required this.onChanged,
  });
  final int index;
  final List<String> labels;
  final void Function(int) onChanged;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Container(
      constraints: const BoxConstraints(minHeight: 48),
      padding: const EdgeInsets.symmetric(horizontal: 3),
      decoration: BoxDecoration(
        color: c.base,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: c.borderSubtle),
      ),
      child: LayoutBuilder(
        builder: (context, box) {
          final w = box.maxWidth / labels.length;
          return Stack(
            children: [
              // The pill behind the selected label: as tall as the row, whatever the text size.
              Positioned.fill(
                child: AnimatedAlign(
                  duration: Motion.slow,
                  curve: Motion.ease,
                  alignment: Alignment(
                    labels.length == 1
                        ? 0
                        : -1 + 2 * index / (labels.length - 1),
                    0,
                  ),
                  child: FractionallySizedBox(
                    widthFactor: 1 / labels.length,
                    heightFactor: 1,
                    child: Container(
                      width: w,
                      decoration: BoxDecoration(
                        color: c.raised,
                        borderRadius: BorderRadius.circular(9),
                      ),
                    ),
                  ),
                ),
              ),
              Row(
                children: [
                  for (var i = 0; i < labels.length; i++)
                    Expanded(
                      child: Semantics(
                        button: true,
                        selected: i == index,
                        child: GestureDetector(
                          behavior: HitTestBehavior.opaque,
                          onTap: () => onChanged(i),
                          child: ConstrainedBox(
                            constraints: const BoxConstraints(minHeight: 48),
                            child: Center(
                              child: AnimatedDefaultTextStyle(
                                duration: Motion.normal,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: Theme.of(context).textTheme.bodyLarge!
                                    .copyWith(
                                      color: i == index ? c.text : c.muted,
                                      fontWeight: i == index
                                          ? FontWeight.w600
                                          : FontWeight.w500,
                                    ),
                                child: Text(labels[i]),
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ],
          );
        },
      ),
    );
  }
}
