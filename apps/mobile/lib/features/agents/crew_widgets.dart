import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';

Color nodeColor(AtlasColors c, AgentNode n) => switch (n.status) {
  'running' => c.accent,
  'completed' => c.success,
  'failed' || 'aborted' => c.error,
  _ => c.faint,
};

String nodeStatusLabel(AgentNode n) => switch (n.status) {
  'running' => 'running',
  'completed' => 'done',
  'failed' => 'failed',
  'aborted' => 'stopped',
  'idle' => 'idle',
  _ => 'queued',
};

/// One line of numbers: `48.2k tok · $0.31 · 12 tools`.
String statsLine(AgentNode n) => [
  if (n.tokens > 0) '${tokensText(n.tokens)} tok',
  if (costText(n.cost).isNotEmpty) costText(n.cost),
  if (n.toolCount > 0) '${n.toolCount} ${n.toolCount == 1 ? 'tool' : 'tools'}',
].join(' · ');

/// A node's avatar with its status ring: an arc that turns while it runs, a full ring
/// (green / red) when it ended, a dashed grey one while it waits to start.
class NodeRing extends StatefulWidget {
  const NodeRing({
    super.key,
    required this.node,
    required this.fallbackAgent,
    this.size = 34,
  });

  final AgentNode node;

  /// The root agent's id (`omp`): a subagent without its own `agent` shows its parent's.
  final String fallbackAgent;
  final double size;

  @override
  State<NodeRing> createState() => _NodeRingState();
}

class _NodeRingState extends State<NodeRing>
    with SingleTickerProviderStateMixin {
  late final AnimationController _a = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1100),
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _sync();
  }

  @override
  void didUpdateWidget(NodeRing old) {
    super.didUpdateWidget(old);
    _sync();
  }

  void _sync() {
    if (widget.node.running && !Motion.reduced(context)) {
      if (!_a.isAnimating) _a.repeat();
    } else {
      _a.stop();
      _a.value = 0;
    }
  }

  @override
  void dispose() {
    _a.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final n = widget.node;
    final s = widget.size;
    final agent = n.agent ?? widget.fallbackAgent;
    final badge = switch (n.status) {
      'completed' => (Icons.check_rounded, c.success),
      'failed' || 'aborted' => (Icons.close_rounded, c.error),
      _ => null,
    };
    return SizedBox.square(
      dimension: s + 8,
      child: Stack(
        alignment: Alignment.center,
        children: [
          AnimatedBuilder(
            animation: _a,
            builder: (_, _) => CustomPaint(
              size: Size.square(s + 8),
              painter: _RingPainter(
                color: nodeColor(c, n),
                status: n.status,
                turn: _a.value,
              ),
            ),
          ),
          if (agent.isNotEmpty && knownAgents.contains(agent))
            AgentLogo(agent, size: s - 8)
          else
            CircleAvatar(
              radius: (s - 8) / 2,
              backgroundColor: c.raised,
              child: Text(
                (n.label.isEmpty ? '·' : n.label[0]).toUpperCase(),
                style: TextStyle(
                  fontSize: s * .38,
                  fontWeight: FontWeight.w700,
                  color: c.text,
                ),
              ),
            ),
          if (badge != null)
            Positioned(
              right: 0,
              bottom: 0,
              child: Container(
                decoration: BoxDecoration(
                  color: c.ground,
                  shape: BoxShape.circle,
                ),
                padding: const EdgeInsets.all(1),
                child: Icon(badge.$1, size: 12, color: badge.$2),
              ),
            ),
        ],
      ),
    );
  }
}

class _RingPainter extends CustomPainter {
  _RingPainter({required this.color, required this.status, required this.turn});
  final Color color;
  final String status;
  final double turn;

