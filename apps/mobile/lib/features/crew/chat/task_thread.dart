/// One exchange of the crewmate chat: the day label, your prompt, the crewmate's reply (steps,
/// result), an inline approval and the task action card, as a column in the thread.
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/markdown/atlas_markdown.dart';
import 'package:atlas_mobile/features/crew/chat/action_card.dart';
import 'package:atlas_mobile/features/crew/chat/approval_bubble.dart';
import 'package:atlas_mobile/features/crew/chat/step_views.dart';
import 'package:atlas_mobile/features/crew/chat_motion.dart';
import 'package:atlas_mobile/features/crew/steps.dart';

String _sourceLabel(String s) => switch (s) {
  'routine' => 'routine',
  'desktop' => 'desktop',
  _ => 'mobile',
};

class CrewmateTaskPair extends StatelessWidget {
  const CrewmateTaskPair({
    super.key,
    required this.task,
    this.enter = false,
    this.delay = Duration.zero,
    this.day,
    required this.crewmateId,
    required this.device,
    required this.look,
    required this.expanded,
    required this.onToggle,
    this.pendingApproval,
    this.onApprove,
  });

  final CrewmateTask task;

  /// First time on screen: the exchange arrives (ChatAppear) instead of just being there.
  final bool enter;
  final Duration delay;

  /// The day separator to show above this exchange, when it starts a day.
  final String? day;
  final String crewmateId;
  final MacDevice device;
  final Map<String, dynamic>? look;
  final bool expanded;
  final VoidCallback onToggle;
  final CrewmatePendingApproval? pendingApproval;
  final void Function(bool)? onApprove;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (day != null)
          Padding(
            padding: const EdgeInsets.only(top: Space.md, bottom: 2),
            child: Center(
              child: Text(
                day!,
                style: TextStyle(
                  fontSize: 13,
                  color: AtlasColors.of(context).faint,
                  fontWeight: FontWeight.w500,
                ),
              ),
            ),
          ),
        _UserBubble(task: task, enter: enter, delay: delay),
        _CrewmateBubble(
          task: task,
          enter: enter,
          delay: delay + const Duration(milliseconds: 110),
          expanded: expanded,
          onToggle: onToggle,
        ),

        // ── approval bubble (inline with task) ─────────────────────────
        if (pendingApproval != null) ...[
          const SizedBox(height: 4),
          CrewmateApprovalBubble(
            look: look,
            approval: pendingApproval!,
            onApprove: onApprove ?? (_) {},
          ),
        ],

        // ── action card (waiting_action state) ─────────────────────────
        if (task.state == 'waiting_action' && task.action != null) ...[
          const SizedBox(height: 4),
          TaskActionCard(task: task, device: device, crewmateId: crewmateId),
        ],

        // ── add_trigger resolved note ──────────────────────────────────
        // Once the task is no longer waiting (done/running/etc.) and it had
        // an add_trigger action, show the one-line resolution note.
        if (task.state != 'waiting_action' &&
            task.action?.kind == 'add_trigger') ...[
          const SizedBox(height: 4),
          _AddTriggerNote(added: task.state != 'cancelled'),
        ],

        const SizedBox(height: 8),
      ],
    );
  }
}

class _UserBubble extends StatelessWidget {
  const _UserBubble({
    required this.task,
    this.enter = false,
    this.delay = Duration.zero,
  });
  final CrewmateTask task;
  final bool enter;
  final Duration delay;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Padding(
      padding: const EdgeInsets.only(top: 8, left: 64),
      child: Align(
        alignment: Alignment.centerRight,
        child: ChatAppear(
          fromRight: true,
          play: enter,
          delay: delay,
          child: _Bubble(
            mine: true,
            tag: task.source == 'mobile' ? null : _sourceLabel(task.source),
            child: Text(
              task.prompt,
              style: TextStyle(color: c.onAccent, height: 1.4, fontSize: 16),
            ),
          ),
        ),
      ),
    );
  }
}

