/// The chat with one crewmate: header (back, who you are talking to, stop), the thread of
/// tasks and replies, the approval banner, the working row and the composer.
library;

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/providers/push_events_provider.dart';
import 'package:atlas_mobile/core/ui/async_view.dart';
import 'package:atlas_mobile/core/ui/computer_state.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat_motion.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/protocol_banner.dart';
import 'package:atlas_mobile/features/crew/chat/approval_bubble.dart';
import 'package:atlas_mobile/features/crew/chat/attachments.dart';
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
  static const _workingRow = 40.0;

  /// The header's row: back, who you talk to, stop (below the status bar).
  static const _headerRow = 64.0;

  final _msg = TextEditingController();
  bool _sending = false;
  final Set<String> _expanded = {};
  final _scrollController = ScrollController();
  // Task id → GlobalKey for scroll-to-task from the board.
  final Map<String, GlobalKey> _taskKeys = {};

  /// The tasks the thread has shown. What the first answer holds is history: it is simply there, complete, from
  /// the first frame. A task that turns up after that (yours, or one from the desktop) arrives with a motion.
  Set<String>? _known;

  /// The tasks that arrived within the last moment: only these play the arrival, also when scrolled to later.
  final Set<String> _arriving = {};
  Timer? _arrivedTimer;

  void _noteArrivals(List<CrewmateTask> tasks) {
    final known = _known;
    if (known == null) {
      _known = {for (final t in tasks) t.id};
      return;
    }
    var fresh = false;
    for (final t in tasks) {
      if (known.add(t.id)) {
        _arriving.add(t.id);
        fresh = true;
      }
    }
    if (fresh) {
      _arrivedTimer?.cancel();
      _arrivedTimer = Timer(const Duration(seconds: 1), _arriving.clear);
    }
  }

  @override
  void initState() {
    super.initState();
    // A thread this phone already read is on screen at once, and is read again behind it.
    final thread = crewmateTasksProvider((widget.device, widget.crewmateId));
    if (ref.exists(thread)) ref.invalidate(thread);
    if (widget.scrollToTaskId != null) {
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => _scrollToTask(widget.scrollToTaskId!),
      );
    }
  }

  @override
  void dispose() {
    _arrivedTimer?.cancel();
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

  /// Sends the composer's text with its [images]. `true` when the crew server took the task (the composer then
  /// lets the images go); `false` keeps text and images for another try, with the server's reason on screen.
  Future<bool> _send(List<PendingImage> images) async {
    final text = _msg.text.trim();
    if (text.isEmpty) return false;
    setState(() => _sending = true);
    String? failure;
    try {
      final mux = await requireMuxOf(ref, widget.device);
      final attachments = await toSent(
        images,
        mux.call,
        () => '${DateTime.now().microsecondsSinceEpoch}-${images.length}-up',
        crewmateId: widget.crewmateId,
      );
      final resp = await mux.call('crewmate_task', {
        'id': widget.crewmateId,
        'prompt': text,
        'source': 'mobile',
        if (attachments.isNotEmpty) 'attachments': attachments,
      });
      if (resp['ok'] != true) {
        failure = resp['error'] as String? ?? 'the server refused the task';
      }
    } on StateError catch (e) {
      failure = e.message;
    } on DeviceUnavailable catch (e) {
      failure = e.message;
    }
    if (!mounted) return failure == null;
    setState(() {
      _sending = false;
      if (failure == null) {
        _msg.clear();
        ref.invalidate(
          crewmateTasksProvider((widget.device, widget.crewmateId)),
        );
      }
    });
    if (failure != null) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(failure)));
    }
    return failure == null;
  }

  /// Allow / Deny on a pending tool approval. Offline, the approval stays pending and this says why.
  Future<void> _approve(CrewmatePendingApproval pa, bool allow) async {
    try {
      await answerApproval(ref, widget.device, pa, allow);
    } on DeviceUnavailable catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(e.message)));
      }
    }
  }

  Future<void> _cancel(String taskId) async {
    HapticFeedback.mediumImpact();
    final Multiplexer mux;
    try {
      mux = await requireMuxOf(ref, widget.device);
    } on DeviceUnavailable catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(e.message)));
      }
      return;
    }
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
    final tasksAsync = ref.watch(
      crewmateTasksProvider((widget.device, widget.crewmateId)),
    );
    // `value` keeps the last answer while the thread reloads or fails to refresh: the history stays, under the
    // offline strip. Without one yet it is a spinner or an error, never "Say hi".
    final tasks = tasksAsync.value ?? const <CrewmateTask>[];
    final loadingTasks = tasksAsync.isLoading && !tasksAsync.hasValue;
    if (tasksAsync.hasValue) _noteArrivals(tasks);
    final tasksError = tasksAsync.hasValue ? null : tasksAsync.error;
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
        preferredSize: const Size.fromHeight(_headerRow),
        // The thread scrolls under the header: its own ground, from the very top (the status bar included).
        child: Stack(
          fit: StackFit.expand,
          children: [
            const GlassHeaderGround(),
            SafeArea(
              bottom: false,
              child: Padding(
                padding: const EdgeInsets.fromLTRB(
                  Space.md,
                  Space.sm,
                  Space.md,
                  0,
                ),
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
                          padding: const EdgeInsets.fromLTRB(
                            Space.sm,
                            Space.xs,
                            Space.lg,
                            Space.xs,
                          ),
                          child: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Crewmate(
                                seed: widget.crewmateId,
                                look: crewmate?.look,
                                state: state,
                                size: 34,
                              ),
                              const SizedBox(width: Space.md),
                              ConstrainedBox(
                                constraints: const BoxConstraints(
                                  maxWidth: 170,
                                ),
                                child: Text(
                                  crewmateName,
                                  overflow: TextOverflow.ellipsis,
                                  style: Theme.of(context)
                                      .textTheme
                                      .titleMedium!
                                      .copyWith(
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
                              dimension: 48,
                            ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
      body: Stack(
        children: [
          // ── message list ───────────────────────────────────────────────
          Positioned.fill(
            child: tasks.isEmpty && loadingTasks
                ? const Center(child: CircularProgressIndicator())
                : tasks.isEmpty && tasksError != null
                ? AsyncView<void>(
                    value: AsyncError<void>(tasksError, StackTrace.empty),
                    data: (_) => const SizedBox.shrink(),
                    errorTitle: "Couldn't read the thread",
                    onRetry: () => ref.invalidate(
                      crewmateTasksProvider((widget.device, widget.crewmateId)),
                    ),
                  )
                : tasks.isEmpty
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
                      MediaQuery.paddingOf(context).top + _headerRow + Space.sm,

                      Space.md,
                      // Room for the composer floating over the end of the thread.
                      // The working row's 40 dp is always reserved: it shows up and goes away without the
                      // thread moving under the reader.
                      Space.sm +
                          68 +
                          MediaQuery.paddingOf(context).bottom +
                          _workingRow,
                    ),
                    itemCount: tasks.length,
                    itemBuilder: (_, i) {
                      final task = tasks[i];
                      final key = _taskKeys.putIfAbsent(task.id, GlobalKey.new);
                      final isApprovalTask =
                          approvalTaskId != null &&
                          task.id == approvalTaskId &&
                          pa != null;
                      // Newest first: a day's label goes above its oldest message.
                      final day =
                          i == tasks.length - 1 ||
                          crewDayOf(task.ts) != crewDayOf(tasks[i + 1].ts);
                      return CrewmateTaskPair(
                        key: key,
                        task: task,
                        enter: _arriving.contains(task.id),
                        day: day ? crewDayLabel(task.ts, DateTime.now()) : null,
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
                            ? (allow) => _approve(pa, allow)
                            : null,
                      );
                    },
                  ),
          ),

          // The computer's state under the header, over the thread: it comes and goes without moving it.
          Positioned(
            top: MediaQuery.paddingOf(context).top + _headerRow,
            left: 0,
            right: 0,
            child: ColoredBox(
              color: c.ground,
              child: Column(
                children: [
                  ProtocolBanner(devices: [widget.device]),
                  OfflineStrip(
                    device: widget.device,
                    suffix: ' · history shown',
                  ),
                ],
              ),
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
    );
  }
}
