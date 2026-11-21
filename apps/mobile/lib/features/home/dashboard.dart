import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/latest_only.dart';
import 'package:atlas_mobile/core/protocol/pty_open.dart';
import 'package:atlas_mobile/core/ui/async_view.dart';
import 'package:atlas_mobile/core/ui/computer_state.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/features/agents/agents_provider.dart';
import 'package:atlas_mobile/features/home/home_skeleton.dart';
import 'package:atlas_mobile/features/home/home_cards.dart';
import 'package:atlas_mobile/features/home/home_header.dart';
import 'package:atlas_mobile/features/home/project_groups.dart';
import 'package:atlas_mobile/features/home/project_tile.dart';
import 'package:atlas_mobile/features/inbox/inbox_providers.dart';
import 'package:atlas_mobile/features/projects/projects_provider.dart';
import 'package:atlas_mobile/features/terminal/new_terminal.dart';

class HomeDashboard extends ConsumerStatefulWidget {
  const HomeDashboard({
    super.key,
    required this.device,
    required this.others,
    required this.onSwitch,
  });

  final MacDevice device;
  final List<MacDevice> others;
  final void Function(MacDevice) onSwitch;

  @override
  ConsumerState<HomeDashboard> createState() => HomeDashboardState();
}

class HomeDashboardState extends ConsumerState<HomeDashboard> {
  Timer? _timer;
  final _poll = LatestOnly();

  @override
  void initState() {
    super.initState();
    // Services have no push: poll the project list. Agent states are pushed live.
    _timer = Timer.periodic(const Duration(seconds: 5), (_) => _tick());
  }

  /// One poll: only in the foreground, and never behind an unanswered one. A computer that is not
  /// connected sends nothing (`requireMux` throws before any command) and the screen keeps its list.
  Future<void> _tick() async {
    if (!appInForeground()) return;
    final ticket = _poll.tryBegin();
    if (ticket == null) return;
    final provider = projectListProvider(widget.device);
    try {
      ref.invalidate(provider);
      await ref.read(provider.future);
    } catch (_) {
      // The screen shows the failure (or keeps the last list); a poll has nobody to tell.
    } finally {
      ticket.done();
    }
  }

  @override
  void dispose() {
    _search.dispose();
    _poll.close();
    _timer?.cancel();
    super.dispose();
  }

  void _openProject(ProjectInfo p) =>
      context.push('/project', extra: {'device': widget.device, 'project': p});

  // The list's own state: what is typed in the search, which repositories show their idle worktrees, whether
  // the "Other folders" section is open. Kept across polls (keys are repository roots, not list positions).
  final _search = TextEditingController();
  String _query = '';
  final _unfolded = <String>{};
  bool _foldersOpen = false;

