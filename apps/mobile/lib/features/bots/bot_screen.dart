
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/providers/push_events_provider.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/bots/bots_provider.dart';
import 'package:atlas_mobile/features/bots/bot_profile_screen.dart';

// ── stable avatar colors (same palette as bots_screen) ───────────────────────

const _kPalette = [
  Color(0xFF26A69A),
  Color(0xFFFF7043),
  Color(0xFF7E57C2),
  Color(0xFF42A5F5),
  Color(0xFF66BB6A),
  Color(0xFFEC407A),
  Color(0xFFFFCA28),
  Color(0xFF26C6DA),
];

Color _avatarColor(String id) {
  final h = id.codeUnits.fold(0, (a, u) => a ^ u);
  return _kPalette[h.abs() % _kPalette.length];
}

// ── helpers ───────────────────────────────────────────────────────────────────

String _elapsed(int startedAtMs) {
  final d = DateTime.now()
      .difference(DateTime.fromMillisecondsSinceEpoch(startedAtMs));
  if (d.inSeconds < 60) return '${d.inSeconds}s';
  return '${d.inMinutes}m';
}

String _chatTime(int tsMs) {
  final dt = DateTime.fromMillisecondsSinceEpoch(tsMs);
  final hh = dt.hour.toString().padLeft(2, '0');
  final mm = dt.minute.toString().padLeft(2, '0');
  return '$hh:$mm';
}

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
  });

  final MacDevice device;
  final String botId;

  @override
  ConsumerState<BotScreen> createState() => _BotScreenState();
}

class _BotScreenState extends ConsumerState<BotScreen> {
  final _msg = TextEditingController();
  bool _sending = false;
  final Set<String> _expanded = {};

  @override
  void dispose() {
    _msg.dispose();
    super.dispose();
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
    Navigator.of(context).push(MaterialPageRoute<void>(
      builder: (_) => BotProfileScreen(device: widget.device, botId: widget.botId),
    ));
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
    final tasks = ref.watch(botTasksProvider((widget.device, widget.botId))).value ?? [];
    final online = ref.watch(connectionNotifierProvider(widget.device)).value != null;
    final pa = bot?.pendingApproval;
    final queued = bot?.queued ?? 0;

    // Derive approval task id from pane key.
    String? approvalTaskId;
    if (pa != null) {
      final parts = pa.paneKey.split(':');
      if (parts.length >= 3) approvalTaskId = parts[2];
    }

    final avatarColor = _avatarColor(widget.botId);
    final botName = bot?.name ?? '';
    final initial = botName.isNotEmpty ? botName[0].toUpperCase() : '?';
    final hostName = botsData?.hostName ?? widget.device.name;
    final agentLabel = bot?.agent ?? '';

    return Scaffold(
      backgroundColor: c.ground,
      appBar: AppBar(
        backgroundColor: c.base,
        titleSpacing: 0,
        title: InkWell(
          borderRadius: BorderRadius.circular(8),
          onTap: bot != null ? () => _openProfile(bot, hostName) : null,
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 6, horizontal: 4),
            child: Row(
              children: [
                // Avatar
                CircleAvatar(
                  radius: 18,
                  backgroundColor: avatarColor.withValues(alpha: .22),
                  child: Text(
                    initial,
                    style: TextStyle(
                      color: avatarColor,
                      fontSize: 14,
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                ),
                const SizedBox(width: 10),
                Flexible(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        botName,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 16,
                          fontWeight: FontWeight.w600,
                          color: c.text,
                        ),
                      ),
                      if (agentLabel.isNotEmpty)
                        Text(
                          '$agentLabel · $hostName',
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(fontSize: 12, color: c.muted),
                        ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
        actions: [
          if (bot != null)
            StatusDot(
              color: switch (bot.state) {
                'working' => c.accent,
                'needs_you' => c.warning,
                _ => c.faint,
              },
              pulse: bot.state == 'working' || bot.state == 'needs_you',
              size: 7,
            ),
          const SizedBox(width: 16),
        ],
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
                  Flexible(child: Text(
                    '\${widget.device.name} is offline · history shown',
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(fontSize: 12, color: c.warning),
                  )),
                ],
              ),
            ),

          // ── message list ───────────────────────────────────────────────
          Expanded(
            child: tasks.isEmpty
                ? Center(
                    child: Text(
                      'Send a message to start',
                      style: TextStyle(color: c.faint),
                    ),
                  )
                : ListView.builder(
                    reverse: true,
                    padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
                    itemCount: tasks.length,
                    itemBuilder: (_, i) {
                      final task = tasks[i];
                      final isApprovalTask = approvalTaskId != null &&
                          task.id == approvalTaskId &&
                          pa != null;
                      return _TaskPair(
                        task: task,
                        botId: widget.botId,
                        botName: botName,
                        avatarColor: avatarColor,
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

          // ── global approval banner (if not matched to a task) ──────────
          if (pa != null && approvalTaskId == null)
            _ApprovalBubble(
              botName: botName,
              avatarColor: avatarColor,
              initial: initial,
              approval: pa,
              onApprove: (allow) => _approve(pa, allow),
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
    );
  }
}

// ── Task pair: user prompt + bot reply ────────────────────────────────────────

class _TaskPair extends StatelessWidget {
  const _TaskPair({
    required this.task,
    required this.botId,
    required this.botName,
    required this.avatarColor,
    required this.initial,
    required this.expanded,
    required this.onToggle,
    this.pendingApproval,
    this.onApprove,
  });

  final BotTask task;
  final String botId;
  final String botName;
  final Color avatarColor;
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
        // ── timestamp ──────────────────────────────────────────────────
        _Timestamp(tsMs: task.ts),

        // ── user bubble (right) ────────────────────────────────────────
        _UserBubble(task: task),
        const SizedBox(height: 4),

        // ── bot reply (left) ──────────────────────────────────────────
        _BotBubble(
          task: task,
          avatarColor: avatarColor,
          initial: initial,
          expanded: expanded,
          onToggle: onToggle,
        ),

        // ── approval bubble (inline with task) ─────────────────────────
        if (pendingApproval != null) ...[
          const SizedBox(height: 4),
          _ApprovalBubble(
            botName: botName,
            avatarColor: avatarColor,
            initial: initial,
            approval: pendingApproval!,
            onApprove: onApprove ?? (_) {},
          ),
        ],
        const SizedBox(height: 16),
      ],
    );
  }
}

// ── Centered timestamp separator ──────────────────────────────────────────────

class _Timestamp extends StatelessWidget {
  const _Timestamp({required this.tsMs});
  final int tsMs;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Center(
        child: Text(
          _chatTime(tsMs),
          style: TextStyle(fontSize: 11, color: c.faint),
        ),
      ),
    );
  }
}

// ── User bubble (right side) ──────────────────────────────────────────────────

class _UserBubble extends StatelessWidget {
  const _UserBubble({required this.task});
  final BotTask task;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    const r = Radius.circular(18);
    return Align(
      alignment: Alignment.centerRight,
      child: Container(
        constraints: BoxConstraints(maxWidth: MediaQuery.of(context).size.width * 0.75),
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
        decoration: BoxDecoration(
          color: c.accent.withValues(alpha: .22),
          borderRadius: const BorderRadius.only(
            topLeft: r, topRight: r, bottomLeft: r, bottomRight: Radius.circular(4),
          ),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            Text(
              task.prompt,
              style: TextStyle(color: c.text, height: 1.4, fontSize: 14),
            ),
            const SizedBox(height: 3),
            Text(
              _sourceLabel(task.source),
              style: TextStyle(fontSize: 10, color: c.muted),
            ),
          ],
        ),
      ),
    );
  }
}

