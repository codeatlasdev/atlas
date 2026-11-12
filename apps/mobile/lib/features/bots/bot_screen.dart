import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' show ImageFilter;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/providers/push_events_provider.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/bots/bots_provider.dart';
import 'package:atlas_mobile/features/bots/bot_profile_screen.dart';
import 'package:atlas_mobile/features/bots/steps.dart';

import 'package:atlas_mobile/core/ui/markdown/atlas_markdown.dart';
import 'package:atlas_mobile/features/bots/crew_text.dart';
import 'package:atlas_mobile/features/bots/chat_motion.dart';
import 'package:atlas_mobile/features/bots/crewmate.dart';

String _sourceLabel(String s) => switch (s) {
  'routine' => 'routine',
  'desktop' => 'desktop',
  _ => 'mobile',
};

// ── screen ────────────────────────────────────────────────────────────────────

class BotScreen extends ConsumerStatefulWidget {
  const BotScreen({
    super.key,
    required this.device,
    required this.botId,
    this.scrollToTaskId,
  });

  final MacDevice device;
  final String botId;

  /// When set, the screen scrolls to this task id after the first frame.
  final String? scrollToTaskId;

  @override
  ConsumerState<BotScreen> createState() => _BotScreenState();
}

class _BotScreenState extends ConsumerState<BotScreen> {
  final _msg = TextEditingController();
  bool _sending = false;
  final Set<String> _expanded = {};
  final _scrollController = ScrollController();
  // Task id → GlobalKey for scroll-to-task from the board.
  final Map<String, GlobalKey> _taskKeys = {};

  /// Messages already shown: each arrives once, never again when scrolled back.
  final Set<String> _arrived = {};