  @override
  void paint(Canvas canvas, Size size) {
    final r = (size.shortestSide - 3) / 2;
    final rect = Rect.fromCircle(center: size.center(Offset.zero), radius: r);
    final p = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.5
      ..strokeCap = StrokeCap.round
      ..color = color;
    switch (status) {
      case 'running':
        canvas.drawCircle(
          rect.center,
          r,
          Paint()
            ..style = PaintingStyle.stroke
            ..strokeWidth = 2.5
            ..color = color.withValues(alpha: .18),
        );
        canvas.drawArc(rect, turn * 2 * math.pi, math.pi * .7, false, p);
      case 'pending':
        // A dashed ring: not started.
        const dashes = 14;
        for (var i = 0; i < dashes; i++) {
          canvas.drawArc(
            rect,
            i * 2 * math.pi / dashes,
            math.pi / dashes,
            false,
            p,
          );
        }
      default:
        canvas.drawCircle(rect.center, r, p);
    }
  }

  @override
  bool shouldRepaint(_RingPainter old) =>
      old.color != color || old.status != status || old.turn != turn;
}

/// The crew: the agent and its subagents as chips in a horizontal rail, children
/// indented behind a connector, the focused one lit. Tap focuses, long-press opens the
/// node's task and report.
class CrewRail extends StatelessWidget {
  const CrewRail({
    super.key,
    required this.tree,
    required this.focus,
    required this.fallbackAgent,
    required this.onFocus,
    required this.onInspect,
  });

  final AgentTree tree;
  final String focus;
  final String fallbackAgent;
  final void Function(String id) onFocus;
  final void Function(AgentNode node) onInspect;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final flat = tree.flat;
    return Container(
      decoration: BoxDecoration(
        color: c.base,
        border: Border(bottom: BorderSide(color: c.borderSubtle)),
      ),
      child: SizedBox(
        height: 66,
        child: ListView.builder(
          scrollDirection: Axis.horizontal,
          padding: const EdgeInsets.symmetric(horizontal: Space.sm),
          itemCount: flat.length,
          itemBuilder: (_, i) {
            final (n, depth) = flat[i];
            return _CrewChip(
              key: ValueKey('crew-${n.id}'),
              node: n,
              depth: depth,
              // A connector shows the chip belongs to the one before it.
              linked: depth > 0,
              focused: n.id == focus,
              fallbackAgent: fallbackAgent,
              onTap: () => onFocus(n.id),
              onLongPress: () => onInspect(n),
            );
          },
        ),
      ),
    );
  }
}

class _CrewChip extends StatefulWidget {
  const _CrewChip({
    super.key,
    required this.node,
    required this.depth,
    required this.linked,
    required this.focused,
    required this.fallbackAgent,
    required this.onTap,
    required this.onLongPress,
  });

  final AgentNode node;
  final int depth;
  final bool linked;
  final bool focused;
  final String fallbackAgent;
  final VoidCallback onTap;
  final VoidCallback onLongPress;

  @override
  State<_CrewChip> createState() => _CrewChipState();
}

class _CrewChipState extends State<_CrewChip> {
  AgentNode get node => widget.node;
  int get depth => widget.depth;
  bool get linked => widget.linked;
  bool get focused => widget.focused;
  String get fallbackAgent => widget.fallbackAgent;

