import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/async_view.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/inbox/activity_row.dart';
import 'package:atlas_mobile/features/inbox/attention_cards.dart';
import 'package:atlas_mobile/features/inbox/inbox_providers.dart';

export 'package:atlas_mobile/features/inbox/inbox_providers.dart';

/// Inbox tab: everything that needs you, on every paired computer.
class InboxScreen extends ConsumerStatefulWidget {
  const InboxScreen({super.key});

  @override
  ConsumerState<InboxScreen> createState() => _InboxScreenState();
}

class _InboxScreenState extends ConsumerState<InboxScreen> {
  /// The "seen" mark as it was when the screen opened: unread dots stay put while you
  /// read, and clear on the next visit.
  DateTime? _seenBefore;

  @override
  void initState() {
    super.initState();
    _seenBefore = ref.read(inboxSeenProvider);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) ref.read(inboxSeenProvider.notifier).markSeen();
    });
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final items = ref.watch(inboxProvider);
    final recent = ref.watch(recentActivityProvider);
    final knowledge = inboxKnowledge(ref);
    void open(Attention a) => openAttention(context, a);
    // Nothing to list AND not everything answered: a spinner or the reason, never "All clear".
    if (items.isEmpty && recent.isEmpty && knowledge.unknown != null) {
      return Scaffold(
        body: SafeArea(
          bottom: false,
          child: Padding(
            padding: const EdgeInsets.only(bottom: kFloatingBarScrollPad),
            child: AsyncView<void>(
              value: knowledge.unknown == InboxUnknown.loading
                  ? const AsyncLoading<void>()
                  : AsyncError<void>(knowledge.error!, StackTrace.empty),
              data: (_) => const SizedBox.shrink(),
              errorTitle: "Couldn't read your inbox",
              onRetry: () {
                ref.invalidate(crewDevicesProvider);
                for (final d
                    in ref.read(crewDevicesProvider).value ??
                        const <MacDevice>[]) {
                  ref.invalidate(crewProvider(d));
                }
              },
            ),
          ),
        ),
      );
    }
    if (items.isEmpty && recent.isEmpty) {
      return Scaffold(
        body: SafeArea(
          bottom: false,
          child: Padding(
            padding: const EdgeInsets.only(bottom: kFloatingBarScrollPad),
            child: EmptyState(
              icon: Icon(
                Icons.notifications_none_rounded,
                size: 56,
                color: c.faint,
              ),
              title: 'All clear',
              message: 'What your crew and agents finish, and anything that needs you, shows up here.',
            ),
          ),
        ),
      );
    }
    return Scaffold(
      body: SafeArea(
        bottom: false,
        child: ListView(
          padding: const EdgeInsets.only(
            top: Space.md,
            bottom: Space.xxl + kFloatingBarScrollPad,
          ),
          children: [
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: Space.lg),
              child: Text(
                'Inbox',
                style: Theme.of(context).textTheme.titleLarge,
              ),
            ),
            if (items.isNotEmpty) ...[
              const Padding(
                padding: EdgeInsets.fromLTRB(Space.lg, Space.lg, Space.lg, 0),
                child: SectionHeader('Needs you'),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: Space.lg),
                child: Column(
                  children: [
                    AttentionHero(item: items.first, onOpen: open),
                    for (var i = 1; i < items.length; i++)
                      AttentionMore(item: items[i], onOpen: open),
                  ],
                ),
              ),
            ],
            if (recent.isNotEmpty) ...[
              const Padding(
                padding: EdgeInsets.fromLTRB(Space.lg, Space.lg, Space.lg, 0),
                child: SectionHeader('Recent'),
              ),
              for (var i = 0; i < recent.length; i++)
                ActivityRow(
                  item: recent[i],
                  unread:
                      _seenBefore == null || recent[i].at.isAfter(_seenBefore!),
                  last: i == recent.length - 1,
                ),
            ],
          ],
        ),
      ),
    );
  }
}