  @override
  void initState() {
    super.initState();
    if (widget.scrollToTaskId != null) {
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => _scrollToTask(widget.scrollToTaskId!),
      );
    }
  }

  @override
  void dispose() {
    _msg.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  void _scrollToTask(String taskId) {
    final key = _taskKeys[taskId];
    if (key?.currentContext == null) return;
    Scrollable.ensureVisible(
      key!.currentContext!,
      duration: const Duration(milliseconds: 400),
      curve: Curves.easeInOut,
      alignment: 0.1,
    );
  }

  Future<void> _send() async {
    final text = _msg.text.trim();
    if (text.isEmpty) return;
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return;
    setState(() => _sending = true);
    final resp = await mux.call('bot_task', {
      'id': widget.botId,
      'prompt': text,
      'source': 'mobile',
    });
    if (!mounted) return;
    setState(() {
      _sending = false;
      if (resp['ok'] == true) {
        _msg.clear();
        ref.invalidate(botTasksProvider((widget.device, widget.botId)));
      }
    });
  }

  Future<void> _cancel(String taskId) async {
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return;
    HapticFeedback.mediumImpact();
    await mux.call('bot_cancel', {'id': widget.botId, 'task': taskId});
    ref.invalidate(botTasksProvider((widget.device, widget.botId)));
  }

  Future<void> _approve(BotPendingApproval pa, bool allow) async {
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return;
    await mux.call('agent_approve', {
      'pane': pa.paneKey,
      'req': pa.req,
      'allow': allow,
    });
  }

  void _openProfile(BotListEntry bot, String hostName) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) =>
            BotProfileScreen(device: widget.device, botId: widget.botId),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);

    ref.listen(devicePushEventsProvider(widget.device), (_, next) {
      next.whenData((ev) {
        if (ev['ev'] == 'bot_task' || ev['ev'] == 'bot_task_update') {
          ref.invalidate(botTasksProvider((widget.device, widget.botId)));
        }
        if (ev['ev'] == 'agent_approval_done') {
          ref.invalidate(botsProvider(widget.device));
        }
      });
    });

    final botsData = ref.watch(botsProvider(widget.device)).value;
    final bot = botsData?.bots.where((b) => b.id == widget.botId).firstOrNull;
    final tasks =
        ref.watch(botTasksProvider((widget.device, widget.botId))).value ?? [];
    final online =
        ref.watch(connectionNotifierProvider(widget.device)).value != null;
    final pa = bot?.pendingApproval;
    final queued = bot?.queued ?? 0;

    // Derive approval task id from pane key.
    String? approvalTaskId;
    if (pa != null) {
      final parts = pa.paneKey.split(':');
      if (parts.length >= 3) approvalTaskId = parts[2];
    }

    final botName = bot?.name ?? '';
    final initial = botName.isNotEmpty ? botName[0].toUpperCase() : '?';
    final hostName = botsData?.hostName ?? widget.device.name;

    final state = CrewState.of(bot?.state ?? 'idle', online: online);
    final running = tasks.where((t) => t.state == 'running').firstOrNull;
    return Scaffold(
      backgroundColor: c.ground,
      extendBodyBehindAppBar: true,
      appBar: PreferredSize(
        preferredSize: const Size.fromHeight(64),
        child: SafeArea(
          bottom: false,
          child: Padding(
            padding: const EdgeInsets.fromLTRB(Space.md, 6, Space.md, 0),
            child: Row(
              children: [
                _RoundButton(
                  icon: Icons.arrow_back_ios_new_rounded,
                  label: 'Back',
                  onTap: () => Navigator.of(context).maybePop(),
                ),
                const Spacer(),
                // Who you are talking to: face and name in one floating pill.
                Semantics(
                  button: true,
                  label:
                      '$botName, ${switch (state) {
                        CrewState.working => 'working',
                        CrewState.needsYou => 'needs you',
                        CrewState.offline => 'offline',
                        CrewState.idle => 'on deck',
                      }}',
                  child: Pressable(
                    onTap: bot != null
                        ? () => _openProfile(bot, hostName)
                        : null,
                    child: _Glass(
                      padding: const EdgeInsets.fromLTRB(6, 5, 18, 5),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Crewmate(
                            seed: widget.botId,
                            look: bot?.look,
                            state: state,
                            size: 34,
                          ),
                          const SizedBox(width: 10),
                          ConstrainedBox(
                            constraints: const BoxConstraints(maxWidth: 170),
                            child: Text(
                              botName,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 17,
                                fontWeight: FontWeight.w600,
                                color: c.text,
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
                const Spacer(),
                // While it works: stop it. Otherwise nothing; the pill opens the profile.
                AnimatedSwitcher(
                  duration: Motion.normal,
                  transitionBuilder: (child, a) => ScaleTransition(
                    scale: a,
                    child: FadeTransition(opacity: a, child: child),
                  ),
                  child: running != null
                      ? _RoundButton(
                          key: const ValueKey('stop'),
                          icon: Icons.stop_rounded,
                          label: 'Stop $botName',
                          onTap: () => _cancel(running.id),
                        )
                      : const SizedBox.square(
                          key: ValueKey('none'),
                          dimension: 44,
                        ),
                ),
              ],
            ),
          ),
        ),
      ),
      body: Column(
        children: [
          // ── offline banner ─────────────────────────────────────────────
          if (!online)
            Container(
              width: double.infinity,
              color: c.warning.withValues(alpha: .12),
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
              child: Row(
                children: [
                  Icon(Icons.wifi_off_rounded, size: 14, color: c.warning),
                  const SizedBox(width: 6),
                  Flexible(
                    child: Text(
                      '${widget.device.name} is offline · history shown',
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(fontSize: 12, color: c.warning),
                    ),
                  ),
                ],
              ),
            ),

          Expanded(
            child: Stack(
              children: [
                // ── message list ───────────────────────────────────────────────
                Positioned.fill(
                  child: tasks.isEmpty
                      ? EmptyState(
                          icon: Crewmate(
                            seed: widget.botId,
                            look: bot?.look,
                            state: state,
                            size: 96,
                          ),
                          title: botName.isEmpty
                              ? 'New crewmate'
                              : 'Say hi to $botName',
                          message: 'Give it a job. It works on its computer and tells you when it is done.',
                        )
                      : ListView.builder(
                          reverse: true,
                          controller: _scrollController,
                          physics: const BouncingScrollPhysics(
                            parent: AlwaysScrollableScrollPhysics(),
                          ),
                          padding: EdgeInsets.fromLTRB(
                            Space.md,
                            MediaQuery.paddingOf(context).top + 72,

                            Space.md,
                            // Room for the composer floating over the end of the thread.
                            Space.sm +
                                68 +
                                MediaQuery.paddingOf(context).bottom +
                                (running != null ? 40 : 0),
                          ),
                          itemCount: tasks.length,
                          itemBuilder: (_, i) {
                            final task = tasks[i];
                            final key = _taskKeys.putIfAbsent(
                              task.id,
                              GlobalKey.new,
                            );
                            final isApprovalTask =
                                approvalTaskId != null &&
                                task.id == approvalTaskId &&
                                pa != null;
                            // Newest first: a day's label goes above its oldest message.
                            final day =
                                i == tasks.length - 1 ||
                                crewDayOf(task.ts) !=
                                    crewDayOf(tasks[i + 1].ts);
                            final enter = _arrived.add(task.id);
                            // Opening the chat: the newest few land in quick succession.
                            final delay = Duration(
                              milliseconds: i < 6 ? i * 55 : 0,
                            );
                            return _TaskPair(
                              key: key,
                              task: task,
                              enter: enter,
                              delay: delay,
                              day: day
                                  ? crewDayLabel(task.ts, DateTime.now())
                                  : null,
                              botId: widget.botId,
                              botName: botName,
                              device: widget.device,
                              look: bot?.look,
                              initial: initial,
                              pendingApproval: isApprovalTask ? pa : null,
                              expanded: _expanded.contains(task.id),
                              onToggle: () => setState(() {
                                _expanded.contains(task.id)
                                    ? _expanded.remove(task.id)
                                    : _expanded.add(task.id);
                              }),
                              onApprove: isApprovalTask
                                  ? (allow) => _approve(pa, allow)
                                  : null,
                            );
                          },
                        ),
                ),

                // The composer floats over the thread, no band of its own: the
                // messages scroll on behind it.
                Positioned(
                  left: 0,
                  right: 0,
                  bottom: 0,
                  child: DecoratedBox(
                    // The thread fades out under the composer instead of being cut.
                    decoration: BoxDecoration(
                      gradient: LinearGradient(
                        begin: Alignment.topCenter,
                        end: Alignment.bottomCenter,
                        colors: [c.ground.withValues(alpha: 0), c.ground],
                        stops: const [0, .45],
                      ),
                    ),
                    child: Padding(
                      padding: const EdgeInsets.only(top: Space.lg),
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          // ── global approval banner (if not matched to a task) ──────────
                          if (pa != null && approvalTaskId == null)
                            Padding(
                              padding: const EdgeInsets.symmetric(
                                horizontal: Space.md,
                              ),
                              child: _ApprovalBubble(
                                botName: botName,
                                look: bot?.look,
                                initial: initial,
                                approval: pa,
                                onApprove: (allow) => _approve(pa, allow),
                              ),
                            ),

                          AnimatedSwitcher(
                            duration: Motion.normal,
                            transitionBuilder: (child, a) => SizeTransition(
                              sizeFactor: a,
                              alignment: Alignment.bottomCenter,
                              child: FadeTransition(opacity: a, child: child),
                            ),
                            child: bot != null && running != null
                                ? _Typing(
                                    key: const ValueKey('typing'),
                                    bot: bot,
                                    since: running.startedAt,
                                  )
                                : const SizedBox(
                                    key: ValueKey('idle'),
                                    width: double.infinity,
                                  ),
                          ),

                          // ── composer ──────────────────────────────────────────────────
                          _Composer(
                            controller: _msg,
                            sending: _sending,
                            online: online,
                            queued: queued,
                            hintName: botName,
                            onSend: _send,
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

// ── Task pair: user prompt + bot reply ────────────────────────────────────────

class _TaskPair extends StatelessWidget {
  const _TaskPair({
    super.key,
    required this.task,
    this.enter = false,
    this.delay = Duration.zero,
    this.day,
    required this.botId,
    required this.botName,
    required this.device,
    required this.look,
    required this.initial,
    required this.expanded,
    required this.onToggle,
    this.pendingApproval,
    this.onApprove,
  });

  final BotTask task;

  /// First time on screen: the exchange arrives (ChatAppear) instead of just being there.
  final bool enter;
  final Duration delay;

  /// The day separator to show above this exchange, when it starts a day.
  final String? day;
  final String botId;
  final String botName;
  final MacDevice device;
  final Map<String, dynamic>? look;
  final String initial;
  final bool expanded;
  final VoidCallback onToggle;
  final BotPendingApproval? pendingApproval;
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
        _BotBubble(
          task: task,
          enter: enter,
          delay: delay + const Duration(milliseconds: 110),
          botId: botId,
          botName: botName,
          look: look,
          expanded: expanded,
          onToggle: onToggle,
        ),

        // ── approval bubble (inline with task) ─────────────────────────
        if (pendingApproval != null) ...[
          const SizedBox(height: 4),
          _ApprovalBubble(
            botName: botName,
            look: look,
            initial: initial,
            approval: pendingApproval!,
            onApprove: onApprove ?? (_) {},
          ),
        ],

        // ── action card (waiting_action state) ─────────────────────────
        if (task.state == 'waiting_action' && task.action != null) ...[
          const SizedBox(height: 4),
          _ActionCard(task: task, device: device, botId: botId),
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

// ── Your bubble: right, in the accent ────────────────────────────────────────

class _UserBubble extends StatelessWidget {
  const _UserBubble({
    required this.task,
    this.enter = false,
    this.delay = Duration.zero,
  });
  final BotTask task;
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

// ── Bot bubble (left side) ────────────────────────────────────────────────────

class _BotBubble extends StatelessWidget {
  const _BotBubble({
    required this.task,
    this.enter = false,
    this.delay = Duration.zero,
    required this.botId,
    required this.botName,
    required this.look,
    required this.expanded,
    required this.onToggle,
  });
  final bool enter;
  final Duration delay;
  final BotTask task;
  final String botId;
  final String botName;
  final Map<String, dynamic>? look;
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
        content = _StepGroupsView(
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
              _StepGroupsView(
                groups: groups,
                running: false,
                startedAt: task.startedAt,
                expanded: expanded,
                onToggle: onToggle,
              ),
              const SizedBox(height: 8),
            ],
            _BotBubbleResult(task: task),
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

// ── Step groups view ──────────────────────────────────────────────────────────

class _StepGroupsView extends StatelessWidget {
  const _StepGroupsView({
    required this.groups,
    required this.running,
    required this.startedAt,
    required this.expanded,
    required this.onToggle,
  });

  final List<StepGroup<BotStep>> groups;
  final bool running;
  final int? startedAt;
  final bool expanded;
  final VoidCallback onToggle;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    if (groups.isEmpty) {
      return Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (running) ...[
            SizedBox.square(
              dimension: 12,
              child: CircularProgressIndicator(strokeWidth: 2, color: c.accent),
            ),
            const SizedBox(width: 8),
          ],
          Text(
            running
                ? (startedAt != null
                      ? 'Working · ${crewElapsed(startedAt!, DateTime.now())}'
                      : 'Working…')
                : 'Done',
            style: TextStyle(color: running ? c.accent : c.muted, fontSize: 16),
          ),
        ],
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (running) ...[
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              SizedBox.square(
                dimension: 12,
                child: CircularProgressIndicator(
                  strokeWidth: 2,
                  color: c.accent,
                ),
              ),
              const SizedBox(width: 8),
              Text(
                startedAt != null
                    ? 'Working · ${crewElapsed(startedAt!, DateTime.now())}'
                    : 'Working…',
                style: TextStyle(color: c.accent, fontSize: 16),
              ),
            ],
          ),
          const SizedBox(height: 6),
        ],
        for (var gi = 0; gi < groups.length; gi++) ...[
          _StepGroupRow(
            group: groups[gi],
            isLast: gi == groups.length - 1,
            running: running,
            expanded: expanded,
            onToggle: onToggle,
          ),
          if (gi < groups.length - 1) const SizedBox(height: 4),
        ],
      ],
    );
  }
}

// ── One step group row ────────────────────────────────────────────────────────

class _StepGroupRow extends StatelessWidget {
  const _StepGroupRow({
    required this.group,
    required this.isLast,
    required this.running,
    required this.expanded,
    required this.onToggle,
  });

  final StepGroup<BotStep> group;
  final bool isLast;
  final bool running;
  final bool expanded;
  final VoidCallback onToggle;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final title = group.title ?? (running ? null : 'Steps');

    if (group.done) {
      // Finished group: '✓ Title · N steps', tap to expand.
      return GestureDetector(
        onTap: onToggle,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(Icons.check_circle_outline, size: 13, color: c.success),
                const SizedBox(width: 4),
                Flexible(
                  child: Text(
                    title != null
                        ? '$title · ${group.steps.length} step${group.steps.length == 1 ? '' : 's'}'
                        : '${group.steps.length} step${group.steps.length == 1 ? '' : 's'}',
                    style: TextStyle(fontSize: 12, color: c.muted),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                const SizedBox(width: 2),
                Icon(
                  expanded
                      ? Icons.keyboard_arrow_up
                      : Icons.keyboard_arrow_down,
                  size: 14,
                  color: c.faint,
                ),
              ],
            ),
            if (expanded) ...[
              const SizedBox(height: 4),
              _StepList(steps: group.steps),
            ],
          ],
        ),
      );
    }

    // Current (open) group: title + last 6 steps, '+N earlier' if more.
    const kMax = 6;
    final shown = group.steps.length > kMax
        ? group.steps.sublist(group.steps.length - kMax)
        : group.steps;
    final earlier = group.steps.length - shown.length;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (title != null) ...[
          Text(
            title,
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.w600,
              color: c.muted,
            ),
          ),
          const SizedBox(height: 4),
        ],
        if (earlier > 0) ...[
          Text(
            '+$earlier earlier',
            style: TextStyle(
              fontSize: 11,
              color: c.faint,
              fontStyle: FontStyle.italic,
            ),
          ),
          const SizedBox(height: 2),
        ],
        _StepList(steps: shown),
      ],
    );
  }
}

// ── Step list ─────────────────────────────────────────────────────────────────

class _StepList extends StatelessWidget {
  const _StepList({required this.steps});
  final List<BotStep> steps;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (final step in steps)
          Padding(
            padding: const EdgeInsets.only(bottom: 2),
            child: Text(
              '· ${stepPhrase(step.text)}',
              style: TextStyle(
                fontSize: 12,
                color: c.muted,
                fontFamily: 'Menlo',
                fontFamilyFallback: const [
                  'SF Mono',
                  'RobotoMono',
                  'monospace',
                ],
              ),
            ),
          ),
      ],
    );
  }
}

class _BotBubbleResult extends StatefulWidget {
  const _BotBubbleResult({required this.task});
  final BotTask task;
  @override
  State<_BotBubbleResult> createState() => _BotBubbleResultState();
}

class _BotBubbleResultState extends State<_BotBubbleResult> {
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

// ── Approval bubble (bot-side, with Deny/Allow) ───────────────────────────────

class _ApprovalBubble extends StatelessWidget {
  const _ApprovalBubble({
    required this.botName,
    required this.look,
    required this.initial,
    required this.approval,
    required this.onApprove,
  });

  final String botName;
  final Map<String, dynamic>? look;
  final String initial;
  final BotPendingApproval approval;
  final void Function(bool) onApprove;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final input = approval.toolInput;
    final cmd = input is Map && input['command'] is String
        ? input['command'] as String
        : null;

    const r = Radius.circular(18);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          Crewmate(
            seed: approval.paneKey,
            look: look,
            state: CrewState.needsYou,
            size: 30,
          ),
          const SizedBox(width: 8),
          Flexible(
            child: Container(
              constraints: BoxConstraints(
                maxWidth: MediaQuery.of(context).size.width * 0.8,
              ),
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: c.warning.withValues(alpha: .1),
                border: Border.all(color: c.warning.withValues(alpha: .4)),
                borderRadius: const BorderRadius.only(
                  topLeft: Radius.circular(4),
                  topRight: r,
                  bottomLeft: r,
                  bottomRight: r,
                ),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Icon(Icons.shield_outlined, size: 14, color: c.warning),
                      const SizedBox(width: 4),
                      Flexible(
                        child: Text(
                          'Wants to run ${approval.toolName ?? 'a tool'}',
                          style: TextStyle(
                            color: c.warning,
                            fontSize: 13,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ),
                    ],
                  ),
                  if (cmd != null) ...[
                    const SizedBox(height: 6),
                    Container(
                      width: double.infinity,
                      padding: const EdgeInsets.symmetric(
                        horizontal: 10,
                        vertical: 6,
                      ),
                      decoration: BoxDecoration(
                        color: c.ground,
                        borderRadius: BorderRadius.circular(8),
                      ),
                      child: Text(
                        cmd,
                        style: TextStyle(
                          fontFamily: 'Menlo',
                          fontFamilyFallback: const [
                            'SF Mono',
                            'RobotoMono',
                            'monospace',
                          ],
                          fontSize: 12,
                          color: c.text,
                        ),
                      ),
                    ),
                  ],
                  const SizedBox(height: 8),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.end,
                    children: [
                      OutlinedButton(
                        onPressed: () => onApprove(false),
                        style: OutlinedButton.styleFrom(
                          foregroundColor: c.error,
                          side: BorderSide(
                            color: c.error.withValues(alpha: .5),
                          ),
                          padding: const EdgeInsets.symmetric(
                            horizontal: 14,
                            vertical: 5,
                          ),
                          minimumSize: Size.zero,
                          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                          shape: RoundedRectangleBorder(
                            borderRadius: BorderRadius.circular(20),
                          ),
                        ),
                        child: const Text('Deny'),
                      ),
                      const SizedBox(width: 6),
                      FilledButton(
                        onPressed: () => onApprove(true),
                        style: FilledButton.styleFrom(
                          backgroundColor: c.warning,
                          foregroundColor: c.ground,
                          padding: const EdgeInsets.symmetric(
                            horizontal: 14,
                            vertical: 5,
                          ),
                          minimumSize: Size.zero,
                          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                          shape: RoundedRectangleBorder(
                            borderRadius: BorderRadius.circular(20),
                          ),
                        ),
                        child: const Text('Allow'),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

// ── "<name> is working" with the crewmate busy ───────────────────────────────

class _Typing extends StatelessWidget {
  const _Typing({super.key, required this.bot, required this.since});
  final BotListEntry bot;
  final int? since;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final elapsed = since == null
        ? ''
        : ' · ${crewElapsed(since!, DateTime.now())}';
    return Padding(
      padding: const EdgeInsets.fromLTRB(Space.md, 0, Space.md, Space.xs),
      child: Row(
        children: [
          Crewmate(
            seed: bot.id,
            look: bot.look,
            state: CrewState.working,
            size: 30,
          ),
          const SizedBox(width: Space.sm),
          Text(
            '${bot.name} is working$elapsed',
            style: TextStyle(
              fontSize: 12,
              color: c.accent,
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(width: 6),
          const _Dots(),
        ],
      ),
    );
  }
}

/// Three dots rising in turn (the desktop's typing indicator).
class _Dots extends StatefulWidget {
  const _Dots();
  @override
  State<_Dots> createState() => _DotsState();
}

class _DotsState extends State<_Dots> with SingleTickerProviderStateMixin {
  late final _c = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1100),
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    Motion.reduced(context) ? _c.stop() : _c.repeat();
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final color = AtlasColors.of(context).accent;
    return AnimatedBuilder(
      animation: _c,
      builder: (_, _) => Row(
        children: [
          for (var i = 0; i < 3; i++)
            Transform.translate(
              offset: Offset(
                0,
                math.max(0, math.sin((_c.value - i * 0.15) * 2 * math.pi)) * -3,
              ),
              child: Container(
                width: 4,
                height: 4,
                margin: const EdgeInsets.symmetric(horizontal: 1.5),
                decoration: BoxDecoration(color: color, shape: BoxShape.circle),
              ),
            ),
        ],
      ),
    );
  }
}

// ── Composer: a floating pill, like the desktop's ────────────────────────────

class _Composer extends StatelessWidget {
  const _Composer({
    required this.controller,
    required this.sending,
    required this.online,
    required this.queued,
    required this.hintName,
    required this.onSend,
  });

  final TextEditingController controller;
  final bool sending;
  final bool online;
  final int queued;
  final String hintName;
  final VoidCallback onSend;

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
                  style: TextStyle(fontSize: 12, color: c.faint),
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
                    Expanded(
                      child: _Glass(
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
                                  onSend();
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

// ── Action card (waiting_action state) ───────────────────────────────────────

class _ActionCard extends ConsumerStatefulWidget {
  const _ActionCard({
    required this.task,
    required this.device,
    required this.botId,
  });

  final BotTask task;
  final MacDevice device;
  final String botId;

  @override
  ConsumerState<_ActionCard> createState() => _ActionCardState();
}

class _ActionCardState extends ConsumerState<_ActionCard> {
  final _controller = TextEditingController();
  bool _submitting = false;

  // GitHub device flow state (for connect_github kind)
  ShipLoginFlow? _flow;
  bool _polling = false;
  Timer? _pollTimer;

  @override
  void dispose() {
    _controller.dispose();
    _pollTimer?.cancel();
    super.dispose();
  }

  Future<void> _submit(String value) async {
    if (value.isEmpty || _submitting) return;
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return;
    setState(() => _submitting = true);
    try {
      await mux.call('bot_action_answer', {
        'id': widget.botId,
        'task': widget.task.id,
        'value': value,
      });
      _controller.clear();
    } finally {
      if (mounted) setState(() => _submitting = false);
    }
  }

  Future<void> _cancel() async {
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return;
    await mux.call('bot_cancel', {'id': widget.botId, 'task': widget.task.id});
    if (mounted) {
      ref.invalidate(botTasksProvider((widget.device, widget.botId)));
    }
  }

  Future<void> _startGithubLogin() async {
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return;
    final resp = await mux.call('bot_github_login_start', {});
    if (!mounted) return;
    if (resp['ok'] != true) return;
    final data = (resp['data'] as Map?)?.cast<String, dynamic>() ?? {};
    setState(() {
      _flow = ShipLoginFlow.fromJson(data);
      _polling = true;
    });
    _pollTimer?.cancel();
    _pollTimer = Timer.periodic(
      const Duration(seconds: 4),
      (_) => _pollLogin(),
    );
  }

  Future<void> _pollLogin() async {
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return;
    final resp = await mux.call('bot_github_login_poll', {});
    if (!mounted) return;
    final data = (resp['data'] as Map?)?.cast<String, dynamic>() ?? {};
    final state = data['state'] as String? ?? 'pending';
    if (state == 'done') {
      _pollTimer?.cancel();
      setState(() {
        _flow = null;
        _polling = false;
      });
      // Wake the runner
      final mux2 = ref.read(connectionNotifierProvider(widget.device)).value;
      await mux2?.call('bot_github_connect_resume', {});
    } else if (state == 'error') {
      _pollTimer?.cancel();
      setState(() {
        _flow = null;
        _polling = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final action = widget.task.action;
    if (action == null) return const SizedBox.shrink();

    const r = Radius.circular(18);
    final cardDecoration = BoxDecoration(
      color: c.warning.withValues(alpha: .08),
      border: Border.all(color: c.warning.withValues(alpha: .35)),
      borderRadius: const BorderRadius.only(
        topLeft: Radius.circular(4),
        topRight: r,
        bottomLeft: r,
        bottomRight: r,
      ),
    );

    Widget cardContent;

    switch (action.kind) {
      case 'connect_github':
        final flow = _flow;
        cardContent = Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Connect GitHub to continue',
              style: TextStyle(
                fontSize: 13,
                fontWeight: FontWeight.w600,
                color: c.text,
              ),
            ),
            if (action.reason != null) ...[
              const SizedBox(height: 4),
              Text(
                action.reason!,
                style: TextStyle(fontSize: 12, color: c.muted),
              ),
            ],
            const SizedBox(height: 10),
            if (flow != null) ...[
              Text(
                'Enter this code at ${flow.verificationUri}',
                style: TextStyle(fontSize: 12, color: c.muted),
              ),
              const SizedBox(height: 6),
              Row(
                children: [
                  Text(
                    flow.userCode,
                    style: TextStyle(
                      fontSize: 20,
                      fontWeight: FontWeight.w700,
                      letterSpacing: 2,
                      fontFamily: 'Menlo',
                      fontFamilyFallback: const [
                        'SF Mono',
                        'RobotoMono',
                        'monospace',
                      ],
                      color: c.text,
                    ),
                  ),
                  const SizedBox(width: 8),
                  OutlinedButton(
                    onPressed: () {
                      // clipboard
                    },
                    style: OutlinedButton.styleFrom(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 10,
                        vertical: 4,
                      ),
                      minimumSize: Size.zero,
                      tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(8),
                      ),
                    ),
                    child: const Text('Copy', style: TextStyle(fontSize: 12)),
                  ),
                ],
              ),
              if (_polling) ...[
                const SizedBox(height: 6),
                Text(
                  'Waiting for authorization…',
                  style: TextStyle(fontSize: 12, color: c.muted),
                ),
              ],
              const SizedBox(height: 8),
              OutlinedButton(
                onPressed: () {
                  _pollTimer?.cancel();
                  setState(() {
                    _flow = null;
                    _polling = false;
                  });
                },
                style: OutlinedButton.styleFrom(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 4,
                  ),
                  minimumSize: Size.zero,
                  tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(20),
                  ),
                ),
                child: const Text('Cancel', style: TextStyle(fontSize: 13)),
              ),
            ] else ...[
              Row(
                children: [
                  FilledButton(
                    onPressed: _submitting ? null : _startGithubLogin,
                    style: FilledButton.styleFrom(
                      backgroundColor: c.warning,
                      foregroundColor: c.ground,
                      padding: const EdgeInsets.symmetric(
                        horizontal: 14,
                        vertical: 6,
                      ),
                      minimumSize: Size.zero,
                      tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(20),
                      ),
                    ),
                    child: const Text('Connect'),
                  ),
                  const SizedBox(width: 8),
                  OutlinedButton(
                    onPressed: _submitting ? null : _cancel,
                    style: OutlinedButton.styleFrom(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 14,
                        vertical: 6,
                      ),
                      minimumSize: Size.zero,
                      tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(20),
                      ),
                    ),
                    child: const Text(
                      'Cancel task',
                      style: TextStyle(fontSize: 13),
                    ),
                  ),
                ],
              ),
            ],
          ],
        );

      case 'provide_secret':
        cardContent = Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              action.label ?? 'Secret required',
              style: TextStyle(
                fontSize: 13,
                fontWeight: FontWeight.w600,
                color: c.text,
              ),
            ),
            if (action.reason != null) ...[
              const SizedBox(height: 4),
              Text(
                action.reason!,
                style: TextStyle(fontSize: 12, color: c.muted),
              ),
            ],
            const SizedBox(height: 8),
            TextField(
              controller: _controller,
              obscureText: true,
              style: TextStyle(fontSize: 14, color: c.text),
              decoration: InputDecoration(
                hintText: 'Enter value',
                hintStyle: TextStyle(color: c.faint, fontSize: 14),
                contentPadding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 8,
                ),
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(8),
                  borderSide: BorderSide(color: c.borderSubtle),
                ),
                enabledBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(8),
                  borderSide: BorderSide(color: c.borderSubtle),
                ),
              ),
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                FilledButton(
                  onPressed: _submitting
                      ? null
                      : () {
                          final v = _controller.text.trim();
                          if (v.isNotEmpty) _submit(v);
                        },
                  style: FilledButton.styleFrom(
                    backgroundColor: c.warning,
                    foregroundColor: c.ground,
                    padding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 6,
                    ),
                    minimumSize: Size.zero,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(20),
                    ),
                  ),
                  child: const Text('Submit'),
                ),
                const SizedBox(width: 8),
                OutlinedButton(
                  onPressed: _submitting ? null : _cancel,
                  style: OutlinedButton.styleFrom(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 6,
                    ),
                    minimumSize: Size.zero,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(20),
                    ),
                  ),
                  child: const Text('Cancel', style: TextStyle(fontSize: 13)),
                ),
              ],
            ),
          ],
        );

      case 'input':
        cardContent = Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              action.label ?? 'Input required',
              style: TextStyle(
                fontSize: 13,
                fontWeight: FontWeight.w600,
                color: c.text,
              ),
            ),
            if (action.reason != null) ...[
              const SizedBox(height: 4),
              Text(
                action.reason!,
                style: TextStyle(fontSize: 12, color: c.muted),
              ),
            ],
            const SizedBox(height: 8),
            TextField(
              controller: _controller,
              style: TextStyle(fontSize: 14, color: c.text),
              decoration: InputDecoration(
                hintText: action.placeholder ?? '',
                hintStyle: TextStyle(color: c.faint, fontSize: 14),
                contentPadding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 8,
                ),
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(8),
                  borderSide: BorderSide(color: c.borderSubtle),
                ),
                enabledBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(8),
                  borderSide: BorderSide(color: c.borderSubtle),
                ),
              ),
              onSubmitted: (v) {
                if (v.trim().isNotEmpty) _submit(v.trim());
              },
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                FilledButton(
                  onPressed: _submitting
                      ? null
                      : () {
                          final v = _controller.text.trim();
                          if (v.isNotEmpty) _submit(v);
                        },
                  style: FilledButton.styleFrom(
                    backgroundColor: c.warning,
                    foregroundColor: c.ground,
                    padding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 6,
                    ),
                    minimumSize: Size.zero,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(20),
                    ),
                  ),
                  child: const Text('Submit'),
                ),
                const SizedBox(width: 8),
                OutlinedButton(
                  onPressed: _submitting ? null : _cancel,
                  style: OutlinedButton.styleFrom(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 6,
                    ),
                    minimumSize: Size.zero,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(20),
                    ),
                  ),
                  child: const Text('Cancel', style: TextStyle(fontSize: 13)),
                ),
              ],
            ),
          ],
        );

      case 'choose':
        cardContent = Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              action.question ?? 'Choose an option',
              style: TextStyle(
                fontSize: 13,
                fontWeight: FontWeight.w600,
                color: c.text,
              ),
            ),
            const SizedBox(height: 8),
            for (int i = 0; i < action.options.length; i++) ...[
              OutlinedButton(
                onPressed: _submitting
                    ? null
                    : () => _submit(action.options[i]),
                style: OutlinedButton.styleFrom(
                  foregroundColor: c.text,
                  side: BorderSide(color: c.borderSubtle),
                  padding: const EdgeInsets.symmetric(
                    horizontal: 14,
                    vertical: 8,
                  ),
                  minimumSize: const Size(double.infinity, 0),
                  tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(10),
                  ),
                  alignment: Alignment.centerLeft,
                ),
                child: Row(
                  children: [
                    Container(
                      width: 22,
                      height: 22,
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                        color: c.ground,
                        borderRadius: BorderRadius.circular(4),
                        border: Border.all(color: c.borderSubtle),
                      ),
                      child: Text(
                        String.fromCharCode(65 + i),
                        style: TextStyle(
                          fontSize: 11,
                          fontWeight: FontWeight.w600,
                          color: c.muted,
                        ),
                      ),
                    ),
                    const SizedBox(width: 10),
                    Flexible(
                      child: Text(
                        action.options[i],
                        style: TextStyle(fontSize: 14, color: c.text),
                      ),
                    ),
                  ],
                ),
              ),
              if (i < action.options.length - 1) const SizedBox(height: 4),
            ],
            const SizedBox(height: 8),
            OutlinedButton(
              onPressed: _submitting ? null : _cancel,
              style: OutlinedButton.styleFrom(
                padding: const EdgeInsets.symmetric(
                  horizontal: 14,
                  vertical: 6,
                ),
                minimumSize: Size.zero,
                tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(20),
                ),
              ),
              child: const Text('Cancel task', style: TextStyle(fontSize: 13)),
            ),
          ],
        );

      case 'sign_in_engine':
        cardContent = Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Sign in to ${action.engine ?? 'engine'}',
              style: TextStyle(
                fontSize: 13,
                fontWeight: FontWeight.w600,
                color: c.text,
              ),
            ),
            const SizedBox(height: 4),
            Text(
              'Sign in to ${action.engine ?? 'engine'} in Settings, then tap Done.',
              style: TextStyle(fontSize: 12, color: c.muted),
            ),
            const SizedBox(height: 10),
            Row(
              children: [
                FilledButton(
                  onPressed: _submitting ? null : () => _submit('done'),
                  style: FilledButton.styleFrom(
                    backgroundColor: c.warning,
                    foregroundColor: c.ground,
                    padding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 6,
                    ),
                    minimumSize: Size.zero,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(20),
                    ),
                  ),
                  child: const Text('Done'),
                ),
                const SizedBox(width: 8),
                OutlinedButton(
                  onPressed: _submitting ? null : _cancel,
                  style: OutlinedButton.styleFrom(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 6,
                    ),
                    minimumSize: Size.zero,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(20),
                    ),
                  ),
                  child: const Text('Cancel', style: TextStyle(fontSize: 13)),
                ),
              ],
            ),
          ],
        );

      case 'add_trigger':
        // The agent wants to add an automation; user can Add or skip.
        final triggerKind = action.triggerKind;
        final triggerPrompt = action.triggerPrompt;
        final resolved = action.reason;
        cardContent = Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(Icons.schedule_outlined, size: 14, color: c.warning),
                const SizedBox(width: 6),
                Flexible(
                  child: Text(
                    '${widget.task.id.isNotEmpty ? '' : ''}'
                    '${(widget.task.action?.name ?? widget.botId)} wants to add an automation',
                    style: TextStyle(
                      fontSize: 13,
                      fontWeight: FontWeight.w600,
                      color: c.warning,
                    ),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8),
            if (triggerKind != null) ...[
              Text(
                _describeTriggerKind(triggerKind),
                style: TextStyle(fontSize: 13, color: c.text),
              ),
              const SizedBox(height: 2),
            ],
            if (triggerPrompt != null && triggerPrompt.isNotEmpty) ...[
              Text(
                triggerPrompt,
                style: TextStyle(fontSize: 12, color: c.muted),
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
              ),
              const SizedBox(height: 2),
            ],
            if (resolved != null) ...[
              Text(resolved, style: TextStyle(fontSize: 12, color: c.muted)),
              const SizedBox(height: 6),
            ] else
              const SizedBox(height: 6),
            Row(
              children: [
                FilledButton(
                  onPressed: _submitting ? null : () => _submit('add'),
                  style: FilledButton.styleFrom(
                    backgroundColor: c.warning,
                    foregroundColor: c.ground,
                    padding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 6,
                    ),
                    minimumSize: Size.zero,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(20),
                    ),
                  ),
                  child: const Text('Add'),
                ),
                const SizedBox(width: 8),
                OutlinedButton(
                  onPressed: _submitting ? null : () => _submit('skip'),
                  style: OutlinedButton.styleFrom(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 6,
                    ),
                    minimumSize: Size.zero,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(20),
                    ),
                  ),
                  child: const Text('Not now', style: TextStyle(fontSize: 13)),
                ),
              ],
            ),
          ],
        );
      default:
        // Unknown kind: plain text input
        cardContent = Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              action.label ?? 'Response needed',
              style: TextStyle(
                fontSize: 13,
                fontWeight: FontWeight.w600,
                color: c.text,
              ),
            ),
            const SizedBox(height: 8),
            TextField(
              controller: _controller,
              style: TextStyle(fontSize: 14, color: c.text),
              decoration: InputDecoration(
                hintStyle: TextStyle(color: c.faint, fontSize: 14),
                contentPadding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 8,
                ),
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(8),
                  borderSide: BorderSide(color: c.borderSubtle),
                ),
                enabledBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(8),
                  borderSide: BorderSide(color: c.borderSubtle),
                ),
              ),
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                FilledButton(
                  onPressed: _submitting
                      ? null
                      : () {
                          final v = _controller.text.trim();
                          if (v.isNotEmpty) _submit(v);
                        },
                  style: FilledButton.styleFrom(
                    backgroundColor: c.warning,
                    foregroundColor: c.ground,
                    padding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 6,
                    ),
                    minimumSize: Size.zero,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(20),
                    ),
                  ),
                  child: const Text('Submit'),
                ),
                const SizedBox(width: 8),
                OutlinedButton(
                  onPressed: _submitting ? null : _cancel,
                  style: OutlinedButton.styleFrom(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 6,
                    ),
                    minimumSize: Size.zero,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(20),
                    ),
                  ),
                  child: const Text('Cancel', style: TextStyle(fontSize: 13)),
                ),
              ],
            ),
          ],
        );
    }

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          const SizedBox(width: 36), // avatar placeholder alignment
          Flexible(
            child: Container(
              constraints: BoxConstraints(
                maxWidth: MediaQuery.of(context).size.width * 0.80,
              ),
              padding: const EdgeInsets.all(12),
              decoration: cardDecoration,
              child: cardContent,
            ),
          ),
        ],
      ),
    );
  }
}

