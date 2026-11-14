/// The inset-list vocabulary of the profile: group, row, section, and the segmented pickers
/// (approval, short replies) that sit in rows.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';

/// An inset group: a title above one rounded card of rows with hairlines between them.
class ProfileGroup extends StatelessWidget {
  const ProfileGroup({super.key, required this.children, this.title});
  final String? title;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: Space.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (title != null)
            Padding(
              padding: const EdgeInsets.only(left: Space.md, bottom: 8),
              child: Text(
                title!,
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w600,
                  color: c.muted,
                ),
              ),
            ),
          ClipRRect(
            borderRadius: BorderRadius.circular(20),
            child: ColoredBox(
              color: c.raised,
              child: Column(
                children: [
                  for (var i = 0; i < children.length; i++) ...[
                    if (i > 0)
                      Divider(
                        height: 1,
                        thickness: 1,
                        indent: Space.md,
                        color: c.borderSubtle,
                      ),
                    children[i],
                  ],
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// One row of a [ProfileGroup]: a label with a control, or tappable text.
class ProfileRow extends StatelessWidget {
  const ProfileRow({
    super.key,
    this.label,
    this.icon,
    this.child,
    this.trailingText,
    this.onTap,
    this.danger = false,
  });
  final String? label;
  final IconData? icon;
  final Widget? child;
  final String? trailingText;
  final VoidCallback? onTap;
  final bool danger;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final fg = danger ? c.error : c.text;
    Widget body;
    if (label != null && child != null) {
      body = Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            label!,
            style: TextStyle(
              fontSize: 15,
              fontWeight: FontWeight.w600,
              color: fg,
            ),
          ),
          const SizedBox(height: 10),
          child!,
        ],
      );
    } else if (label != null) {
      body = Row(
        children: [
          if (icon != null) ...[
            Icon(icon, size: 20, color: danger ? c.error : c.muted),
            const SizedBox(width: 12),
          ],
          Expanded(
            child: Text(
              label!,
              style: TextStyle(
                fontSize: 15,
                fontWeight: FontWeight.w500,
                color: fg,
              ),
            ),
          ),
          if (trailingText != null)
            Text(
              trailingText!,
              style: TextStyle(fontSize: 14, color: c.accent),
            ),
          if (onTap != null && !danger) ...[
            const SizedBox(width: 4),
            Icon(Icons.chevron_right_rounded, color: c.faint, size: 20),
          ],
        ],
      );
    } else {
      body = Row(
        children: [
          Expanded(child: child ?? const SizedBox()),
          if (onTap != null)
            Icon(Icons.edit_outlined, size: 16, color: c.faint),
        ],
      );
    }
    return InkWell(
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: Space.md, vertical: 14),
        child: body,
      ),
    );
  }
}

class ProfileSection extends StatelessWidget {
  const ProfileSection({super.key, required this.title, required this.child});
  final String title;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [SectionHeader(title), child],
    );
  }
}

class ApprovalPicker extends StatelessWidget {
  const ApprovalPicker({
    super.key,
    required this.value,
    required this.onChanged,
    this.enabled = true,
  });
  final String value;
  final void Function(String) onChanged;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SegmentedChoice(
          options: const [('ask', 'Ask first'), ('auto', 'Auto-approve')],
          value: value,
          onChanged: enabled ? onChanged : null,
        ),
        if (!enabled) ...[
          const SizedBox(height: 6),
          Text(
            'OpenCode crewmates run without approval prompts.',
            style: TextStyle(color: c.muted, fontSize: 12),
          ),
        ],
      ],
    );
  }
}

class ShortRepliesRow extends StatelessWidget {
  const ShortRepliesRow({
    super.key,
    required this.value,
    required this.onChanged,
  });
  final bool? value;
  final void Function(bool?) onChanged;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                'Summarize long replies with a cheap model.',
                style: TextStyle(color: c.muted, fontSize: 13),
              ),
            ),
          ],
        ),
        const SizedBox(height: 8),
        SegmentedChoice(
          options: const [
            ('crew', 'Crew default'),
            ('on', 'On'),
            ('off', 'Off'),
          ],
          value: value == true
              ? 'on'
              : value == false
              ? 'off'
              : 'crew',
          onChanged: (v) => onChanged(
            v == 'on'
                ? true
                : v == 'off'
                ? false
                : null,
          ),
        ),
      ],
    );
  }
}

/// A pill-shaped choice: the selected option sits on a sliding rounded thumb. Labels never
/// wrap (Material's SegmentedButton broke "Crew default" in two next to its checkmark).
class SegmentedChoice extends StatelessWidget {
  const SegmentedChoice({
    super.key,
    required this.options,
    required this.value,
    this.onChanged,
  });
  final List<(String, String)> options;
  final String value;
  final void Function(String)? onChanged;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final index = options
        .indexWhere((o) => o.$1 == value)
        .clamp(0, options.length - 1);
    return Opacity(
      opacity: onChanged == null ? .5 : 1,
      child: Container(
        height: 40,
        padding: const EdgeInsets.all(3),
        decoration: BoxDecoration(
          color: c.ground,
          borderRadius: BorderRadius.circular(20),
        ),
        child: LayoutBuilder(
          builder: (_, box) {
            final w = box.maxWidth / options.length;
            return Stack(
              children: [
                AnimatedPositioned(
                  duration: Motion.normal,
                  curve: Curves.easeOutCubic,
                  left: w * index,
                  width: w,
                  top: 0,
                  bottom: 0,
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      color: c.overlay,
                      borderRadius: BorderRadius.circular(17),
                    ),
                  ),
                ),
                Row(
                  children: [
                    for (final (v, label) in options)
                      Expanded(
                        child: Semantics(
                          button: true,
                          selected: v == value,
                          child: GestureDetector(
                            behavior: HitTestBehavior.opaque,
                            onTap: onChanged == null || v == value
                                ? null
                                : () {
                                    HapticFeedback.selectionClick();
                                    onChanged!(v);
                                  },
                            child: Center(
                              child: AnimatedDefaultTextStyle(
                                duration: Motion.fast,
                                style: TextStyle(
                                  fontSize: 13.5,
                                  fontWeight: v == value
                                      ? FontWeight.w600
                                      : FontWeight.w500,
                                  color: v == value ? c.text : c.muted,
                                ),
                                child: Text(
                                  label,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
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
      ),
    );
  }
}