class _CrewmateBubble extends StatelessWidget {
  const _CrewmateBubble({
    required this.task,
    this.enter = false,
    this.delay = Duration.zero,
    required this.expanded,
    required this.onToggle,
  });
  final bool enter;
  final Duration delay;
  final CrewmateTask task;
  final bool expanded;
  final VoidCallback onToggle;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);

    Widget content;
    switch (task.state) {
      case 'queued':
        content = Text(
          task.preparing != null ? 'Downloading ${task.preparing}…' : 'Queued…',
          style: TextStyle(color: c.faint, fontSize: 15),
        );

      case 'running':
        final groups = stepGroups(
          task.steps,
          true,
          groupOf: (s) => s.group,
          fallback: task.title,
        );
        content = StepGroupsView(
          groups: groups,
          running: true,
          startedAt: task.startedAt,
          expanded: expanded,
          onToggle: onToggle,
        );

      case 'failed':
        content = Text(
          task.error ?? 'Task failed',
          style: TextStyle(color: c.error, fontSize: 15),
        );

      case 'cancelled':
        content = Text(
          'Cancelled',
          style: TextStyle(color: c.faint, fontSize: 15),
        );

      case 'waiting_action':
        content = Text(
          'Waiting for input…',
          style: TextStyle(color: c.faint, fontSize: 15),
        );

      default: // done
        final groups = stepGroups(
          task.steps,
          false,
          groupOf: (s) => s.group,
          fallback: task.title,
        );
        content = Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (groups.isNotEmpty) ...[
              StepGroupsView(
                groups: groups,
                running: false,
                startedAt: task.startedAt,
                expanded: expanded,
                onToggle: onToggle,
              ),
              const SizedBox(height: 8),
            ],
            _CrewmateBubbleResult(task: task),
          ],
        );
    }

    // The crewmate's bubble: left, soft surface, its face beside the last line. The
    // content changes size smoothly as the task runs, steps come in and the reply lands.
    return Padding(
      padding: const EdgeInsets.only(top: 8),
      child: ChatAppear(
        fromRight: false,
        play: enter,
        delay: delay,
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            Flexible(
              child: _Bubble(
                mine: false,
                child: AnimatedSize(
                  duration: Motion.slow,
                  curve: Motion.ease,
                  alignment: Alignment.topLeft,
                  child: AnimatedSwitcher(
                    duration: Motion.normal,
                    layoutBuilder: (current, previous) => Stack(
                      alignment: Alignment.topLeft,
                      children: [...previous, ?current],
                    ),
                    child: KeyedSubtree(
                      key: ValueKey(task.state),
                      child: content,
                    ),
                  ),
                ),
              ),
            ),
            const SizedBox(width: 48),
          ],
        ),
      ),
    );
  }
}

/// A modern chat bubble: big radius everywhere, a smaller one on the corner that points
/// to its sender, the time tucked under the text.
class _Bubble extends StatelessWidget {
  const _Bubble({required this.mine, required this.child, this.tag});
  final bool mine;

  /// Where a message of yours came from when not this phone ("desktop", "routine").
  final String? tag;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 13),
      decoration: BoxDecoration(
        color: mine ? c.accent : c.raised,
        borderRadius: BorderRadius.circular(24),
      ),
      child: tag == null
          ? child
          : Column(
              crossAxisAlignment: CrossAxisAlignment.end,
              mainAxisSize: MainAxisSize.min,
              children: [
                child,
                const SizedBox(height: 4),
                Icon(
                  tag == 'routine'
                      ? Icons.schedule_rounded
                      : Icons.desktop_mac_rounded,
                  size: 12,
                  color: c.onAccent.withValues(alpha: .45),
                ),
              ],
            ),
    );
  }
}

class _CrewmateBubbleResult extends StatefulWidget {
  const _CrewmateBubbleResult({required this.task});
  final CrewmateTask task;
  @override
  State<_CrewmateBubbleResult> createState() => _CrewmateBubbleResultState();
}

class _CrewmateBubbleResultState extends State<_CrewmateBubbleResult> {
  bool _showFull = false;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final isSummarized =
        widget.task.summarized && widget.task.replyFull != null;
    final shown = _showFull && isSummarized
        ? widget.task.replyFull!
        : (widget.task.result ?? '');
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        AtlasMarkdown(
          shown,
          style: TextStyle(color: c.text, fontSize: 16, height: 1.45),
        ),
        if (isSummarized) ...[
          const SizedBox(height: 6),
          GestureDetector(
            onTap: () => setState(() => _showFull = !_showFull),
            child: Text(
              _showFull ? 'Show summary' : 'See full reply',
              style: TextStyle(color: c.accent, fontSize: 12),
            ),
          ),
        ],
      ],
    );
  }
}

/// One-line note shown after an add_trigger action is answered.
class _AddTriggerNote extends StatelessWidget {
  const _AddTriggerNote({required this.added});
  final bool added;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        children: [
          const SizedBox(width: 36),
          Icon(
            added ? Icons.check_circle_outline : Icons.remove_circle_outline,
            size: 13,
            color: added ? c.success : c.faint,
          ),
          const SizedBox(width: 4),
          Text(
            added ? 'Automation added' : 'Not added',
            style: TextStyle(fontSize: 12, color: added ? c.success : c.faint),
          ),
        ],
      ),
    );
  }
}
