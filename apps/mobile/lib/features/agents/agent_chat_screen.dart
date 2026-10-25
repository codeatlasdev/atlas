import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';
import 'package:atlas_mobile/features/agents/agent_switcher.dart';
import 'package:atlas_mobile/features/agents/agents_provider.dart';
import 'package:atlas_mobile/features/agents/timeline_widgets.dart';

export 'package:atlas_mobile/features/agents/agent_info.dart'
    show agentStateLabel;

/// How often the timeline is re-read while the agent can still add to it.
const historyPoll = Duration(seconds: 2);

/// omp's slash commands the phone can send; `/goal` needs text.
const _ompCommands = <(String, String, bool)>[
  ('/compact', 'Compact the context', false),
  ('/new', 'Start a new session', false),
  ('/goal', 'Set a goal for the agent', true),
  ('/export', 'Export the session', false),
];

/// One agent: its conversation as a timeline (read from its session file, polled while it
/// works), its state (pushed live by hostd), the pending permission prompt, a box to send
/// the next prompt, a Stop button while it works, and a button to its terminal.
///
/// With [sub] it shows that subagent's transcript instead: read-only, no composer.
class AgentChatScreen extends ConsumerStatefulWidget {
  const AgentChatScreen({
    super.key,
    required this.device,
    required this.paneKey,
    required this.initial,
    required this.title,
    this.sub,
  });

  final MacDevice device;
  final String paneKey;

  /// Status from the project screen, shown until the live one arrives.
  final AgentStatus initial;
  final String title;

  /// A subagent's name: show its transcript, read-only.
  final String? sub;

  @override
  ConsumerState<AgentChatScreen> createState() => _AgentChatScreenState();
}

class _AgentChatScreenState extends ConsumerState<AgentChatScreen> {
  final _msg = TextEditingController();
  final _scroll = ScrollController();
  bool _sending = false;
  bool _stopping = false;
  String? _error;

  AgentHistory _history = const AgentHistory();
  bool _loaded = false;
  String? _historyKey;
  Timer? _timer;
  bool _inFlight = false;

  /// Blocks in the timeline when the user last sat at the bottom; more now = new activity.
  int _seenBlocks = 0;
  bool _atBottom = true;

  bool get _readOnly => widget.sub != null;

  @override
  void initState() {
    super.initState();
    _scroll.addListener(_onScroll);
  }

  void _onScroll() {
    final at = !_scroll.hasClients || _scroll.offset < 80;
    if (at != _atBottom) setState(() => _atBottom = at);
  }

  Future<void> _loadHistory() async {
    if (_inFlight) return;
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return;
    _inFlight = true;
    try {
      final resp = await mux.call('agent_history', {
        'pane': widget.paneKey,
        'limit': 200,
        'sub': ?widget.sub,
      });
      if (!mounted || resp['ok'] != true) return;
      final h = AgentHistory.fromJson(
        (resp['data'] as Map).cast<String, dynamic>(),
      );
      setState(() {
        _history = h;
        _loaded = true;
      });
    } finally {
      _inFlight = false;
    }
  }

  /// Poll while the agent (or the subagent being read) can still produce steps.
  void _syncPolling(bool active) {
    if (active && _timer == null) {
      _timer = Timer.periodic(historyPoll, (_) => _loadHistory());
    } else if (!active && _timer != null) {
      _timer!.cancel();
      _timer = null;
    }
  }

