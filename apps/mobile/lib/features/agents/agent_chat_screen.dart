import 'dart:convert';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/features/agents/agents_provider.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// Agent states as the desktop labels them.
String agentStateLabel(String state) => switch (state) {
  'working' => 'working',
  'blocked' => 'needs you',
  'waiting' => 'asked you',
  'done' => 'done',
  _ => 'idle',
};

/// One agent: its conversation (read from its session file), its state (pushed live by
/// hostd), the pending permission prompt, a box to send the next prompt while it is idle
/// or done, and a button to its terminal.
class AgentChatScreen extends ConsumerStatefulWidget {
  const AgentChatScreen({
    super.key,
    required this.device,
    required this.paneKey,
    required this.initial,
    required this.title,
  });

  final MacDevice device;
  final String paneKey;

  /// Status from the project screen, shown until the live one arrives.
  final AgentStatus initial;
  final String title;

  @override
  ConsumerState<AgentChatScreen> createState() => _AgentChatScreenState();
}

class _AgentChatScreenState extends ConsumerState<AgentChatScreen> {
  final _msg = TextEditingController();
  bool _sending = false;
  String? _error;

  /// Turns from the agent's session file; reloaded whenever its state changes.
  List<({String role, String text})> _history = const [];
  bool _truncated = false;
  String? _historyKey;

  Future<void> _loadHistory() async {
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return;
    final resp = await mux.call('agent_history', {
      'pane': widget.paneKey,
      'limit': 200,
    });
    if (!mounted || resp['ok'] != true) return;
    final data = (resp['data'] as Map).cast<String, dynamic>();
    setState(() {
      _history = [
        for (final m in (data['messages'] as List? ?? const []))
          (role: (m as Map)['role'] as String, text: m['text'] as String),
      ];
      _truncated = data['truncated'] == true;
    });
  }