  /// The rows under "Projects": the search's hits, or one block per repository then the other folders.
  List<Widget> _rows(
    List<ProjectInfo> list,
    Map<String, List<(String, AgentStatus)>> agentsOf,
    Set<String> flagged,
  ) {
    final l10n = context.l10n;
    Widget tile(ProjectInfo p, {bool nested = false}) => ProjectTile(
      key: ValueKey('project-${p.id}'),
      project: p,
      agents: agentsOf[p.id] ?? const [],
      flagged: flagged.contains(p.id),
      onOpen: () => _openProject(p),
      nested: nested,
    );
    if (_query.trim().isNotEmpty) {
      final hits = searchProjects(list, _query, flagged: flagged);
      if (hits.isEmpty) {
        return [
          Padding(
            padding: const EdgeInsets.all(Space.xl),
            child: Text(
              l10n.mobile_projects_search_empty(_query.trim()),
              textAlign: TextAlign.center,
              style: Theme.of(context).textTheme.bodyLarge!
                  .copyWith(color: AtlasColors.of(context).muted),
            ),
          ),
        ];
      }
      return [for (final p in hits) tile(p)];
    }
    final home = groupProjects(list, flagged: flagged);
    return [
      for (final g in home.groups) ...[
        tile(g.head),
        // Active worktrees always show under their project; the idle ones on request.
        for (final w in g.worktrees)
          if (isActive(w, flagged) || _unfolded.contains(g.key))
            tile(w, nested: true),
        if (g.worktrees.any((w) => !isActive(w, flagged)))
          _FoldRow(
            key: ValueKey('fold-${g.key}'),
            label: _unfolded.contains(g.key)
                ? l10n.mobile_projects_worktrees_hide
                : l10n.mobile_projects_worktrees_show(
                    g.worktrees.where((w) => !isActive(w, flagged)).length,
                  ),
            open: _unfolded.contains(g.key),
            onTap: () => setState(() {
              if (!_unfolded.remove(g.key)) _unfolded.add(g.key);
            }),
          ),
      ],
      if (home.folders.isNotEmpty) ...[
        _FoldersHeader(
          count: home.folders.length,
          open: _foldersOpen,
          onTap: () => setState(() => _foldersOpen = !_foldersOpen),
        ),
        if (_foldersOpen)
          for (final f in home.folders) tile(f),
      ],
    ];
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final conn = ref.watch(connectionNotifierProvider(widget.device));
    final connected = conn.value != null;
    // Always watch so the last loaded value survives reconnects.
    final projects = ref.watch(projectListProvider(widget.device));
    // Live agent states override the list's snapshot as soon as they are pushed.
    final live = connected
        ? ref.watch(agentStatusMapNotifierProvider(widget.device)).value ??
              const {}
        : const <String, AgentStatus>{};

    final list = projects.value ?? const <ProjectInfo>[];
    final agentsOf = <String, List<(String, AgentStatus)>>{};
    for (final p in list) {
      for (final (pane, s0) in p.agents) {
        (agentsOf[p.id] ??= []).add((pane, live[pane] ?? s0));
      }
    }
    final attention = attentionFor(
      widget.device,
      projects: list,
      live: live,
      crewmates: const [],
    );
    // Projects that need you, then running, then stopped.
    final flagged = {
      for (final a in attention)
        if (a
            case AgentAttention(:final project) ||
                ServiceAttention(:final project))
          project.id,
    };
    final home = groupProjects(list, flagged: flagged);
    final missing = connected
        ? ref.watch(missingIntegrationsProvider(widget.device)).value ??
              const <AgentIntegration>[]
        : const <AgentIntegration>[];
    final working = agentsOf.values
        .expand((v) => v)
        .where((a) => a.$2.state == 'working')
        .length;

    // The backdrop is the app's (AtlasBackdrop); the list starts below the status bar: what scrolls away is cut
    // there, never drawn under the clock.
    return SafeArea(
      bottom: false,
      child: RefreshIndicator(
        color: c.accent,
        onRefresh: () async {
          if (!connected) {
            ref.invalidate(connectionNotifierProvider(widget.device));
          } else {
            ref.invalidate(projectListProvider(widget.device));
            ref.invalidate(missingIntegrationsProvider(widget.device));
          }
        },
        child: CustomScrollView(
          physics: const AlwaysScrollableScrollPhysics(),
          // The search keyboard goes away as soon as the list is scrolled.
          keyboardDismissBehavior: ScrollViewKeyboardDismissBehavior.onDrag,
          slivers: [
            SliverToBoxAdapter(
              child: HomeHeader(
                device: widget.device,
                conn: conn,
                others: widget.others,
                onSwitch: widget.onSwitch,
                status: !connected || projects.value == null
                    ? null
                    : attention.isNotEmpty
                    ? (
                        '${attention.length} ${attention.length == 1 ? 'needs' : 'need'} you',
                        c.warning,
                      )
                    : working > 0
                    ? (
                        '$working ${working == 1 ? 'agent' : 'agents'} working',
                        c.accent,
                      )
                    : ('All clear', c.success),
              ),
            ),
            // ── Content slivers ──────────────────────────────────────────
            if (!connected && !projects.hasValue && !conn.isLoading)
              // Out of reach and nothing known yet: the one place the full state belongs.
              // EmptyState scrolls itself (a LayoutBuilder: no intrinsic sizes), so the
              // sliver hands it all the room that is left.
              SliverFillRemaining(
                child: Padding(
                  padding: const EdgeInsets.only(top: Space.xxl),
                  child: ReachingView(device: widget.device, connecting: false),
                ),
              )
            else if (!projects.hasValue && !projects.hasError)
              // Reaching the computer or reading its list: the list's own shape, the header pill says
              // which ("Connecting…").
              SliverPadding(
                padding: const EdgeInsets.only(top: Space.sm),
                sliver: SliverList.list(
                  children: const [
                    Padding(
                      padding: EdgeInsets.fromLTRB(
                        Space.lg,
                        0,
                        Space.lg,
                        Space.xs,
                      ),
                      child: SectionHeader('Projects'),
                    ),
                    HomeProjectsSkeleton(),
                  ],
                ),
              )
            else ...[
              // The computer is out of reach but its last list is still here.
              if (!connected)
                SliverToBoxAdapter(
                  child: OfflineStrip(
                    device: widget.device,
                    suffix: ' · projects shown',
                  ),
                ),
              // Attention cards + integrations + section header
              SliverPadding(
                padding: const EdgeInsets.fromLTRB(
                  Space.lg,
                  Space.sm,
                  Space.lg,
                  0,
                ),
                sliver: SliverList.list(
                  children: [
                    for (final a in missing) HomeIntegrationOff(agent: a),
                    if (missing.isNotEmpty) const SizedBox(height: Space.md),
                    SectionHeader(
                      'Projects',
                      count: home.groups.length,
                      trailing: canOpenTerminal(widget.device)
                          ? TextButton.icon(
                              key: const Key('new-terminal-computer'),
                              onPressed: () => newTerminalOnComputer(
                                context,
                                ref,
                                widget.device,
                              ),
                              icon: const Icon(
                                Icons.terminal_rounded,
                                size: 18,
                              ),
                              label: const Text('New terminal'),
                            )
                          : null,
                    ),
                    // Only where a list is long enough to need it (or while a search is typed).
                    if (list.length > 5 || _query.isNotEmpty)
                      _ProjectSearch(
                        controller: _search,
                        onChanged: (v) => setState(() => _query = v),
                      ),
                  ],
                ),
              ),
              // The project list, kept through a reconnect or a failed refresh; "No projects
              // yet" only when the computer answered "none". Nothing loaded yet: the spinner,
              // or the error with "Try again" (AsyncView).
              if (!projects.hasValue)
                SliverFillRemaining(
                  child: AsyncView<List<ProjectInfo>>(
                    value: projects,
                    errorTitle: "Couldn't read the projects",
                    onRetry: () =>
                        ref.invalidate(projectListProvider(widget.device)),
                    data: (_) => const SizedBox.shrink(),
                  ),
                )
              else if (list.isEmpty)
                const SliverFillRemaining(
                  child: Padding(
                    padding: EdgeInsets.only(bottom: kFloatingBarScrollPad),
                    child: HomeNoProjects(),
                  ),
                )
              else
                SliverPadding(
                  padding: const EdgeInsets.fromLTRB(
                    0,
                    Space.xs,
                    0,
                    Space.xxl + kFloatingBarScrollPad,
                  ),
                  sliver: SliverList.list(
                    children: _rows(list, agentsOf, flagged),
                  ),
                ),
            ],
          ],
        ),
      ),
    );
  }
}

