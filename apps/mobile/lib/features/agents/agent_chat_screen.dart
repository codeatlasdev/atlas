import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';
import 'package:atlas_mobile/features/agents/agent_switcher.dart';
import 'package:atlas_mobile/features/agents/agents_provider.dart';
import 'package:atlas_mobile/features/agents/agent_hub_screen.dart';
import 'package:atlas_mobile/features/agents/crew_widgets.dart';
import 'package:atlas_mobile/features/agents/timeline_widgets.dart';
import 'package:atlas_mobile/features/agents/approval_banner.dart';
import 'package:atlas_mobile/features/agents/chat/ask_dialog.dart';
import 'package:atlas_mobile/features/agents/chat/capped_strips.dart';
import 'package:atlas_mobile/features/agents/chat/chat_composer.dart';
import 'package:atlas_mobile/features/agents/chat/command_sheet.dart';
import 'package:atlas_mobile/features/agents/chat/same_json.dart';
import 'package:atlas_mobile/features/agents/chat/typing_bubble.dart';

export 'package:atlas_mobile/features/agents/agent_info.dart'
    show agentStateLabel;

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

/// The thread as the list draws it: its blocks, whether the typing bubble is under them and whether older
/// messages were cut.
class _Thread {
  const _Thread(this.blocks, this.typing, this.truncated);
  final List<TimelineBlock> blocks;
  final bool typing;
  final bool truncated;
}

class _AgentChatScreenState extends ConsumerState<AgentChatScreen> {
  final _msg = TextEditingController();
  final _composerFocus = FocusNode();
  final _scroll = ScrollController();
  bool _sending = false;
  bool _stopping = false;
  String? _error;

  AgentHistory _history = const AgentHistory();
  bool _loaded = false;
  String? _historyKey;

  /// The last payloads as hostd sent them, to tell a poll that changed nothing.
  Object? _historyRaw;
  Object? _treeRaw;

  /// `groupTimeline` of [_history], and the widget built for each block, kept while
  /// their inputs stay the same: a rebuild of the screen (status push, scroll flag,
  /// composer) reuses them, so Flutter skips the unchanged blocks instead of laying
  /// out and re-parsing every message again.
  List<TimelineBlock>? _grouped;
  AgentHistory? _groupedOf;
  (bool, Duration?)? _groupedWith;
  final _built = <TimelineBlock, Widget>{};
  String? _builtParent;
  Timer? _timer;
  bool _inFlight = false;

  /// The crew, refreshed by [_loadTree]; the map screen listens to it.
  final _tree = ValueNotifier<AgentTree>(AgentTree.empty);
  Timer? _treeTimer;
  bool _treeInFlight = false;

  /// Which agent the timeline shows: `''` is the main one, else a dotted subagent id.
  late String _focus = widget.sub ?? '';

  /// Last seen status per node, to buzz when one finishes, fails or needs the user.
  final _seen = <String, String>{};
  String? _seenState;

  /// Blocks in the timeline when the user last sat at the bottom; more now = new activity.
  int _seenBlocks = 0;
  bool _atBottom = true;

  /// What the thread showed when the reader scrolled away from the end, and what the last build would show now.
  /// A reversed list keeps its pixel offset, so anything added or changed near the end would push the part the
  /// reader is on by its height: while they read above the end the thread stays as it was, and the "New
  /// activity" button says there is more. Back at the end they get the live thread.
  _Thread? _frozen;
  _Thread _live = const _Thread([], false, false);

  String? get _sub => _focus.isEmpty ? null : _focus;

  /// Terminal and approvals belong to the main agent's pane; a subagent has neither.
  bool get _readOnly => _sub != null;

  @override
  void initState() {
    super.initState();
    _scroll.addListener(_onScroll);
    // The crew moves the strips, the polling and the hub button; a poll that brought
    // the same crew never fires this.
    _tree.addListener(_onTree);
  }

  void _onTree() {
    if (mounted) setState(() {});
  }

  /// [groupTimeline] of the current history, recomputed only when its inputs change.
  List<TimelineBlock> _blocks({
    required bool recapLast,
    required Duration? lastDuration,
  }) {
    final with_ = (recapLast, lastDuration);
    if (!identical(_groupedOf, _history) || _groupedWith != with_) {
      _grouped = groupTimeline(
        _history.items,
        recapLast: recapLast,
        lastDuration: lastDuration,
      );
      _groupedOf = _history;
      _groupedWith = with_;
      _built.clear();
    }
    return _grouped!;
  }

  /// The padded widget of block [i] of [shown], built once per block. Blocks are the
  /// keys: identical blocks of a new grouping are new objects, so [_blocks] clears this.
  Widget _blockAt(List<TimelineBlock> shown, int i, String? parentId) {
    if (_builtParent != parentId) {
      _built.clear();
      _builtParent = parentId;
    }
    final b = shown[i];
    return _built[b] ??= Padding(
      padding: const EdgeInsets.only(top: Space.md),
      child: Builder(
        builder: (context) => timelineBlock(
          context,
          b,
          history: _history,
          onOpenSubagent: _focusNode,
          parentId: parentId,
        ),
      ),
    );
  }

