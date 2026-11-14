/// The chat with one crewmate: header (back, who you are talking to, stop), the thread of
/// tasks and replies, the approval banner, the working row and the composer.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/providers/push_events_provider.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat_motion.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/crew/chat/approval_bubble.dart';
import 'package:atlas_mobile/features/crew/chat/composer.dart';
import 'package:atlas_mobile/features/crew/chat/glass.dart';
import 'package:atlas_mobile/features/crew/chat/task_thread.dart';
import 'package:atlas_mobile/features/crew/chat/working_row.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/crew_text.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_avatar.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_look.dart';
import 'package:atlas_mobile/features/crew/profile/crewmate_profile_screen.dart';

class CrewmateChatScreen extends ConsumerStatefulWidget {
  const CrewmateChatScreen({
    super.key,
    required this.device,
    required this.crewmateId,
    this.scrollToTaskId,
  });

  final MacDevice device;
  final String crewmateId;

  /// When set, the screen scrolls to this task id after the first frame.
  final String? scrollToTaskId;

  @override
  ConsumerState<CrewmateChatScreen> createState() => _CrewmateChatScreenState();
}

class _CrewmateChatScreenState extends ConsumerState<CrewmateChatScreen> {
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
    final resp = await mux.call('crewmate_task', {
      'id': widget.crewmateId,
      'prompt': text,
      'source': 'mobile',
    });
    if (!mounted) return;
    setState(() {
      _sending = false;
      if (resp['ok'] == true) {
        _msg.clear();
        ref.invalidate(
          crewmateTasksProvider((widget.device, widget.crewmateId)),
        );
      }
    });
  }

  Future<void> _cancel(String taskId) async {
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return;
    HapticFeedback.mediumImpact();
    await mux.call('crewmate_cancel', {
      'id': widget.crewmateId,
      'task': taskId,
    });
    ref.invalidate(crewmateTasksProvider((widget.device, widget.crewmateId)));
  }

  void _openProfile(CrewmateEntry crewmate, String hostName) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => CrewmateProfileScreen(
          device: widget.device,
          crewmateId: widget.crewmateId,
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);

    // Task pushes are handled by crewmateTasksProvider itself (isTaskPushFor); an answered
    // approval changes the crewmate row (its pending approval), so the list reloads.
    ref.listen(devicePushEventsProvider(widget.device), (_, next) {
      next.whenData((ev) {
        if (ev['ev'] == 'agent_approval_done') {
          ref.invalidate(crewProvider(widget.device));
        }
      });
    });

    final crewData = ref.watch(crewProvider(widget.device)).value;
    final crewmate = crewData?.crewmates
        .where((b) => b.id == widget.crewmateId)
        .firstOrNull;
    final tasks =
        ref
            .watch(crewmateTasksProvider((widget.device, widget.crewmateId)))
            .value ??
        [];
    final online =
        ref.watch(connectionNotifierProvider(widget.device)).value != null;
    final pa = crewmate?.pendingApproval;
    final queued = crewmate?.queued ?? 0;

    // Derive approval task id from pane key.
    String? approvalTaskId;
    if (pa != null) {
      final parts = pa.paneKey.split(':');
      if (parts.length >= 3) approvalTaskId = parts[2];
    }

    final crewmateName = crewmate?.name ?? '';
    final hostName = crewData?.hostName ?? widget.device.name;

    final state = CrewState.of(crewmate?.state ?? 'idle', online: online);
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
                GlassRoundButton(
                  icon: Icons.arrow_back_ios_new_rounded,
                  label: 'Back',
                  onTap: () => Navigator.of(context).maybePop(),
                ),
                const Spacer(),
                // Who you are talking to: face and name in one floating pill.
                Semantics(
                  button: true,
                  label:
                      '$crewmateName, ${switch (state) {
                        CrewState.working => 'working',
                        CrewState.needsYou => 'needs you',
                        CrewState.offline => 'offline',
                        CrewState.idle => 'on deck',
                      }}',
                  child: Pressable(
                    onTap: crewmate != null
                        ? () => _openProfile(crewmate, hostName)
                        : null,
                    child: GlassPill(
                      padding: const EdgeInsets.fromLTRB(6, 5, 18, 5),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Crewmate(
                            seed: widget.crewmateId,
                            look: crewmate?.look,
                            state: state,
                            size: 34,
                          ),
                          const SizedBox(width: 10),
                          ConstrainedBox(
                            constraints: const BoxConstraints(maxWidth: 170),
                            child: Text(
                              crewmateName,
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
                      ? GlassRoundButton(
                          key: const ValueKey('stop'),
                          icon: Icons.stop_rounded,
                          label: 'Stop $crewmateName',
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
                            seed: widget.crewmateId,
                            look: crewmate?.look,
                            state: state,
                            size: 96,
                          ),
                          title: crewmateName.isEmpty
                              ? 'New crewmate'
                              : 'Say hi to $crewmateName',
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
                            return CrewmateTaskPair(
                              key: key,
                              task: task,
                              enter: enter,
                              delay: delay,
                              day: day
                                  ? crewDayLabel(task.ts, DateTime.now())
                                  : null,
                              crewmateId: widget.crewmateId,
                              device: widget.device,
                              look: crewmate?.look,
                              pendingApproval: isApprovalTask ? pa : null,
                              expanded: _expanded.contains(task.id),
                              onToggle: () => setState(() {
                                _expanded.contains(task.id)
                                    ? _expanded.remove(task.id)
                                    : _expanded.add(task.id);
                              }),
                              onApprove: isApprovalTask
                                  ? (allow) => answerApproval(
                                      ref,
                                      widget.device,
                                      pa,
                                      allow,
                                    )
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
                              child: CrewmateApprovalBubble(
                                look: crewmate?.look,
                                approval: pa,
                                onApprove: (allow) => answerApproval(
                                  ref,
                                  widget.device,
                                  pa,
                                  allow,
                                ),
                              ),
                            ),

                          AnimatedSwitcher(
                            duration: Motion.normal,
                            transitionBuilder: (child, a) => SizeTransition(
                              sizeFactor: a,
                              alignment: Alignment.bottomCenter,
                              child: FadeTransition(opacity: a, child: child),
                            ),
                            child: crewmate != null && running != null
                                ? CrewmateWorkingRow(
                                    key: const ValueKey('typing'),
                                    crewmate: crewmate,
                                    since: running.startedAt,
                                  )
                                : const SizedBox(
                                    key: ValueKey('idle'),
                                    width: double.infinity,
                                  ),
                          ),

                          // ── composer ──────────────────────────────────────────────────
                          ChatComposer(
                            controller: _msg,
                            sending: _sending,
                            online: online,
                            queued: queued,
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
