/// A crewmate's profile: what it does, how it works (approval, replies, memory), its automations,
/// its GitHub account, and removing it from the crew.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/crew/automations.dart';
import 'package:atlas_mobile/features/crew/automation_editor_sheet.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/chat_motion.dart';
import 'package:atlas_mobile/features/crew/profile/automations_section.dart';
import 'package:atlas_mobile/features/crew/profile/github_card.dart';
import 'package:atlas_mobile/features/crew/profile/profile_dialogs.dart';
import 'package:atlas_mobile/features/crew/profile/profile_header.dart';
import 'package:atlas_mobile/features/crew/profile/profile_rows.dart';
import 'package:atlas_mobile/features/crew/widgets/github_device_login.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_look.dart';

class CrewmateProfileScreen extends ConsumerStatefulWidget {
  const CrewmateProfileScreen({
    super.key,
    required this.device,
    required this.crewmateId,
  });

  final MacDevice device;
  final String crewmateId;

  @override
  ConsumerState<CrewmateProfileScreen> createState() =>
      _CrewmateProfileScreenState();
}

class _CrewmateProfileScreenState extends ConsumerState<CrewmateProfileScreen> {
  CrewGithubStatus? _githubStatus;
  bool _githubLoading = false;
  late final _login = GithubDeviceLogin(onChanged: () => setState(() {}));

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _loadGithub());
  }

  Future<void> _loadGithub() async {
    final mux = await ref.read(
      connectionNotifierProvider(widget.device).future,
    );
    if (mux == null) return;
    final ghResp = await mux.call('crew_github_status', {});
    if (mounted && ghResp['ok'] == true) {
      final ghData = (ghResp['data'] as Map?)?.cast<String, dynamic>() ?? {};
      setState(() {
        _githubStatus = CrewGithubStatus.fromJson(ghData);
      });
    }
  }

  Future<void> _editRole(String current) async {
    final result = await askRole(context, current);
    if (result == null || !mounted) return;
    final mux = await ref.read(
      connectionNotifierProvider(widget.device).future,
    );
    if (mux == null) return;
    await mux.call('crewmate_update', {
      'id': widget.crewmateId,
      'role': result,
    });
    ref.invalidate(crewProvider(widget.device));
  }

  Future<void> _editMemory() async {
    final mux = await ref.read(
      connectionNotifierProvider(widget.device).future,
    );
    if (mux == null) return;

    final resp = await mux.call('crewmate_memory', {'id': widget.crewmateId});
    if (!mounted || resp['ok'] != true) return;
    final text = (resp['data'] as Map?)?['text'] as String? ?? '';
    final saved = await editMemory(context, text);
    if (saved == null || !mounted) return;
    await mux.call('crewmate_memory_set', {
      'id': widget.crewmateId,
      'text': saved,
    });
  }

  Future<void> _deleteCrewmate() async {
    if (!await confirmDeleteCrewmate(context) || !mounted) return;
    final mux = await ref.read(
      connectionNotifierProvider(widget.device).future,
    );
    if (mux == null) return;
    final resp = await mux.call('crewmate_delete', {'id': widget.crewmateId});
    if (!mounted) return;
    if (resp['ok'] == true) {
      Navigator.of(context).pop();
      ref.invalidate(crewProvider(widget.device));
    } else {
      _toast(resp['error'] as String? ?? 'Could not delete crewmate');
    }
  }

  // ── automations ───────────────────────────────────────────────────────────

  Future<void> _saveTriggers(List<Trigger> triggers) async {
    final mux = await ref.read(
      connectionNotifierProvider(widget.device).future,
    );
    if (mux == null) return;
    await mux.call('crewmate_triggers_set', {
      'id': widget.crewmateId,
      'triggers': triggers.map((t) => t.toJson()).toList(),
    });
    ref.invalidate(crewProvider(widget.device));
  }

  Future<void> _addAutomation(String crewmateName) async {
    final result = await showAutomationEditor(
      context,
      crewmateName: crewmateName,
    );
    if (result == null) return;
    final crewmate = ref
        .read(crewProvider(widget.device))
        .value
        ?.crewmates
        .where((b) => b.id == widget.crewmateId)
        .firstOrNull;
    await _saveTriggers([...?crewmate?.triggers, result]);
  }

  Future<void> _editAutomation(CrewmateEntry crewmate, Trigger trigger) async {
    final result = await showAutomationEditor(
      context,
      existing: trigger,
      crewmateName: crewmate.name,
    );
    if (result == null) return;
    await _saveTriggers([
      for (final t in crewmate.triggers)
        if (t.id == trigger.id) result else t,
    ]);
  }

  Future<void> _deleteAutomation(
    CrewmateEntry crewmate,
    Trigger trigger,
  ) async {
    if (!await confirmRemoveAutomation(
      context,
      describeTrigger(trigger.kind),
    )) {
      return;
    }
    await _saveTriggers(
      crewmate.triggers.where((t) => t.id != trigger.id).toList(),
    );
  }

  // ── github ────────────────────────────────────────────────────────────────

  Future<void> _startLogin() async {
    final mux = await ref.read(
      connectionNotifierProvider(widget.device).future,
    );
    if (mux == null) return;
    setState(() => _githubLoading = true);
    try {
      final error = await _login.start(mux, _pollLogin);
      if (error != null && mounted) _toast(error);
    } finally {
      if (mounted) setState(() => _githubLoading = false);
    }
  }

  Future<void> _pollLogin() async {
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return;
    final (result, data) = await _login.pollOnce(mux);
    if (!mounted) return;
    switch (result) {
      case GithubPoll.done:
        setState(() {
          _githubStatus = CrewGithubStatus(
            installed: true,
            login: data['login'] as String?,
          );
        });
      case GithubPoll.error:
        _toast(data['error']?.toString() ?? 'GitHub sign-in failed');
      case GithubPoll.pending:
    }
  }

  void _toast(String text) =>
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(text)));

  Future<void> _logout() async {
    final mux = await ref.read(
      connectionNotifierProvider(widget.device).future,
    );
    if (mux == null) return;
    await mux.call('crew_github_logout', {});
    if (!mounted) return;
    setState(() {
      _githubStatus = CrewGithubStatus(
        installed: _githubStatus?.installed ?? true,
        login: null,
      );
    });
  }

  @override
  void dispose() {
    _login.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final crewData = ref.watch(crewProvider(widget.device)).value;
    final crewmate = crewData?.crewmates
        .where((b) => b.id == widget.crewmateId)
        .firstOrNull;

    final hostName = crewData?.hostName ?? widget.device.name;
    final state = crewmate == null
        ? CrewState.idle
        : CrewState.of(crewmate.state);
    return Scaffold(
      body: crewmate == null
          ? Center(
              child: crewData == null
                  ? const CircularProgressIndicator()
                  : Text(
                      'Crewmate not found',
                      style: TextStyle(color: c.faint),
                    ),
            )
          : Stack(
              children: [
                // The crewmate's own colour, a soft glow behind its portrait.
                Positioned(
                  top: -220,
                  left: -80,
                  right: -80,
                  height: 560,
                  child: IgnorePointer(
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        gradient: RadialGradient(
                          colors: [
                            CrewLook.from(
                              crewmate.look,
                              crewmate.id,
                            ).color.withValues(alpha: .26),
                            CrewLook.from(
                              crewmate.look,
                              crewmate.id,
                            ).color.withValues(alpha: 0),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
                ListView(
                  physics: const BouncingScrollPhysics(
                    parent: AlwaysScrollableScrollPhysics(),
                  ),
                  padding: EdgeInsets.fromLTRB(
                    Space.lg,
                    MediaQuery.paddingOf(context).top + 56,
                    Space.lg,
                    Space.xxl + MediaQuery.paddingOf(context).bottom,
                  ),
                  children: [
                    EnterFade(
                      child: ProfileHeader(
                        crewmate: crewmate,
                        hostName: hostName,
                        state: state,
                      ),
                    ),
                    const SizedBox(height: Space.xl),

                    EnterFade(
                      index: 1,
                      child: ProfileGroup(
                        title: 'What it does',
                        children: [
                          ProfileRow(
                            onTap: () => _editRole(crewmate.role),
                            child: Text(
                              crewmate.role.isNotEmpty
                                  ? crewmate.role
                                  : 'Tap to describe its job.',
                              style: TextStyle(
                                color: crewmate.role.isNotEmpty
                                    ? c.text
                                    : c.faint,
                                height: 1.45,
                                fontSize: 15,
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),

                    EnterFade(
                      index: 2,
                      child: ProfileGroup(
                        title: 'How it works',
                        children: [
                          ProfileRow(
                            label: 'Approval',
                            child: ApprovalPicker(
                              value: crewmate.approval,
                              enabled: crewmate.approvalSupported,
                              onChanged: (val) async {
                                final mux = ref
                                    .read(
                                      connectionNotifierProvider(widget.device),
                                    )
                                    .value;
                                if (mux == null) return;
                                await mux.call('crewmate_update', {
                                  'id': widget.crewmateId,
                                  'approval': val,
                                });
                                ref.invalidate(crewProvider(widget.device));
                              },
                            ),
                          ),
                          ProfileRow(
                            label: 'Short replies',
                            child: ShortRepliesRow(
                              value: crewmate.shortReplies,
                              onChanged: (val) async {
                                final mux = ref
                                    .read(
                                      connectionNotifierProvider(widget.device),
                                    )
                                    .value;
                                if (mux == null) return;
                                await mux.call('crewmate_update', {
                                  'id': widget.crewmateId,
                                  'short_replies': val,
                                });
                                ref.invalidate(crewProvider(widget.device));
                              },
                            ),
                          ),
                          ProfileRow(
                            icon: Icons.auto_stories_outlined,
                            label: 'Memory',
                            trailingText: 'View / Edit',
                            onTap: _editMemory,
                          ),
                        ],
                      ),
                    ),

                    EnterFade(
                      index: 3,
                      child: AutomationsSection(
                        crewmate: crewmate,
                        now: DateTime.now(),
                        onAdd: () => _addAutomation(crewmate.name),
                        onEdit: (t) => _editAutomation(crewmate, t),
                        onDelete: (t) => _deleteAutomation(crewmate, t),
                        onToggle: (t, enabled) async {
                          await _saveTriggers([
                            for (final tr in crewmate.triggers)
                              if (tr.id == t.id)
                                tr.copyWith(enabled: enabled)
                              else
                                tr,
                          ]);
                        },
                      ),
                    ),
                    const SizedBox(height: Space.lg),

                    EnterFade(
                      index: 4,
                      child: ProfileGroup(
                        title: 'Accounts',
                        children: [
                          Padding(
                            padding: const EdgeInsets.all(Space.md),
                            child: GithubCard(
                              status: _githubStatus,
                              flow: _login.flow,
                              loading: _githubLoading,
                              polling: _login.polling,
                              onConnect: _startLogin,
                              onDisconnect: _logout,
                              onCancelFlow: _login.cancel,
                            ),
                          ),
                        ],
                      ),
                    ),

                    EnterFade(
                      index: 5,
                      child: ProfileGroup(
                        children: [
                          ProfileRow(
                            icon: Icons.delete_outline_rounded,
                            label: 'Remove from the crew',
                            danger: true,
                            onTap: _deleteCrewmate,
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
                // Floating back button, like the chat's.
                Positioned(
                  top: MediaQuery.paddingOf(context).top + 6,
                  left: Space.md,
                  child: Semantics(
                    button: true,
                    label: 'Back',
                    child: Pressable(
                      onTap: () => Navigator.of(context).maybePop(),
                      child: Container(
                        width: 44,
                        height: 44,
                        decoration: BoxDecoration(
                          color: c.overlay.withValues(alpha: .82),
                          shape: BoxShape.circle,
                          border: Border.all(color: c.border),
                        ),
                        child: Icon(
                          Icons.arrow_back_ios_new_rounded,
                          size: 18,
                          color: c.text,
                        ),
                      ),
                    ),
                  ),
                ),
              ],
            ),
    );
  }
}
