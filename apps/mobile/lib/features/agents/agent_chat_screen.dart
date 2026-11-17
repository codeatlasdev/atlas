import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/agent_approval.dart';
import 'package:atlas_mobile/core/protocol/agent_history.dart';
import 'package:atlas_mobile/core/protocol/agent_tree.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/protocol/timeline_blocks.dart';
import 'package:atlas_mobile/core/protocol/timeline_items.dart';
import 'package:atlas_mobile/core/ui/chat_reader.dart';
import 'package:atlas_mobile/core/ui/route_page.dart';
import 'package:atlas_mobile/features/agents/agent_hub_screen.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';
import 'package:atlas_mobile/features/agents/agent_switcher.dart';
import 'package:atlas_mobile/features/agents/agents_provider.dart';
import 'package:atlas_mobile/features/agents/chat/ask_dialog.dart';
import 'package:atlas_mobile/features/agents/chat/chat_app_bar.dart';
import 'package:atlas_mobile/features/agents/chat/chat_buzzer.dart';
import 'package:atlas_mobile/features/agents/chat/chat_composer.dart';
import 'package:atlas_mobile/features/agents/chat/chat_strips.dart';
import 'package:atlas_mobile/features/agents/chat/command_sheet.dart';
import 'package:atlas_mobile/features/agents/chat/open_agent_terminal.dart';
import 'package:atlas_mobile/features/agents/chat/pending_zone.dart';
import 'package:atlas_mobile/features/agents/chat/same_json.dart';
import 'package:atlas_mobile/features/agents/chat/thread_blocks.dart';
import 'package:atlas_mobile/features/agents/chat/thread_list.dart';

/// How often the timeline is re-read while the agent can still add to it.
const historyPoll = Duration(seconds: 2);

/// One agent: its conversation as a timeline (read from its session file, polled while it
/// works), its state (pushed live by hostd), the pending permission prompt, a box to send
/// the next prompt, a Stop button while it works, and a button to its terminal.
///
/// Above the timeline sits the crew: the agent and its nested subagents (`agent_tree`,
/// polled while anything runs). Focusing a node switches the timeline to that agent's
/// transcript (`agent_history` with `sub`); only the main agent has a composer.
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

  /// A subagent's dotted name to open focused on (its transcript, read-only).
  final String? sub;

  @override
  ConsumerState<AgentChatScreen> createState() => _AgentChatScreenState();
}

class _AgentChatScreenState extends ConsumerState<AgentChatScreen> {
  final _msg = TextEditingController();
  final _composerFocus = FocusNode();

  /// Where the reader is above the end of the thread, and the thread as they left it.
  late final _reader = ChatReader<ChatThread>(onChanged: () => setState(() {}));
  bool _sending = false;
  bool _stopping = false;
  String? _error;

  AgentHistory _history = const AgentHistory();
  bool _loaded = false;
  String? _historyKey;

  /// The last payloads as hostd sent them, to tell a poll that changed nothing.
  Object? _historyRaw;
  Object? _treeRaw;

  final _thread = ThreadBlocks();
  final _buzzer = ChatBuzzer();
  Timer? _timer;
  bool _inFlight = false;

  /// The crew, refreshed by [_loadTree]; the map screen listens to it.
  final _tree = ValueNotifier<AgentTree>(AgentTree.empty);
  Timer? _treeTimer;
  bool _treeInFlight = false;

  /// Which agent the timeline shows: `''` is the main one, else a dotted subagent id.
  late String _focus = widget.sub ?? '';

  String? get _sub => _focus.isEmpty ? null : _focus;

  /// Terminal and approvals belong to the main agent's pane; a subagent has neither.
  bool get _readOnly => _sub != null;

  @override
  void initState() {
    super.initState();
    // The crew moves the strips, the polling and the hub button; a poll that brought
    // the same crew never fires this.
    _tree.addListener(_onTree);
  }

  void _onTree() {
    if (mounted) setState(() {});
  }

  Future<void> _loadHistory() async {
    if (_inFlight) return;
    final mux = liveMux(ref, widget.device);
    if (mux == null) return;
    _inFlight = true;
    final asked = _focus;
    try {
      final resp = await mux.call('agent_history', {
        'pane': widget.paneKey,
        'limit': 200,
        'sub': ?_sub,
      });
      if (!mounted || resp['ok'] != true) return;
      // The user focused another agent while this was in flight: its reply is stale.
      if (asked != _focus) return;
      final raw = resp['data'];
      // hostd sends no revision or cursor, so "new" means "differs by value". Nothing
      // new: no parse, no rebuild.
      if (_loaded && sameJson(raw, _historyRaw)) return;
      final h = AgentHistory.fromJson((raw as Map).cast<String, dynamic>());
      _historyRaw = raw;
      setState(() {
        _history = h;
        _loaded = true;
      });
    } finally {
      _inFlight = false;
      // A focus change during the call left its own load waiting on this one.
      if (mounted && asked != _focus) _loadHistory();
    }
  }

