/// The images of a task as a row of thumbnails: the ones the owner sent (right-aligned under their prompt) and
/// the captures the agent took (left-aligned, on its card and under its reply). Each is fetched from the crew
/// server when it first shows (`taskImageProvider`) and tapped to see large. A fetch that fails says so on that
/// image only, with a retry: a phone without the `read` scope sees exactly this, never a broken thread.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';

/// [images] of task [taskId], as thumbnails. [alignEnd] puts them under the owner's bubble (right, indented
/// like it); otherwise they sit at the left, as on a card.
class TaskImageRow extends StatelessWidget {
  const TaskImageRow({
    super.key,
    required this.images,
    required this.taskId,
    required this.device,
    required this.crewmateId,
    this.alignEnd = true,
  });

  final List<TaskAttachment> images;
  final String taskId;
  final MacDevice device;
  final String crewmateId;
  final bool alignEnd;

  @override
  Widget build(BuildContext context) {
    if (images.isEmpty) return const SizedBox.shrink();
    return Padding(
      padding: EdgeInsets.only(top: 6, left: alignEnd ? 64 : 0),
      child: Align(
        alignment: alignEnd ? Alignment.centerRight : Alignment.centerLeft,
        child: Wrap(
          spacing: 8,
          runSpacing: 8,
          alignment: alignEnd ? WrapAlignment.end : WrapAlignment.start,
          children: [
            for (final a in images)
              _TaskImage(
                attachment: a,
                request: (device, crewmateId, taskId, a.name),
              ),
          ],
        ),
      ),
    );
  }
}

class _TaskImage extends ConsumerWidget {
  const _TaskImage({required this.attachment, required this.request});
  final TaskAttachment attachment;
  final (MacDevice, String, String, String) request;
  static const double side = 112;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final c = AtlasColors.of(context);
    final image = ref.watch(taskImageProvider(request));
    Widget frame(Widget child) => ClipRRect(
      borderRadius: BorderRadius.circular(14),
      child: SizedBox.square(dimension: side, child: child),
    );
    return image.when(
      loading: () => Semantics(
        container: true,
        label: 'Loading ${attachment.name}',
        child: frame(
          ColoredBox(
            color: c.overlay,
            child: Center(
              child: SizedBox.square(
                dimension: 20,
                child: CircularProgressIndicator(
                  strokeWidth: 2,
                  color: c.faint,
                ),
              ),
            ),
          ),
        ),
      ),
      error: (e, _) => Semantics(
        container: true,
        button: true,
        label: '${attachment.name} could not be loaded, tap to retry',
        child: GestureDetector(
          onTap: () => ref.invalidate(taskImageProvider(request)),
          child: frame(
            ColoredBox(
              color: c.overlay,
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Icon(Icons.refresh_rounded, color: c.faint),
                  const SizedBox(height: 4),
                  ExcludeSemantics(
                    child: Text(
                      'Tap to retry',
                      style: TextStyle(fontSize: 11, color: c.muted),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
      data: (img) => Semantics(
        container: true,
        button: true,
        image: true,
        label: attachment.name,
        child: GestureDetector(
          onTap: () => showDialog<void>(
            context: context,
            builder: (_) => Dialog(
              backgroundColor: Colors.black,
              insetPadding: const EdgeInsets.all(12),
              child: InteractiveViewer(
                child: Image.memory(img.bytes, fit: BoxFit.contain),
              ),
            ),
          ),
          child: frame(
            Image.memory(
              img.bytes,
              fit: BoxFit.cover,
              errorBuilder: (_, _, _) => ColoredBox(
                color: c.overlay,
                child: Icon(Icons.broken_image_outlined, color: c.faint),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
