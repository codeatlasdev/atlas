/// The chat's composer: a floating pill whose send dot buds out of its right end once there is text.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat_motion.dart';
import 'package:atlas_mobile/features/crew/chat/attachments.dart';
import 'package:atlas_mobile/features/crew/chat/glass.dart';

// ── Composer: a floating pill, like the desktop's ────────────────────────────

class ChatComposer extends ConsumerStatefulWidget {
  const ChatComposer({
    super.key,
    required this.controller,
    required this.sending,
    required this.online,
    required this.queued,
    required this.onSend,
  });

  final TextEditingController controller;
  final bool sending;
  final bool online;
  final int queued;

  /// Sends the text with the images picked so far. `true`: it went, and the images are let go; `false`:
  /// it failed, and they stay for the next try.
  final Future<bool> Function(List<PendingImage> images) onSend;

  @override
  ConsumerState<ChatComposer> createState() => _ChatComposerState();
}

class _ChatComposerState extends ConsumerState<ChatComposer> {
  final List<PendingImage> _images = [];
  String? _error;

  TextEditingController get controller => widget.controller;
  bool get sending => widget.sending;
  bool get online => widget.online;
  int get queued => widget.queued;

  void _fail(String message) => setState(() => _error = message);

  Future<void> _chooseSource() async {
    final source = await showModalBottomSheet<_Source>(
      context: context,
      backgroundColor: AtlasColors.of(context).overlay,
      builder: (ctx) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(
              leading: const Icon(Icons.photo_camera_outlined),
              title: const Text('Take photo'),
              onTap: () => Navigator.of(ctx).pop(_Source.camera),
            ),
            ListTile(
              leading: const Icon(Icons.photo_library_outlined),
              title: const Text('Photo library'),
              onTap: () => Navigator.of(ctx).pop(_Source.gallery),
            ),
          ],
        ),
      ),
    );
    if (source == null || !mounted) return;
    final picker = ref.read(imagePickerProvider);
    final List<PickedImage> picked;
    try {
      picked = source == _Source.camera
          ? await picker.camera()
          : await picker.gallery();
    } on Object catch (e) {
      // A permission refused in the system sheet, or no camera: say so, never a silent nothing.
      if (mounted) _fail('Could not open the ${source.name}: $e');
      return;
    }
    if (!mounted) return;
    final r = addImages(_images, picked);
    setState(() {
      _images
        ..clear()
        ..addAll(r.images);
      _error = r.errors.isEmpty ? null : r.errors.join('\n');
    });
  }

  Future<void> _send() async {
    final images = List<PendingImage>.of(_images);
    final ok = await widget.onSend(images);
    if (ok && mounted) {
      setState(() {
        _images.clear();
        _error = null;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return SafeArea(
      top: false,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(
          Space.md,
          Space.xs,
          Space.md,
          Space.sm,
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (queued > 0)
              Padding(
                padding: const EdgeInsets.only(left: Space.lg, bottom: 6),
                child: Text(
                  queued == 1 ? '1 task queued' : '$queued tasks queued',
                  style: TextStyle(fontSize: 12, color: c.muted),
                ),
              ),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.only(left: Space.lg, bottom: 6),
                child: Text(
                  _error!,
                  style: TextStyle(fontSize: 12, color: c.error),
                ),
              ),
            if (_images.isNotEmpty)
              Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: SizedBox(
                  height: 64,
                  child: ListView.separated(
                    scrollDirection: Axis.horizontal,
                    padding: const EdgeInsets.symmetric(horizontal: 4),
                    itemCount: _images.length,
                    separatorBuilder: (_, _) => const SizedBox(width: 8),
                    itemBuilder: (_, i) {
                      final image = _images[i];
                      return _PendingThumb(
                        key: ObjectKey(image),
                        image: image,
                        onRemove: () => setState(() => _images.remove(image)),
                      );
                    },
                  ),
                ),
              ),
            // The field keeps its shape; once there is text, the send dot detaches from
            // its right end and floats beside it (_SendSlot).
            ValueListenableBuilder<TextEditingValue>(
              valueListenable: controller,
              builder: (_, value, _) {
                final ready = value.text.trim().isNotEmpty && !sending;
                return Row(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    // Attach: the camera or the photo library; the picked images wait above the field.
                    Padding(
                      padding: const EdgeInsets.only(right: 8),
                      child: GlassRoundButton(
                        icon: Icons.add_photo_alternate_outlined,
                        label: 'Attach image',
                        onTap: _images.length >= maxAttachments
                            ? () => _fail(
                                'A task takes at most $maxAttachments images',
                              )
                            : _chooseSource,
                      ),
                    ),
                    Expanded(
                      child: GlassPill(
                        child: TextField(
                          controller: controller,
                          minLines: 1,
                          maxLines: 6,
                          cursorColor: c.text,
                          textCapitalization: TextCapitalization.sentences,
                          style: TextStyle(
                            fontSize: 16,
                            color: c.text,
                            height: 1.35,
                          ),
                          decoration: InputDecoration(
                            hintText: online
                                ? 'Message'
                                : 'Offline — queued when back',
                            hintStyle: TextStyle(color: c.faint, fontSize: 16),
                            border: InputBorder.none,
                            isCollapsed: true,
                            // The app theme gives every field a filled box; the pill is it.
                            filled: false,
                            enabledBorder: InputBorder.none,
                            focusedBorder: InputBorder.none,
                            contentPadding: const EdgeInsets.fromLTRB(
                              22,
                              15,
                              22,
                              15,
                            ),
                          ),
                        ),
                      ),
                    ),
                    _SendSlot(
                      shown: ready || sending,
                      fieldColor: c.overlay.withValues(alpha: .82),
                      child: Semantics(
                        button: true,
                        label: 'Send',
                        child: Pressable(
                          onTap: ready
                              ? () {
                                  HapticFeedback.lightImpact();
                                  _send();
                                }
                              : null,
                          child: Container(
                            width: _SendSlot.dot,
                            height: _SendSlot.dot,
                            decoration: BoxDecoration(
                              color: c.accent,
                              shape: BoxShape.circle,
                            ),
                            alignment: Alignment.center,
                            child: sending
                                ? SizedBox.square(
                                    dimension: 18,
                                    child: CircularProgressIndicator(
                                      strokeWidth: 2,
                                      color: c.onAccent,
                                    ),
                                  )
                                : Icon(
                                    Icons.arrow_upward_rounded,
                                    size: 22,
                                    color: c.onAccent,
                                  ),
                          ),
                        ),
                      ),
                    ),
                  ],
                );
              },
            ),
          ],
        ),
      ),
    );
  }
}