  /// The agent's own terminal: the live pty whose scope is this pane.
  Future<void> _openTerminal() async {
    void say(String text) =>
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(text)));
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) {
      return say('Not connected to your computer');
    }
    final resp = await mux.call('pty_list');
    if (!mounted) return;
    if (resp['ok'] != true) {
      return say('${resp['error'] ?? 'Could not list terminals'}');
    }
    String? pty;
    for (final p in (resp['data']?['ptys'] as List?) ?? const []) {
      final m = p as Map;
      if (m['exited'] == null &&
          (m['scope'] as Map?)?['pane_key'] == widget.paneKey) {
        pty = m['pty'] as String?;
      }
    }
    if (pty == null) {
      return say('This agent has no open terminal');
    }
    context.push(
      '/terminal',
      extra: {
        'device': widget.device,
        'pty': pty,
        'title': widget.title,
        'pane': widget.paneKey,
      },
    );
  }

  @override
  void dispose() {
    _timer?.cancel();
    _msg.dispose();
    _scroll.dispose();
    super.dispose();
  }

  Future<bool> _sendText(String text) async {
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (text.isEmpty || mux == null) return false;
    setState(() {
      _sending = true;
      _error = null;
    });
    final resp = await mux.call('agent_send', {
      'pane': widget.paneKey,
      'text': text,
    });
    if (!mounted) return false;
    final ok = resp['ok'] == true;
    setState(() {
      _sending = false;
      if (!ok) _error = resp['error'] as String? ?? 'error';
    });
    if (ok) _loadHistory();
    return ok;
  }

  Future<void> _send() async {
    if (await _sendText(_msg.text.trim())) _msg.clear();
  }

  Future<void> _stop() async {
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null || _stopping) return;
    HapticFeedback.mediumImpact();
    setState(() {
      _stopping = true;
      _error = null;
    });
    final resp = await mux.call('agent_interrupt', {'pane': widget.paneKey});
    if (!mounted) return;
    setState(() {
      _stopping = false;
      if (resp['ok'] != true) {
        _error = resp['error'] as String? ?? 'Could not stop the agent';
      }
    });
  }

  Future<void> _command() async {
    final pick = await showModalBottomSheet<(String, bool)>(
      context: context,
      showDragHandle: true,
      backgroundColor: AtlasColors.of(context).overlay,
      builder: (ctx) {
        final c = AtlasColors.of(ctx);
        return SafeArea(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(Space.lg, 0, Space.lg, Space.lg),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('Commands', style: Theme.of(ctx).textTheme.titleMedium),
                const SizedBox(height: Space.md),
                Wrap(
                  spacing: Space.sm,
                  runSpacing: Space.sm,
                  children: [
                    for (final (cmd, hint, arg) in _ompCommands)
                      ActionChip(
                        label: Text(arg ? '$cmd …' : cmd),
                        tooltip: hint,
                        backgroundColor: c.base,
                        side: BorderSide(color: c.borderSubtle),
                        labelStyle: const TextStyle(
                          fontFamily: monoFamily,
                          fontFamilyFallback: monoFallback,
                          fontSize: 13,
                        ),
                        onPressed: () => Navigator.pop(ctx, (cmd, arg)),
                      ),
                  ],
                ),
              ],
            ),
          ),
        );
      },
    );
    if (pick == null || !mounted) return;
    final (cmd, needsText) = pick;
    if (!needsText) {
      await _sendText(cmd);
      return;
    }
    final text = await _ask('Goal');
    if (text == null || text.isEmpty) return;
    await _sendText('$cmd $text');
  }

  Future<String?> _ask(String label) =>
      showDialog<String>(context: context, builder: (_) => _AskDialog(label));

  Future<void> _approve(AgentApproval a, bool allow) async {
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return;
    final resp = await mux.call('agent_approve', {
      'pane': widget.paneKey,
      'req': a.req,
      'allow': allow,
    });
    if (resp['ok'] != true && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(resp['error'] as String? ?? 'error')),
      );
    }
  }

  void _openSubagent(String name, AgentStatus status) =>
      Navigator.of(context).push(
        MaterialPageRoute<void>(
          builder: (_) => AgentChatScreen(
            device: widget.device,
            paneKey: widget.paneKey,
            initial: status,
            title: name,
            sub: name,
          ),
        ),
      );

  Future<void> _switch(Map<String, AgentStatus> all) async {
    final pane = await showAgentSwitcher(
      context,
      agents: all,
      current: widget.paneKey,
    );
    if (pane == null || pane == widget.paneKey || !mounted) return;
    Navigator.of(context).pushReplacement(
      MaterialPageRoute<void>(
        builder: (_) => AgentChatScreen(
          device: widget.device,
          paneKey: pane,
          initial: all[pane]!,
          title: agentTitle(all[pane]!),
        ),
      ),
    );
  }

  void _jumpToBottom() {
    if (!_scroll.hasClients) return;
    _scroll.animateTo(0, duration: Motion.slow, curve: Motion.ease);
  }

  String _firstPrompt() {
    for (final i in _history.items) {
      if (i is UserItem) return i.text;
    }
    return '';
  }

  PreferredSizeWidget _appBar(
    AgentStatus status,
    Map<String, AgentStatus> all,
  ) {
    final c = AtlasColors.of(context);
    final sub = widget.sub;
    final title =
        sub ??
        agentTitle(
          status,
          historyTitle: _history.title,
          firstPrompt: _firstPrompt(),
        );
    final inState = agentActive(status.state)
        ? elapsed(status.stateStartedAt)
        : '';
    final subtitle = sub != null
        ? '${agentLabel(status.agent)} · subagent'
        : [
            agentStateLabel(status.state),
            ?status.model,
            if (inState.isNotEmpty) inState,
          ].join(' · ');
    final canSwitch = sub == null && all.length > 1;
    return AppBar(
      titleSpacing: 0,
      title: InkWell(
        borderRadius: BorderRadius.circular(Space.radiusSm),
        onTap: canSwitch ? () => _switch(all) : null,
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 4, horizontal: 4),
          child: Row(
            children: [
              AgentLogo(status.agent, size: 30),
              const SizedBox(width: Space.sm + 2),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Row(
                      children: [
                        Flexible(
                          child: Text(
                            title,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(
                              fontSize: 16,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                        ),
                        if (canSwitch)
                          Icon(Icons.unfold_more, size: 16, color: c.faint),
                      ],
                    ),
                    Row(
                      children: [
                        if (sub == null)
                          StatusDot(
                            color: c.agentState(status.state),
                            pulse:
                                status.state == 'working' ||
                                status.state == 'blocked',
                            size: 6,
                          ),
                        Flexible(
                          child: Text(
                            subtitle,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(fontSize: 12, color: c.muted),
                          ),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
      actions: [
        if (!_readOnly)
          IconButton(
            icon: const Icon(Icons.terminal),
            tooltip: 'Terminal',
            onPressed: _openTerminal,
          ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    ref.listen(connectionNotifierProvider(widget.device), (prev, next) {
      if (prev?.value == null && next.value != null) _loadHistory();
    });
    final all =
        ref.watch(agentStatusMapNotifierProvider(widget.device)).value ??
        const <String, AgentStatus>{};
    final status = all[widget.paneKey] ?? widget.initial;
    // A new state (turn started or ended) may mean new turns in the session file.
    final key = '${status.state}:${status.updatedAt}';
    if (key != _historyKey) {
      _historyKey = key;
      WidgetsBinding.instance.addPostFrameCallback((_) => _loadHistory());
    }
    // A subagent is read while it runs; the agent while it can still add steps.
    final active = widget.sub != null
        ? (_history.items.isEmpty ||
              agentActive(status.state) ||
              _history.items.any((i) => i is ToolItem && i.status == 'running'))
        : agentActive(status.state);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _syncPolling(active);
    });

    final approval = _readOnly
        ? null
        : ref.watch(
            pendingApprovalNotifierProvider((widget.device, widget.paneKey)),
          );
    final c = AtlasColors.of(context);
    final blocks = groupTimeline(_history.items);
    // Without session turns yet: what the hooks reported.
    final fallback = <TimelineBlock>[
      if (blocks.isEmpty && status.prompt != null)
        ItemBlock(UserItem(status.prompt!)),
      if (blocks.isEmpty &&
          status.lastMessage != null &&
          status.state != 'working')
        ItemBlock(AssistantItem(status.lastMessage!)),
    ];
    final shown = blocks.isEmpty ? fallback : blocks;
    if (_atBottom) _seenBlocks = shown.length;
    final fresh = !_atBottom && shown.length > _seenBlocks;

    return Scaffold(
      appBar: _appBar(status, all),
      body: Column(
        children: [
          Expanded(
            child: Stack(
              children: [
                RefreshIndicator(
                  color: c.accent,
                  onRefresh: _loadHistory,
                  child: ListView(
                    controller: _scroll,
                    // Newest at the bottom, scrolled there on open.
                    reverse: true,
                    physics: const AlwaysScrollableScrollPhysics(),
                    padding: const EdgeInsets.all(Space.lg),
                    children: [
                      if (status.state == 'working' && !_readOnly)
                        const Padding(
                          padding: EdgeInsets.only(top: 10),
                          child: Align(
                            alignment: Alignment.centerLeft,
                            child: _Typing(),
                          ),
                        ),
                      for (final b in shown.reversed)
                        Padding(
                          padding: const EdgeInsets.only(top: 10),
                          child: timelineBlock(
                            context,
                            b,
                            history: _history,
                            onOpenSubagent: (n) => _openSubagent(n, status),
                          ),
                        ),
                      if (_history.truncated)
                        Padding(
                          padding: const EdgeInsets.only(top: 10),
                          child: Center(
                            child: Text(
                              'older messages not shown',
                              style: TextStyle(fontSize: 11, color: c.muted),
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
                if (shown.isEmpty && _loaded)
                  Center(
                    child: Text(
                      'Nothing here yet',
                      style: TextStyle(color: c.faint),
                    ),
                  ),
                if (fresh)
                  Positioned(
                    bottom: Space.md,
                    left: 0,
                    right: 0,
                    child: Center(
                      child: FilledButton.tonalIcon(
                        onPressed: _jumpToBottom,
                        icon: const Icon(Icons.arrow_downward, size: 16),
                        label: const Text('New activity'),
                      ),
                    ),
                  ),
              ],
            ),
          ),
          if (approval != null)
            _ApprovalBanner(
              approval: approval,
              onAnswer: (allow) => _approve(approval, allow),
            ),
          if (_error != null)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12),
              child: Text(
                _error!,
                style: TextStyle(color: c.error, fontSize: 12),
              ),
            ),
          if (!_readOnly) _composer(status),
        ],
      ),
    );
  }

  Widget _composer(AgentStatus status) {
    final c = AtlasColors.of(context);
    final canType = canSendChat(status);
    final canSend = canType && !_sending;
    final working = status.state == 'working';
    OutlineInputBorder border([Color? color]) => OutlineInputBorder(
      borderRadius: BorderRadius.circular(22),
      borderSide: color == null ? BorderSide.none : BorderSide(color: color),
    );
    return Container(
      decoration: BoxDecoration(
        color: c.base,
        border: Border(top: BorderSide(color: c.borderSubtle)),
      ),
      child: SafeArea(
        top: false,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(4, 8, 12, 8),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              if (status.agent == 'omp')
                IconButton(
                  tooltip: 'Commands',
                  onPressed: canType && !_sending ? _command : null,
                  icon: const Text(
                    '/',
                    style: TextStyle(
                      fontFamily: monoFamily,
                      fontFamilyFallback: monoFallback,
                      fontSize: 20,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                )
              else
                const SizedBox(width: 8),
              Expanded(
                child: TextField(
                  controller: _msg,
                  minLines: 1,
                  maxLines: 5,
                  enabled: canType,
                  decoration: InputDecoration(
                    hintText: canType
                        ? 'Message'
                        : switch (status.state) {
                            'working' => 'Working… tap stop to interrupt',
                            'blocked' => 'Answer the request above first',
                            'waiting' =>
                              'The agent asked you something in its terminal',
                            _ => 'The agent cannot take a message now',
                          },
                    fillColor: c.ground,
                    border: border(),
                    enabledBorder: border(),
                    disabledBorder: border(),
                    focusedBorder: border(c.accent),
                  ),
                ),
              ),
              const SizedBox(width: 8),
              if (working)
                IconButton.filled(
                  tooltip: 'Stop',
                  style: IconButton.styleFrom(
                    backgroundColor: c.error.withValues(alpha: .2),
                    foregroundColor: c.error,
                  ),
                  onPressed: _stopping ? null : _stop,
                  icon: _stopping
                      ? const SizedBox.square(
                          dimension: 18,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Icon(Icons.stop_rounded),
                )
              else
                IconButton.filled(
                  tooltip: 'Send message',
                  onPressed: canSend ? _send : null,
                  icon: _sending
                      ? const SizedBox.square(
                          dimension: 18,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Icon(Icons.arrow_upward_rounded),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class _ApprovalBanner extends StatelessWidget {
  const _ApprovalBanner({required this.approval, required this.onAnswer});
  final AgentApproval approval;
  final void Function(bool allow) onAnswer;

  @override
  Widget build(BuildContext context) {
    final input = approval.toolInput;
    final detail = input is Map && input['command'] is String
        ? input['command'] as String
        : const JsonEncoder.withIndent('  ').convert(input);
    return Container(
      width: double.infinity,
      margin: const EdgeInsets.fromLTRB(12, 0, 12, 8),
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: AtlasColors.of(context).warning.withValues(alpha: .12),
        border: Border.all(
          color: AtlasColors.of(context).warning.withValues(alpha: .6),
        ),
        borderRadius: BorderRadius.circular(Space.radius),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(
                Icons.shield_outlined,
                size: 18,
                color: AtlasColors.of(context).warning,
              ),
              const SizedBox(width: 6),
              Text(
                'Wants to run ${approval.toolName ?? 'a tool'}',
                style: const TextStyle(fontWeight: FontWeight.w600),
              ),
            ],
          ),
          const SizedBox(height: 6),
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 120),
            child: SingleChildScrollView(
              child: SelectableText(
                detail,
                style: const TextStyle(
                  fontFamily: monoFamily,
                  fontFamilyFallback: monoFallback,
                  fontSize: 12,
                ),
              ),
            ),
          ),
          const SizedBox(height: 8),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              OutlinedButton(
                onPressed: () => onAnswer(false),
                child: const Text('Deny'),
              ),
              const SizedBox(width: 8),
              FilledButton(
                onPressed: () => onAnswer(true),
                child: const Text('Allow'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// Three dots that rise in turn while the agent works.
class _Typing extends StatefulWidget {
  const _Typing();

  @override
  State<_Typing> createState() => _TypingState();
}

class _TypingState extends State<_Typing> with SingleTickerProviderStateMixin {
  late final AnimationController _a = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1100),
  )..repeat();

  @override
  void dispose() {
    _a.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
      decoration: BoxDecoration(
        color: c.base,
        border: Border.all(color: c.borderSubtle),
        borderRadius: BorderRadius.circular(18),
      ),
      child: AnimatedBuilder(
        animation: _a,
        builder: (_, _) => Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            for (var i = 0; i < 3; i++)
              Transform.translate(
                offset: Offset(
                  0,
                  -3 *
                      math
                          .sin(((_a.value - i * .18) % 1) * math.pi)
                          .clamp(0, 1),
                ),
                child: Container(
                  margin: const EdgeInsets.symmetric(horizontal: 2),
                  width: 6,
                  height: 6,
                  decoration: BoxDecoration(
                    color: c.muted,
                    shape: BoxShape.circle,
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

/// A one-field prompt; it owns its controller so the dialog's exit animation never
/// paints a disposed one.
class _AskDialog extends StatefulWidget {
  const _AskDialog(this.label);
  final String label;

  @override
  State<_AskDialog> createState() => _AskDialogState();
}

class _AskDialogState extends State<_AskDialog> {
  final _ctl = TextEditingController();

  @override
  void dispose() {
    _ctl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(widget.label),
    content: TextField(
      controller: _ctl,
      autofocus: true,
      minLines: 1,
      maxLines: 4,
      textCapitalization: TextCapitalization.sentences,
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        onPressed: () => Navigator.pop(context, _ctl.text.trim()),
        child: const Text('Send'),
      ),
    ],
  );
}