/// The search over projects, worktree branches and folders.
class _ProjectSearch extends StatelessWidget {
  const _ProjectSearch({required this.controller, required this.onChanged});
  final TextEditingController controller;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: Space.sm),
      child: ValueListenableBuilder<TextEditingValue>(
        valueListenable: controller,
        builder: (context, value, _) => TextField(
          key: const Key('project-search'),
          controller: controller,
          onChanged: onChanged,
          textInputAction: TextInputAction.search,
          autocorrect: false,
          decoration: InputDecoration(
            hintText: context.l10n.mobile_projects_search_hint,
            prefixIcon: Icon(Icons.search_rounded, color: c.faint),
            suffixIcon: value.text.isEmpty
                ? null
                : IconButton(
                    tooltip: context.l10n.mobile_projects_search_clear,
                    icon: Icon(Icons.close_rounded, color: c.faint),
                    onPressed: () {
                      controller.clear();
                      onChanged('');
                    },
                  ),
          ),
        ),
      ),
    );
  }
}

/// "N worktrees" / "Hide worktrees" under a repository: aligned with the rows' text, quiet until needed.
class _FoldRow extends StatelessWidget {
  const _FoldRow({
    super.key,
    required this.label,
    required this.open,
    required this.onTap,
  });
  final String label;
  final bool open;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Semantics(
      button: true,
      expanded: open,
      child: InkWell(
        onTap: onTap,
        child: Padding(
          // Under the title column: the avatar is 50 wide, then the row's gap.
          padding: const EdgeInsets.fromLTRB(
            Space.lg + 50 + Space.md,
            Space.sm,
            Space.lg,
            Space.sm,
          ),
          child: Row(
            children: [
              Icon(
                open ? Icons.expand_less_rounded : Icons.expand_more_rounded,
                size: 20,
                color: c.accent,
              ),
              const SizedBox(width: Space.xs),
              Flexible(
                child: Text(
                  label,
                  style: Theme.of(context).textTheme.bodyMedium!
                      .copyWith(color: c.accent, fontWeight: FontWeight.w600),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// The "Other folders" section: collapsed by default, says what those folders are when open.
class _FoldersHeader extends StatelessWidget {
  const _FoldersHeader({
    required this.count,
    required this.open,
    required this.onTap,
  });
  final int count;
  final bool open;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final t = Theme.of(context).textTheme;
    return Semantics(
      button: true,
      expanded: open,
      child: InkWell(
        key: const Key('other-folders'),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(
            Space.lg + 2,
            Space.xl,
            Space.lg,
            Space.sm,
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text.rich(
                      TextSpan(
                        text: context.l10n.mobile_projects_folders,
                        children: [
                          TextSpan(
                            text: '  $count',
                            style: TextStyle(color: c.muted),
                          ),
                        ],
                      ),
                      style: t.titleSmall,
                    ),
                  ),
                  Icon(
                    open
                        ? Icons.expand_less_rounded
                        : Icons.expand_more_rounded,
                    color: c.muted,
                  ),
                ],
              ),
              if (open)
                Padding(
                  padding: const EdgeInsets.only(top: Space.xs),
                  child: Text(
                    context.l10n.mobile_projects_folders_hint,
                    style: t.bodySmall!.copyWith(color: c.muted),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}