// ── Bot bubble (left side) ────────────────────────────────────────────────────

class _BotBubble extends StatelessWidget {
  const _BotBubble({
    required this.task,
    required this.avatarColor,
    required this.initial,
    required this.expanded,
    required this.onToggle,
  });

  final BotTask task;
  final Color avatarColor;
  final String initial;
  final bool expanded;
  final VoidCallback onToggle;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);

    Widget content;
    switch (task.state) {
      case 'queued':
        content = Text('Queued…', style: TextStyle(color: c.faint, fontSize: 14));

      case 'running':
        content = GestureDetector(
          onTap: task.steps.isNotEmpty ? onToggle : null,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  SizedBox.square(
                    dimension: 12,
                    child: CircularProgressIndicator(strokeWidth: 2, color: c.accent),
                  ),
                  const SizedBox(width: 8),
                  Text(
                    task.startedAt != null
                        ? 'Working · ${_elapsed(task.startedAt!)}'
                        : 'Working…',
                    style: TextStyle(color: c.accent, fontSize: 14),
                  ),
                  if (task.steps.isNotEmpty) ...[
                    const SizedBox(width: 4),
                    Icon(
                      expanded ? Icons.keyboard_arrow_up : Icons.keyboard_arrow_down,
                      size: 16,
                      color: c.faint,
                    ),
                  ],
                ],
              ),
              if (expanded && task.steps.isNotEmpty) ...[
                const SizedBox(height: 8),
                for (final step in task.steps)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 3),
                    child: Text(
                      '· ${step.text}',
                      style: TextStyle(
                        fontSize: 12,
                        color: c.muted,
                        fontFamily: 'Menlo',
                        fontFamilyFallback: const ['SF Mono', 'RobotoMono', 'monospace'],
                      ),
                    ),
                  ),
              ],
            ],
          ),
        );

      case 'failed':
        content = Text(
          task.error ?? 'Task failed',
          style: TextStyle(color: c.error, fontSize: 14),
        );

      case 'cancelled':
        content = Text('Cancelled', style: TextStyle(color: c.faint, fontSize: 14));

      default: // done
        content = Text(
          task.result ?? '',
          style: TextStyle(color: c.text, fontSize: 14, height: 1.4),
        );
    }

    const r = Radius.circular(18);
    return Row(
      crossAxisAlignment: CrossAxisAlignment.end,
      children: [
        // Small avatar
        CircleAvatar(
          radius: 14,
          backgroundColor: avatarColor.withValues(alpha: .22),
          child: Text(initial,
              style: TextStyle(color: avatarColor, fontSize: 11, fontWeight: FontWeight.w700)),
        ),
        const SizedBox(width: 8),
        Flexible(
          child: Container(
            constraints: BoxConstraints(
              maxWidth: MediaQuery.of(context).size.width * 0.72,
            ),
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
            decoration: BoxDecoration(
              color: c.base,
              border: Border.all(color: c.borderSubtle),
              borderRadius: const BorderRadius.only(
                topLeft: Radius.circular(4), topRight: r, bottomLeft: r, bottomRight: r,
              ),
            ),
            child: content,
          ),
        ),
      ],
    );
  }
}