// ── helpers ───────────────────────────────────────────────────────────────────

String _describeTriggerKind(TriggerKind kind) => switch (kind) {
  ScheduleTrigger(:final cron) => 'Schedule: $cron',
  IntervalTrigger(:final everySecs) => 'Every ${_formatInterval(everySecs)}',
  WebhookTrigger() => 'Webhook',
  GithubTrigger(:final repo, :final events) =>
    'GitHub: $repo (${events.join(', ')})',
  FileTrigger(:final path, :final glob) =>
    'File: $path${glob != null ? '/$glob' : ''}',
  BotTrigger(:final fromBot, :final on) => 'After $fromBot ($on)',
  ServiceTrigger(:final service, :final on) => 'Service $service ($on)',
};

String _formatInterval(int secs) {
  if (secs < 60) return '${secs}s';
  if (secs < 3600) return '${secs ~/ 60}m';
  return '${secs ~/ 3600}h';
}

// ── add_trigger resolved note ──────────────────────────────────────────────────

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

/// A round, softly filled header button (back), like the reference's.
class _RoundButton extends StatelessWidget {
  const _RoundButton({
    super.key,
    required this.icon,
    required this.label,
    required this.onTap,
  });
  final IconData icon;
  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Semantics(
    button: true,
    label: label,
    child: Pressable(
      onTap: onTap,
      child: _Glass(
        circle: true,
        child: SizedBox.square(
          dimension: 44,
          child: Icon(icon, size: 18, color: AtlasColors.of(context).text),
        ),
      ),
    ),
  );
}

/// A floating piece over the thread: frosted, hairline edge, soft shadow.
class _Glass extends StatelessWidget {
  const _Glass({required this.child, this.circle = false, this.padding});
  final Widget child;
  final bool circle;
  final EdgeInsets? padding;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final shape = circle ? const CircleBorder() : const StadiumBorder();
    return ClipPath(
      clipper: ShapeBorderClipper(shape: shape),
      child: BackdropFilter(
        filter: ImageFilter.blur(sigmaX: 18, sigmaY: 18),
        child: Container(
          padding: padding,
          decoration: ShapeDecoration(
            color: c.overlay.withValues(alpha: .82),
            shape: circle
                ? CircleBorder(side: BorderSide(color: c.border))
                : StadiumBorder(side: BorderSide(color: c.border)),
          ),
          child: child,
        ),
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