  void _onScroll() {
    final at = !_scroll.hasClients || _scroll.offset < 80;
    if (at != _atBottom) {
      setState(() {
        _atBottom = at;
        _frozen = at ? null : _live;
      });
    }
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
      _buzzOnTransitions(t);
      // Everything that shows the crew listens to this notifier.
      _tree.value = t;
    } finally {
      _treeInFlight = false;
    }
  }

  /// A tick when a node ends well, a heavier one when it fails; the first read only
  /// records, so opening the screen never buzzes for old news.
  void _buzzOnTransitions(AgentTree t) {
    final first = _seen.isEmpty;
    var failed = false, done = false;
    for (final n in t.nodes) {
      final before = _seen[n.id];
      if (!first && before != null && before != n.status) {
        if (n.failed) failed = true;
        if (n.done && !n.isRoot) done = true;
      }
      _seen[n.id] = n.status;
    }
    if (failed) {
      HapticFeedback.heavyImpact();
    } else if (done) {
      HapticFeedback.lightImpact();
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
      _seenBlocks = 0;
      _atBottom = true;
      _frozen = null;
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

  /// The agent's own terminal: the live pty whose scope is this pane.
  Future<void> _openTerminal() async {
    void say(String text) =>
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(text)));
    final Multiplexer mux;
    try {
      mux = await requireMuxOf(ref, widget.device);
    } on DeviceUnavailable catch (e) {
      return say(e.message);
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
    _treeTimer?.cancel();
    _tree.removeListener(_onTree);
    _tree.dispose();
    _msg.dispose();
    _composerFocus.dispose();
    _scroll.dispose();
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

  void _openHub(AgentStatus status) => Navigator.of(context).push(
    MaterialPageRoute<void>(
      builder: (_) => AgentHubScreen(
        tree: _tree,
        focus: _focus,
        fallbackAgent: status.agent,
        onFocus: _focusNode,
        onMessage: status.agent == 'omp' ? _messageAbout : null,
      ),
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

  /// Main › … › the focused agent. When the tree does not list it (yet, or at all on an
  /// older hostd) the path is built from its dotted id, so Main is always one tap away.
  List<AgentNode> _crumbPath(AgentTree tree) {
    final listed = tree.path(_focus);
    if (listed.isNotEmpty && listed.first.isRoot) return listed;
    final segs = _focus.split('.');
    return [
      tree.node('') ?? const AgentNode(id: '', status: 'idle'),
      for (var i = 0; i < segs.length; i++)
        tree.node(segs.sublist(0, i + 1).join('.')) ??
            AgentNode(
              id: segs.sublist(0, i + 1).join('.'),
              parent: i == 0 ? '' : segs.sublist(0, i).join('.'),
              name: segs[i],
              status: 'pending',
            ),
    ];
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
    final sub = _sub;
    final title =
        sub?.split('.').last ??
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
      // Two lines of text (title, state): the bar grows with the text size instead of
      // clipping the second one.
      toolbarHeight: math.max(
        kToolbarHeight,
        MediaQuery.textScalerOf(context).scale(16 * 1.4 + 12 * 1.4) + 16,
      ),
      title: InkWell(
        borderRadius: BorderRadius.circular(Space.radiusSm),
        onTap: canSwitch ? () => _switch(all) : null,
        child: Padding(
          padding: const EdgeInsets.symmetric(
            vertical: Space.xs,
            horizontal: Space.xs,
          ),
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
                            style: Theme.of(context).textTheme.titleMedium!
                                .copyWith(fontWeight: FontWeight.w600),
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
                            style: Theme.of(context).textTheme.bodySmall!
                                .copyWith(color: c.muted),
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
        IconButton(
          icon: const Icon(Icons.hub_outlined),
          tooltip: 'Agent hub',
          onPressed: _tree.value.subs.isEmpty ? null : () => _openHub(status),
        ),
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
    if (_seenState != null && _seenState != status.state) {
      if (status.state == 'done') {
        HapticFeedback.lightImpact();
      } else if (needsYou(status)) {
        HapticFeedback.heavyImpact();
      }
    }
    _seenState = status.state;

    final approval = _readOnly
        ? null
        : ref.watch(
            pendingApprovalNotifierProvider((widget.device, widget.paneKey)),
          );
    final c = AtlasColors.of(context);
    final turnOver = !agentActive(status.state);
    final blocks = _blocks(
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
    if (_atBottom) _seenBlocks = shown.length;
    final fresh = !_atBottom && shown.length > _seenBlocks;
    _live = _Thread(
      shown,
      status.state == 'working' && !_readOnly,
      _history.truncated,
    );
    // What the list draws: the thread as the reader left it while they are away from the end.
    final view = _frozen ?? _live;
    final typing = view.typing;
    final slots =
        (typing ? 1 : 0) + view.blocks.length + (view.truncated ? 1 : 0);

    return Scaffold(
      appBar: _appBar(status, all),
      body: LayoutBuilder(
        builder: (context, body) => Column(
          children: [
            // Crew, breadcrumb, jobs, todos and the live line sit above the timeline. On a
            // short phone, with large text or the keyboard up, they must not take the whole
            // screen: they scroll inside a capped area and the timeline keeps the rest.
            CappedStrips(
              bodyHeight: body.maxHeight,
              children: [
                if (tree.subs.isNotEmpty) ...[
                  CrewRail(
                    tree: tree,
                    focus: _focus,
                    fallbackAgent: status.agent,
                    onFocus: _focusNode,
                    onInspect: (n) => showAgentDetail(
                      context,
                      tree: _tree,
                      id: n.id,
                      fallbackAgent: status.agent,
                      onOpen: _focusNode,
                      onJump: (id) => showAgentDetail(
                        context,
                        tree: _tree,
                        id: id,
                        fallbackAgent: status.agent,
                        onOpen: _focusNode,
                        onJump: _focusNode,
                        onMessage: status.agent == 'omp' ? _messageAbout : null,
                      ),
                      onMessage: status.agent == 'omp' ? _messageAbout : null,
                    ),
                  ),
                ],
                if (_focus.isNotEmpty)
                  Breadcrumb(path: _crumbPath(tree), onTap: _focusNode),
                if (_focus.isEmpty) ...[
                  JobsStrip(tree: tree, onOpen: _focusNode),
                  if (tree.node('')?.todos case final todos?
                      when todos.total > 0)
                    TodoCard(todos),
                ],
                if (focusNode != null &&
                    (focusNode.running || tree.subs.isNotEmpty))
                  NowStrip(
                    node: focusNode,
                    since: focusNode.isRoot
                        ? status.stateStartedAt
                        : DateTime.now().millisecondsSinceEpoch -
                              focusNode.durationMs,
                  ),
              ],
            ),
            Expanded(
              child: Stack(
                children: [
                  RefreshIndicator(
                    color: c.accent,
                    onRefresh: () async {
                      await Future.wait([_loadHistory(), _loadTree()]);
                    },
                    child: GestureDetector(
                      behavior: HitTestBehavior.translucent,
                      onHorizontalDragEnd: (d) =>
                          _swipe(d.primaryVelocity ?? 0),
                      child: ListView.builder(
                        controller: _scroll,
                        // Newest at the bottom, scrolled there on open.
                        reverse: true,
                        physics: const AlwaysScrollableScrollPhysics(),
                        padding: const EdgeInsets.all(Space.lg),
                        // Only the blocks near the viewport are built: a 200-item chat
                        // costs what it shows.
                        itemCount: slots,
                        itemBuilder: (context, i) {
                          if (typing && i == 0) {
                            return const Padding(
                              padding: EdgeInsets.only(top: Space.md),
                              child: Align(
                                alignment: Alignment.centerLeft,
                                child: TypingBubble(),
                              ),
                            );
                          }
                          final at = i - (typing ? 1 : 0);
                          if (at < view.blocks.length) {
                            return _blockAt(
                              view.blocks,
                              view.blocks.length - 1 - at,
                              focusNode?.parent,
                            );
                          }
                          return Padding(
                            padding: const EdgeInsets.only(top: Space.md),
                            child: Center(
                              child: Text(
                                'older messages not shown',
                                style: Theme.of(context).textTheme.labelSmall!
                                    .copyWith(color: c.muted),
                              ),
                            ),
                          );
                        },
                      ),
                    ),
                  ),
                  if (shown.isEmpty && _loaded)
                    const EmptyState(
                      title: 'Nothing here yet',
                      message: 'What this agent does shows up here.',
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
            // What waits for an answer sits above the composer. With the keyboard up on a
            // short phone it scrolls inside the room that is left, so the composer, which
            // is where the user types, never leaves the screen.
            // Arriving later, it grows from the composer's edge instead of shoving the thread in one frame.
            AnimatedSize(
              duration: Motion.reduced(context) ? Duration.zero : Motion.normal,
              curve: Motion.ease,
              alignment: Alignment.bottomCenter,
              child: approval == null && _error == null
                  ? const SizedBox(width: double.infinity)
                  : ConstrainedBox(
                      constraints: BoxConstraints(
                        maxHeight: body.maxHeight * .25,
                      ),
                      child: SingleChildScrollView(
                        child: Column(
                          children: [
                            if (approval != null)
                              ApprovalBanner(
                                approval: approval,
                                onAnswer: (allow) => _approve(approval, allow),
                              ),
                            if (_error != null)
                              Padding(
                                padding: const EdgeInsets.symmetric(
                                  horizontal: Space.md,
                                ),
                                child: Text(
                                  _error!,
                                  style: Theme.of(context).textTheme.bodySmall!
                                      .copyWith(color: c.error),
                                ),
                              ),
                          ],
                        ),
                      ),
                    ),
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