  Future<void> _loadTree() async {
    if (_treeInFlight) return;
    final mux = liveMux(ref, widget.device);
    if (mux == null) return;
    _treeInFlight = true;
    try {
      final resp = await mux.call('agent_tree', {'pane': widget.paneKey});
      // An older hostd has no `agent_tree`: the crew stays empty, the chat works.
      if (!mounted || resp['ok'] != true) return;
      final raw = resp['data'];
      if (sameJson(raw, _treeRaw)) return;
      final t = AgentTree.fromJson((raw as Map).cast<String, dynamic>());
      _treeRaw = raw;
      _buzzer.onTree(t);
      // Everything that shows the crew listens to this notifier.
      _tree.value = t;
    } finally {
      _treeInFlight = false;
    }
  }

  /// Moves the timeline to [id] (`''` = the main agent).
  void _focusNode(String id) {
    if (id == _focus) return;
    HapticFeedback.selectionClick();
    setState(() {
      _focus = id;
      _history = const AgentHistory();
      _historyRaw = null;
      _loaded = false;
      _reader.reset();
      _error = null;
    });
    _loadHistory();
  }

  /// Swipe on the timeline: to the next or previous sibling of the focused agent.
  void _swipe(double velocity) {
    if (velocity.abs() < 300) return;
    final sibs = _tree.value.siblings(_focus);
    final at = sibs.indexWhere((n) => n.id == _focus);
    if (sibs.length < 2 || at < 0) return;
    final to = at + (velocity < 0 ? 1 : -1);
    if (to < 0 || to >= sibs.length) return;
    _focusNode(sibs[to].id);
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

  /// The crew is read while anything runs (and once more when it stops, to show the end).
  void _syncTreePolling(bool active) {
    if (active && _treeTimer == null) {
      _treeTimer = Timer.periodic(historyPoll, (_) => _loadTree());
    } else if (!active && _treeTimer != null) {
      _treeTimer!.cancel();
      _treeTimer = null;
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    _treeTimer?.cancel();
    _tree.removeListener(_onTree);
    _tree.dispose();
    _msg.dispose();
    _composerFocus.dispose();
    _reader.dispose();
    super.dispose();
  }

  /// The newest status for this pane, kept by [build] for the handlers.
  AgentStatus? _status;

  /// What goes to the main agent for [text] typed on the focused agent: its own text on
  /// the main agent, else a line that names the subagent (hostd only reaches the main
  /// agent's terminal, and omp relays it).
  String _outgoing(String text) => _sub == null ? text : 'Tell $_sub: $text';

  Future<bool> _sendText(String text) async {
    if (text.isEmpty) return false;
    final Multiplexer mux;
    try {
      mux = await requireMuxOf(ref, widget.device);
    } on DeviceUnavailable catch (e) {
      // Not sent: the composer keeps the draft, and the screen says why.
      if (mounted) setState(() => _error = e.message);
      return false;
    }
    if (!mounted) return false;
    final steer = _status != null && canSteer(_status!);
    setState(() {
      _sending = true;
      _error = null;
    });
    final resp = await mux.call('agent_send', {
      'pane': widget.paneKey,
      'text': _outgoing(text),
      if (steer) 'steer': true,
    });
    if (!mounted) return false;
    final ok = resp['ok'] == true;
    setState(() {
      _sending = false;
      if (!ok) _error = resp['error'] as String? ?? 'error';
    });
    if (ok) {
      HapticFeedback.lightImpact();
      _loadHistory();
    }
    return ok;
  }

  Future<void> _send() async {
    if (await _sendText(_msg.text.trim())) _msg.clear();
  }

  Future<void> _stop() async {
    if (_stopping) return;
    HapticFeedback.mediumImpact();
    setState(() {
      _stopping = true;
      _error = null;
    });
    final Multiplexer mux;
    try {
      mux = await requireMuxOf(ref, widget.device);
    } on DeviceUnavailable catch (e) {
      if (mounted) {
        setState(() {
          _stopping = false;
          _error = e.message;
        });
      }
      return;
    }
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
    final pick = await showOmpCommands(context);
    if (pick == null || !mounted) return;
    final (cmd, needsText) = pick;
    if (!needsText) {
      await _sendText(cmd);
      return;
    }
    final text = await askText(context, 'Goal');
    if (text == null || text.isEmpty) return;
    await _sendText('$cmd $text');
  }

  Future<void> _approve(AgentApproval a, bool allow) async {
    final Multiplexer mux;
    try {
      mux = await requireMuxOf(ref, widget.device);
    } on DeviceUnavailable catch (e) {
      // The banner stays up: the agent is still waiting for this answer.
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(e.message)));
      }
      return;
    }
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

  void _openHub(AgentStatus status) => pushScreen<void>(
    context,
    AgentHubScreen(
      tree: _tree,
      focus: _focus,
      fallbackAgent: status.agent,
      onFocus: _focusNode,
      onMessage: status.agent == 'omp' ? _messageAbout : null,
    ),
  );