// ── Approval bubble (bot-side, with Deny/Allow) ───────────────────────────────

class _ApprovalBubble extends StatelessWidget {
  const _ApprovalBubble({
    required this.botName,
    required this.avatarColor,
    required this.initial,
    required this.approval,
    required this.onApprove,
  });

  final String botName;
  final Color avatarColor;
  final String initial;
  final BotPendingApproval approval;
  final void Function(bool) onApprove;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final input = approval.toolInput;
    final cmd = input is Map && input['command'] is String ? input['command'] as String : null;

    const r = Radius.circular(18);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          CircleAvatar(
            radius: 14,
            backgroundColor: avatarColor.withValues(alpha: .22),
            child: Text(initial,
                style: TextStyle(color: avatarColor, fontSize: 11, fontWeight: FontWeight.w700)),
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
                  topLeft: Radius.circular(4), topRight: r, bottomLeft: r, bottomRight: r,
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
                          style: TextStyle(color: c.warning, fontSize: 13, fontWeight: FontWeight.w600),
                        ),
                      ),
                    ],
                  ),
                  if (cmd != null) ...[
                    const SizedBox(height: 6),
                    Container(
                      width: double.infinity,
                      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
                      decoration: BoxDecoration(
                        color: c.ground,
                        borderRadius: BorderRadius.circular(8),
                      ),
                      child: Text(
                        cmd,
                        style: TextStyle(
                          fontFamily: 'Menlo',
                          fontFamilyFallback: const ['SF Mono', 'RobotoMono', 'monospace'],
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
                          side: BorderSide(color: c.error.withValues(alpha: .5)),
                          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 5),
                          minimumSize: Size.zero,
                          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
                        ),
                        child: const Text('Deny'),
                      ),
                      const SizedBox(width: 6),
                      FilledButton(
                        onPressed: () => onApprove(true),
                        style: FilledButton.styleFrom(
                          backgroundColor: c.warning,
                          foregroundColor: c.ground,
                          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 5),
                          minimumSize: Size.zero,
                          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
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

// ── iMessage-style composer ───────────────────────────────────────────────────

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
    return Container(
      color: c.base,
      child: SafeArea(
        top: false,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // Pill with send icon inside
              Container(
                decoration: BoxDecoration(
                  color: c.ground,
                  borderRadius: BorderRadius.circular(24),
                  border: Border.all(color: c.borderSubtle),
                ),
                child: Row(
                  children: [
                    Expanded(
                      child: TextField(
                        controller: controller,
                        minLines: 1,
                        maxLines: 5,
                        style: TextStyle(fontSize: 14, color: c.text),
                        decoration: InputDecoration(
                          hintText: online
                              ? 'Message $hintName'
                              : 'Offline — queued when back',
                          hintStyle: TextStyle(color: c.faint, fontSize: 14),
                          border: InputBorder.none,
                          contentPadding: const EdgeInsets.symmetric(
                            horizontal: 16, vertical: 10,
                          ),
                        ),
                      ),
                    ),
                    Padding(
                      padding: const EdgeInsets.only(right: 6),
                      child: GestureDetector(
                        onTap: sending ? null : onSend,
                        child: AnimatedContainer(
                          duration: const Duration(milliseconds: 150),
                          width: 32,
                          height: 32,
                          decoration: BoxDecoration(
                            color: c.accent.withValues(alpha: sending ? .4 : 1),
                            shape: BoxShape.circle,
                          ),
                          child: sending
                              ? Padding(
                                  padding: const EdgeInsets.all(7),
                                  child: CircularProgressIndicator(
                                    strokeWidth: 2,
                                    color: c.onAccent,
                                  ),
                                )
                              : Icon(
                                  Icons.arrow_upward_rounded,
                                  size: 18,
                                  color: c.onAccent,
                                ),
                        ),
                      ),
                    ),
                  ],
                ),
              ),
              if (queued > 0) ...[
                const SizedBox(height: 4),
                Padding(
                  padding: const EdgeInsets.only(left: 16),
                  child: Text(
                    queued == 1 ? '1 task queued' : '$queued tasks queued',
                    style: TextStyle(fontSize: 11, color: c.faint),
                  ),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