  /// The agent's own terminal: the live pty whose scope is this pane.
  Future<void> _openTerminal() async {
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return;
    final resp = await mux.call('pty_list');
    String? pty;
    for (final p in (resp['data']?['ptys'] as List?) ?? const []) {
      final m = p as Map;
      if (m['exited'] == null &&
          (m['scope'] as Map?)?['pane_key'] == widget.paneKey) {
        pty = m['pty'] as String?;
      }
    }
    if (!mounted) return;
    if (pty == null) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('This agent has no open terminal')),
      );
      return;
    }
    context.push('/terminal', extra: {'device': widget.device, 'pty': pty});
  }

  @override
  void dispose() {
    _msg.dispose();
    super.dispose();
  }

  Future<void> _send() async {
    final text = _msg.text.trim();
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (text.isEmpty || mux == null) return;
    setState(() {
      _sending = true;
      _error = null;
    });
    final resp = await mux.call('agent_send', {
      'pane': widget.paneKey,
      'text': text,
    });
    if (!mounted) return;
    setState(() {
      _sending = false;
      if (resp['ok'] == true) {
        _msg.clear();
      } else {
        _error = resp['error'] as String? ?? 'error';
      }
    });
  }

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

  @override
  Widget build(BuildContext context) {
    ref.listen(connectionNotifierProvider(widget.device), (prev, next) {
      if (prev?.value == null && next.value != null) _loadHistory();
    });
    final live = ref
        .watch(agentStatusMapNotifierProvider(widget.device))
        .value?[widget.paneKey];
    final status = live ?? widget.initial;
    // A new state (turn started or ended) may mean new turns in the session file.
    final key = '${status.state}:${status.updatedAt}';
    if (key != _historyKey) {
      _historyKey = key;
      WidgetsBinding.instance.addPostFrameCallback((_) => _loadHistory());
    }
    final approval = ref.watch(
      pendingApprovalNotifierProvider((widget.device, widget.paneKey)),
    );
    final canSend = canSendChat(status) && !_sending;
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;

    return Scaffold(
      appBar: AppBar(
        title: Text(widget.title),
        actions: [
          IconButton(
            icon: const Icon(Icons.terminal),
            tooltip: 'Terminal',
            onPressed: _openTerminal,
          ),
        ],
        bottom: PreferredSize(
          preferredSize: const Size.fromHeight(28),
          child: Padding(
            padding: const EdgeInsets.only(left: 16, bottom: 8),
            child: Row(
              children: [
                StatusDot(
                  color: AtlasColors.of(context).agentState(status.state),
                  pulse: status.state == 'working' || status.state == 'blocked',
                  size: 7,
                ),
                Text(
                  '${status.agent} · ${agentStateLabel(status.state)}'
                  '${status.model != null ? ' · ${status.model}' : ''}',
                  style: TextStyle(fontSize: 12, color: muted),
                ),
              ],
            ),
          ),
        ),
      ),
      body: Column(
        children: [
          Expanded(
            child: ListView(
              // Newest at the bottom, scrolled there on open.
              reverse: true,
              padding: const EdgeInsets.all(16),
              children: [
                if (status.state == 'working')
                  const Padding(
                    padding: EdgeInsets.only(top: 10),
                    child: Align(
                      alignment: Alignment.centerLeft,
                      child: _Typing(),
                    ),
                  ),
                // No session turns yet: show what the hooks reported.
                if (_history.isEmpty) ...[
                  if (status.lastMessage != null && status.state != 'working')
                    _turn('assistant', status.lastMessage!),
                  if (status.prompt != null) _turn('user', status.prompt!),
                ] else
                  for (final m in _history.reversed) _turn(m.role, m.text),
                if (_truncated)
                  Padding(
                    padding: const EdgeInsets.only(top: 10),
                    child: Center(
                      child: Text(
                        'older messages not shown',
                        style: TextStyle(fontSize: 11, color: muted),
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
                style: TextStyle(
                  color: AtlasColors.of(context).error,
                  fontSize: 12,
                ),
              ),
            ),
          Container(
            decoration: BoxDecoration(
              color: AtlasColors.of(context).base,
              border: Border(
                top: BorderSide(color: AtlasColors.of(context).borderSubtle),
              ),
            ),
            child: SafeArea(
              top: false,
              child: Padding(
                padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
                child: Row(
                  children: [
                    Expanded(
                      child: TextField(
                        controller: _msg,
                        minLines: 1,
                        maxLines: 5,
                        enabled: canSendChat(status),
                        decoration: InputDecoration(
                          hintText: canSendChat(status)
                              ? 'Message'
                              : switch (status.state) {
                                  'working' => 'Wait for the agent to finish',
                                  'blocked' => 'Answer the request above first',
                                  'waiting' => 'The agent asked you something in its terminal',
                                  _ => 'The agent cannot take a message now',
                                },
                          fillColor: AtlasColors.of(context).ground,
                          border: OutlineInputBorder(
                            borderRadius: BorderRadius.circular(22),
                            borderSide: BorderSide.none,
                          ),
                          enabledBorder: OutlineInputBorder(
                            borderRadius: BorderRadius.circular(22),
                            borderSide: BorderSide.none,
                          ),
                          disabledBorder: OutlineInputBorder(
                            borderRadius: BorderRadius.circular(22),
                            borderSide: BorderSide.none,
                          ),
                          focusedBorder: OutlineInputBorder(
                            borderRadius: BorderRadius.circular(22),
                            borderSide: BorderSide(
                              color: AtlasColors.of(context).accent,
                            ),
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(width: 8),
                    IconButton.filled(
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
          ),
        ],
      ),
    );
  }
}

Widget _turn(String role, String text) => Padding(
  padding: const EdgeInsets.only(top: 10),
  child: Align(
    alignment: role == 'user' ? Alignment.centerRight : Alignment.centerLeft,
    child: _Bubble(text: text, mine: role == 'user'),
  ),
);

class _Bubble extends StatelessWidget {
  const _Bubble({required this.text, required this.mine});
  final String text;
  final bool mine;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    const r = Radius.circular(18);
    return Container(
      constraints: BoxConstraints(
        maxWidth: MediaQuery.of(context).size.width * 0.82,
      ),
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
      decoration: BoxDecoration(
        color: mine ? c.accent.withValues(alpha: .2) : c.base,
        border: mine ? null : Border.all(color: c.borderSubtle),
        borderRadius: BorderRadius.only(
          topLeft: r,
          topRight: r,
          bottomLeft: mine ? r : const Radius.circular(4),
          bottomRight: mine ? const Radius.circular(4) : r,
        ),
      ),
      child: SelectableText(text, style: TextStyle(color: c.text, height: 1.4)),
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