  /// Focuses [n] and puts the cursor in the composer: on the main agent that is a message
  /// to it, on a subagent one to the main agent about it.
  void _messageAbout(AgentNode n) {
    _focusNode(n.id);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _composerFocus.requestFocus();
    });
  }

  Future<void> _switch(Map<String, AgentStatus> all) async {
    final pane = await showAgentSwitcher(
      context,
      agents: all,
      current: widget.paneKey,
    );
    if (pane == null || pane == widget.paneKey || !mounted) return;
    Navigator.of(context).pushReplacement(
      screenRoute<void>(
        context,
        (_) => AgentChatScreen(
          device: widget.device,
          paneKey: pane,
          initial: all[pane]!,
          title: agentTitle(all[pane]!),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    ref.listen(connectionNotifierProvider(widget.device), (prev, next) {
      if (prev?.value == null && next.value != null) {
        _loadHistory();
        _loadTree();
      }
    });
    final all =
        ref.watch(agentStatusMapNotifierProvider(widget.device)).value ??
        const <String, AgentStatus>{};
    final status = all[widget.paneKey] ?? widget.initial;
    // A new state (turn started or ended) may mean new turns in the session file.
    _status = status;
    final key = '${status.state}:${status.updatedAt}';
    if (key != _historyKey) {
      _historyKey = key;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        _loadHistory();
        _loadTree();
      });
    }
    final tree = _tree.value;
    final focusNode = tree.node(_focus);
    // A subagent is read while it runs; the agent while it can still add steps.
    final active = _sub != null
        ? (_history.items.isEmpty ||
              agentActive(status.state) ||
              (focusNode?.running ?? false) ||
              _history.items.any((i) => i is ToolItem && i.status == 'running'))
        : agentActive(status.state);
    final treeActive = agentActive(status.state) || tree.anyRunning;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _syncPolling(active);
      _syncTreePolling(treeActive);
    });
    // Buzz when the main agent itself ends a turn or needs the user.
    _buzzer.onStatus(status);

    final approval = _readOnly
        ? null
        : ref.watch(
            pendingApprovalNotifierProvider((widget.device, widget.paneKey)),
          );
    final turnOver = !agentActive(status.state);
    final blocks = _thread.blocks(
      _history,
      recapLast: turnOver || _sub != null && !(focusNode?.running ?? false),
      lastDuration: _sub == null && status.stateStartedAt > 0 && turnOver
          ? Duration(
              milliseconds: (status.updatedAt - status.stateStartedAt).clamp(
                0,
                1 << 40,
              ),
            )
          : null,
    );
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
    // What the list draws: the thread as the reader left it while they are away from the end.
    final view = _reader.view(
      ChatThread(
        shown,
        status.state == 'working' && !_readOnly,
        _history.truncated,
      ),
      shown.length,
    );
    final onMessage = status.agent == 'omp' ? _messageAbout : null;

    return Scaffold(
      appBar: agentChatAppBar(
        context,
        status: status,
        history: _history,
        sub: _sub,
        onSwitch: _sub == null && all.length > 1 ? () => _switch(all) : null,
        onHub: tree.subs.isEmpty ? null : () => _openHub(status),
        onTerminal: _readOnly
            ? null
            : () => openAgentTerminal(
                context,
                ref,
                device: widget.device,
                paneKey: widget.paneKey,
                title: widget.title,
              ),
      ),
      body: LayoutBuilder(
        builder: (context, body) => Column(
          children: [
            ChatStrips(
              bodyHeight: body.maxHeight,
              treeNotifier: _tree,
              focus: _focus,
              status: status,
              onFocus: _focusNode,
              onMessage: onMessage,
            ),
            Expanded(
              child: ThreadList(
                reader: _reader,
                view: view,
                empty: shown.isEmpty && _loaded,
                blockAt: (i) => _thread.widgetAt(
                  view.blocks,
                  i,
                  focusNode?.parent,
                  history: _history,
                  onOpenSubagent: _focusNode,
                ),
                onRefresh: () async {
                  await Future.wait([_loadHistory(), _loadTree()]);
                },
                onSwipe: _swipe,
              ),
            ),
            PendingZone(
              approval: approval,
              error: _error,
              maxHeight: body.maxHeight * .25,
              onAnswer: _approve,
            ),
            // Rigid when there is room, scrolls when there is not (large text with the
            // keyboard up on a short phone): the field is always reachable.
            ConstrainedBox(
              constraints: BoxConstraints(maxHeight: body.maxHeight * .5),
              child: SingleChildScrollView(
                reverse: true,
                child: ChatComposer(
                  status: status,
                  sub: _sub,
                  controller: _msg,
                  focusNode: _composerFocus,
                  sending: _sending,
                  stopping: _stopping,
                  onSend: _send,
                  onStop: _stop,
                  onCommand: _command,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