enum _Source { camera, gallery }

/// One picked image above the field: a small square with a remove button.
class _PendingThumb extends StatelessWidget {
  const _PendingThumb({super.key, required this.image, required this.onRemove});
  final PendingImage image;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return SizedBox.square(
      dimension: 64,
      child: Stack(
        children: [
          ClipRRect(
            borderRadius: BorderRadius.circular(12),
            child: Image.memory(
              image.bytes,
              width: 64,
              height: 64,
              fit: BoxFit.cover,
              // A truncated file passes the signature check; it must not break the composer.
              errorBuilder: (_, _, _) => Container(
                width: 64,
                height: 64,
                color: c.overlay,
                child: Icon(Icons.broken_image_outlined, color: c.faint),
              ),
            ),
          ),
          // Inside the thumbnail, never poking out of the list: a badge half outside its slot lost half its
          // taps. A 32 px target around the 24 px dot.
          Positioned(
            top: 0,
            right: 0,
            child: Semantics(
              container: true,
              button: true,
              label: 'Remove ${image.name}',
              child: Pressable(
                onTap: onRemove,
                child: SizedBox.square(
                  dimension: 32,
                  child: Center(
                    child: Container(
                      width: 24,
                      height: 24,
                      decoration: BoxDecoration(
                        color: c.overlay,
                        shape: BoxShape.circle,
                        border: Border.all(color: c.border),
                      ),
                      child: Icon(Icons.close_rounded, size: 14, color: c.text),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Where the send dot lives. Empty field: no slot, the field has the whole row. Text in: the
/// dot buds out of the field's right end, stretches away on a thinning neck that snaps, and
/// springs into place while the field gives up the room. Back to empty: pulled back in.
class _SendSlot extends StatefulWidget {
  const _SendSlot({
    required this.shown,
    required this.fieldColor,
    required this.child,
  });
  final bool shown;
  final Color fieldColor;
  final Widget child;

  static const double dot = 46;
  static const double gap = 8;

  @override
  State<_SendSlot> createState() => _SendSlotState();
}

class _SendSlotState extends State<_SendSlot>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: Motion.slow * 2,
    reverseDuration: Motion.slow,
    value: widget.shown ? 1 : 0,
  );

  @override
  void didUpdateWidget(_SendSlot old) {
    super.didUpdateWidget(old);
    if (old.shown == widget.shown) return;
    if (Motion.reduced(context)) {
      _c.value = widget.shown ? 1 : 0;
    } else if (widget.shown) {
      _c.forward();
    } else {
      _c.reverse();
    }
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    const size = _SendSlot.dot, gap = _SendSlot.gap;
    return AnimatedBuilder(
      animation: _c,
      child: widget.child,
      builder: (_, child) {
        final t = _c.value;
        if (t == 0) return const SizedBox.shrink();
        // The field gives up the room smoothly while the dot travels out of it.
        final room = Curves.easeOutCubic.transform(t) * (size + gap);
        final travel = Curves.easeOutBack.transform(t);
        // The dot is never small or faded: it is a piece of the field's right end, full
        // size from the start, that pulls away to the right and snaps free.
        final dx = -(size + gap) * (1 - travel);
        // Its colour turns from the field's to the accent once it is out.
        final tint = Curves.easeIn.transform(((t - .2) / .5).clamp(0.0, 1.0));
        // The neck: full height at first (still one piece), pinching to nothing.
        final neck = (1 - t / .55).clamp(0.0, 1.0);
        final neckH = size * Curves.easeIn.transform(neck);
        return SizedBox(
          width: room,
          height: size,
          child: OverflowBox(
            alignment: Alignment.centerRight,
            minWidth: size + gap,
            maxWidth: size + gap,
            child: Stack(
              clipBehavior: Clip.none,
              alignment: Alignment.centerRight,
              children: [
                if (neckH > 0.5)
                  Positioned(
                    // From inside the field's end to the dot's centre.
                    left: -size * .5,
                    right: size / 2 - dx,
                    top: (size - neckH) / 2,
                    height: neckH,
                    child: ClipRRect(
                      borderRadius: BorderRadius.circular(neckH / 2),
                      child: ColoredBox(color: widget.fieldColor),
                    ),
                  ),
                Transform.translate(
                  offset: Offset(dx, 0),
                  child: ColorFiltered(
                    colorFilter: ColorFilter.mode(
                      widget.fieldColor.withValues(alpha: 1 - tint),
                      BlendMode.srcATop,
                    ),
                    child: child,
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}