  @override
  void didUpdateWidget(_CrewChip old) {
    super.didUpdateWidget(old);
    // Focus moved here (a swipe, the map, a result card): bring it into the rail.
    if (widget.focused && !old.focused) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) {
          Scrollable.ensureVisible(
            context,
            duration: Motion.normal,
            curve: Motion.ease,
            alignment: .5,
          );
        }
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final onTap = widget.onTap;
    final onLongPress = widget.onLongPress;
    final name = node.isRoot
        ? (node.agent ?? fallbackAgent).isEmpty
              ? 'Agent'
              : agentLabel(node.agent ?? fallbackAgent)
        : node.label;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (linked)
          // Depth reads as a longer stub: `└─` for children, `└──` for grandchildren.
          Container(width: 6.0 + 6 * depth, height: 2, color: c.border),
        Semantics(
          button: true,
          selected: focused,
          label: '${node.isRoot ? name : node.id}, ${nodeStatusLabel(node)}',
          child: InkWell(
            borderRadius: BorderRadius.circular(Space.radius),
            onTap: () {
              HapticFeedback.selectionClick();
              onTap();
            },
            onLongPress: () {
              HapticFeedback.mediumImpact();
              onLongPress();
            },
            child: AnimatedContainer(
              duration: Motion.normal,
              curve: Motion.ease,
              margin: const EdgeInsets.symmetric(vertical: 6),
              padding: const EdgeInsets.fromLTRB(4, 2, 10, 2),
              decoration: BoxDecoration(
                color: focused ? c.accent.withValues(alpha: .14) : null,
                border: Border.all(
                  color: focused ? c.accent : Colors.transparent,
                ),
                borderRadius: BorderRadius.circular(Space.radius),
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  NodeRing(node: node, fallbackAgent: fallbackAgent, size: 30),
                  const SizedBox(width: 6),
                  ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 110),
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 13,
                            fontWeight: FontWeight.w600,
                            color: focused ? c.text : c.muted,
                          ),
                        ),
                        Text(
                          nodeStatusLabel(node),
                          style: TextStyle(
                            fontSize: 11,
                            color: nodeColor(c, node),
                          ),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ],
    );
  }
}

/// `Main › Scout › Deep`, always on screen while a subagent is focused: every crumb but the
/// last is a chip that goes back up in one tap, `Main` first.
class Breadcrumb extends StatelessWidget {
  const Breadcrumb({super.key, required this.path, required this.onTap});

  final List<AgentNode> path;
  final void Function(String id) onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Container(
      width: double.infinity,
      decoration: BoxDecoration(
        color: c.base,
        border: Border(bottom: BorderSide(color: c.borderSubtle)),
      ),
      padding: const EdgeInsets.symmetric(horizontal: Space.md, vertical: 6),
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: Row(
          children: [
            for (var i = 0; i < path.length; i++) ...[
              if (i > 0) Icon(Icons.chevron_right, size: 16, color: c.faint),
              _Crumb(
                key: ValueKey('crumb-${path[i].id}'),
                label: path[i].isRoot ? 'Main' : path[i].label,
                current: i == path.length - 1,
                home: path[i].isRoot,
                onTap: i == path.length - 1 ? null : () => onTap(path[i].id),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _Crumb extends StatelessWidget {
  const _Crumb({
    super.key,
    required this.label,
    required this.current,
    required this.home,
    required this.onTap,
  });
  final String label;
  final bool current;
  final bool home;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Material(
      color: current ? c.accent.withValues(alpha: .15) : c.raised,
      shape: StadiumBorder(
        side: BorderSide(
          color: current ? c.accent.withValues(alpha: .5) : c.borderSubtle,
        ),
      ),
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (home) ...[
                Icon(Icons.arrow_back_rounded, size: 13, color: c.accent),
                const SizedBox(width: 4),
              ],
              Text(
                label,
                style: TextStyle(
                  fontSize: 12.5,
                  fontWeight: current || home
                      ? FontWeight.w700
                      : FontWeight.w500,
                  color: current
                      ? c.text
                      : home
                      ? c.accent
                      : c.muted,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// One live line under the app bar: what the focused agent is doing right now, a ticking
/// timer, tokens and cost. A soft shimmer sweeps it while it runs.
class NowStrip extends StatefulWidget {
  const NowStrip({super.key, required this.node, required this.since});

  final AgentNode node;

  /// Epoch ms the focused agent started (state start for the root, derived for
  /// subagents); the timer counts from here while it runs.
  final int since;

  @override
  State<NowStrip> createState() => _NowStripState();
}

class _NowStripState extends State<NowStrip>
    with SingleTickerProviderStateMixin {
  late final AnimationController _shimmer = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1800),
  );

  /// Re-reads the clock each second so the elapsed time ticks while it runs.
  Timer? _tick;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _sync();
  }

  @override
  void didUpdateWidget(NowStrip old) {
    super.didUpdateWidget(old);
    _sync();
  }

  void _sync() {
    if (widget.node.running) {
      _tick ??= Timer.periodic(
        const Duration(seconds: 1),
        (_) => mounted ? setState(() {}) : null,
      );
    } else {
      _tick?.cancel();
      _tick = null;
    }
    if (widget.node.running && !Motion.reduced(context)) {
      if (!_shimmer.isAnimating) _shimmer.repeat();
    } else {
      _shimmer.stop();
      _shimmer.value = 0;
    }
  }

  @override
  void dispose() {
    _tick?.cancel();
    _shimmer.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final n = widget.node;
    final running = n.running;
    final text = running
        ? (n.now ?? 'Thinking…')
        : switch (n.status) {
            'completed' => 'Finished',
            'failed' => 'Failed',
            'aborted' => 'Stopped',
            'pending' => 'Waiting to start',
            _ => 'Idle',
          };
    final dur = running ? elapsed(widget.since) : durationText(n.durationMs);
    final stats = statsLine(n);
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(
        horizontal: Space.lg,
        vertical: Space.sm,
      ),
      decoration: BoxDecoration(
        color: c.base,
        border: Border(bottom: BorderSide(color: c.borderSubtle)),
      ),
      child: Row(
        children: [
          Expanded(
            flex: 2,
            child: AnimatedBuilder(
              animation: _shimmer,
              builder: (_, child) => running && !Motion.reduced(context)
                  ? ShaderMask(
                      blendMode: BlendMode.srcIn,
                      shaderCallback: (r) => LinearGradient(
                        begin: Alignment(-1 + 3 * _shimmer.value, 0),
                        end: Alignment(3 * _shimmer.value, 0),
                        colors: [c.muted, c.text, c.muted],
                      ).createShader(r),
                      child: child,
                    )
                  : child!,
              child: Text(
                text,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 13,
                  color: running ? c.text : c.muted,
                ),
              ),
            ),
          ),
          if (dur.isNotEmpty || stats.isNotEmpty) ...[
            const SizedBox(width: Space.sm),
            Flexible(
              child: Text(
                [
                  if (dur.isNotEmpty) dur,
                  if (stats.isNotEmpty) stats,
                ].join(' · '),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 11.5,
                  color: c.faint,
                  fontFeatures: const [FontFeature.tabularFigures()],
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }
}

/// "Waiting on 2 jobs": the main agent's running subagents with a tiny progress bar each
/// (how much of their context window is used, else just a pulse). Tap one to open it.
class JobsStrip extends StatelessWidget {
  const JobsStrip({super.key, required this.tree, required this.onOpen});

  final AgentTree tree;
  final void Function(String id) onOpen;

  @override
  Widget build(BuildContext context) {
    final jobs = [
      for (final n in tree.subs)
        if (n.running || n.status == 'pending') n,
    ];
    if (jobs.isEmpty) return const SizedBox.shrink();
    final c = AtlasColors.of(context);
    return Container(
      width: double.infinity,
      decoration: BoxDecoration(
        color: c.base,
        border: Border(bottom: BorderSide(color: c.borderSubtle)),
      ),
      padding: const EdgeInsets.fromLTRB(
        Space.lg,
        Space.sm,
        Space.lg,
        Space.sm,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Waiting on ${jobs.length} ${jobs.length == 1 ? 'job' : 'jobs'}',
            key: const ValueKey('jobs-title'),
            style: TextStyle(
              fontSize: 12.5,
              fontWeight: FontWeight.w700,
              color: c.accent,
            ),
          ),
          for (final j in jobs.take(5))
            InkWell(
              key: ValueKey('job-${j.id}'),
              onTap: () => onOpen(j.id),
              child: Padding(
                padding: const EdgeInsets.symmetric(vertical: 3),
                child: Row(
                  children: [
                    Expanded(
                      flex: 5,
                      child: Text(
                        [
                          j.id,
                          if (j.now != null && j.now!.isNotEmpty) j.now!,
                        ].join(' · '),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(fontSize: 12.5, color: c.muted),
                      ),
                    ),
                    const SizedBox(width: Space.sm),
                    Expanded(
                      flex: 2,
                      child: ClipRRect(
                        borderRadius: BorderRadius.circular(2),
                        child: LinearProgressIndicator(
                          value: j.contextShare,
                          minHeight: 3,
                          color: c.accent,
                          backgroundColor: c.borderSubtle,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          if (jobs.length > 5)
            Text(
              '+${jobs.length - 5} more',
              style: TextStyle(fontSize: 11.5, color: c.faint),
            ),
        ],
      ),
    );
  }
}

/// The main agent's todo list (omp's todo tool): collapsed to the task in progress and a
/// count, expands to the phases with their tasks ticked.
class TodoCard extends StatefulWidget {
  const TodoCard(this.todos, {super.key});
  final TodoList todos;

  @override
  State<TodoCard> createState() => _TodoCardState();
}

class _TodoCardState extends State<TodoCard> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final t = widget.todos;
    final current = t.current;
    return Container(
      width: double.infinity,
      decoration: BoxDecoration(
        color: c.base,
        border: Border(bottom: BorderSide(color: c.borderSubtle)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          InkWell(
            key: const ValueKey('todo-toggle'),
            onTap: () => setState(() => _open = !_open),
            child: Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: Space.lg,
                vertical: Space.sm,
              ),
              child: Row(
                children: [
                  Icon(Icons.checklist_rounded, size: 16, color: c.muted),
                  const SizedBox(width: 6),
                  Text(
                    '${t.done}/${t.total}',
                    style: TextStyle(
                      fontSize: 12.5,
                      fontWeight: FontWeight.w700,
                      color: c.text,
                    ),
                  ),
                  const SizedBox(width: Space.sm),
                  Expanded(
                    child: Text(
                      current?.content ?? 'Todos',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(fontSize: 12.5, color: c.muted),
                    ),
                  ),
                  Icon(
                    _open ? Icons.expand_less : Icons.expand_more,
                    size: 18,
                    color: c.faint,
                  ),
                ],
              ),
            ),
          ),
          if (_open)
            ConstrainedBox(
              constraints: BoxConstraints(
                maxHeight: MediaQuery.sizeOf(context).height * .35,
              ),
              child: SingleChildScrollView(
                padding: const EdgeInsets.fromLTRB(
                  Space.lg,
                  0,
                  Space.lg,
                  Space.sm,
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    for (final p in t.phases) ...[
                      Padding(
                        padding: const EdgeInsets.only(top: 4),
                        child: Text(
                          p.name,
                          style: TextStyle(
                            fontSize: 12,
                            fontWeight: FontWeight.w700,
                            color: c.muted,
                          ),
                        ),
                      ),
                      for (final k in p.tasks) _TodoRow(k),
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

class _TodoRow extends StatelessWidget {
  const _TodoRow(this.task);
  final TodoTask task;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final (icon, color) = switch (task.status) {
      'completed' => (Icons.check_box_rounded, c.success),
      'in_progress' => (Icons.timelapse_rounded, c.accent),
      'blocked' => (Icons.block_rounded, c.error),
      'abandoned' => (Icons.indeterminate_check_box_outlined, c.faint),
      _ => (Icons.check_box_outline_blank_rounded, c.faint),
    };
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(icon, size: 16, color: color),
          const SizedBox(width: 6),
          Expanded(
            child: Text(
              task.content,
              style: TextStyle(
                fontSize: 12.5,
                height: 1.3,
                color: task.done || task.status == 'abandoned'
                    ? c.faint
                    : c.text,
                decoration: task.done ? TextDecoration.lineThrough : null,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
