/// A crewmate's token usage (today, 7 days, 30 days, a 30-day bar chart, the cost the engine reported) and
/// what the engine CLIs expose about their limits. The desktop's Usage section, plus `engine_limits`.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/crew/ops/crew_ops_provider.dart';

class CrewmateUsageScreen extends ConsumerWidget {
  const CrewmateUsageScreen({
    super.key,
    required this.device,
    required this.crewmateId,
    required this.name,
  });

  final MacDevice device;
  final String crewmateId;
  final String name;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final c = AtlasColors.of(context);
    final key = (device: device, crewmateId: crewmateId);
    final usage = ref.watch(usageProvider(key));
    final limits = ref.watch(engineLimitsProvider(device));
    return Scaffold(
      appBar: AppBar(
        title: const Text('Usage'),
        actions: [
          IconButton(
            tooltip: 'Refresh',
            icon: const Icon(Icons.refresh_rounded),
            onPressed: () {
              ref.invalidate(usageProvider(key));
              ref.invalidate(engineLimitsProvider(device));
            },
          ),
        ],
      ),
      body: usage.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => EmptyState(
          title: "Couldn't read the usage",
          message: '${e is StateError ? e.message : e}',
          action: OutlinedButton(
            onPressed: () => ref.invalidate(usageProvider(key)),
            child: const Text('Try again'),
          ),
        ),
        data: (records) {
          final now = DateTime.now();
          final totals = usageTotals(records, now);
          return ListView(
            padding: const EdgeInsets.fromLTRB(
              Space.lg,
              Space.sm,
              Space.lg,
              Space.xxl,
            ),
            children: [
              Text(
                '$name: tokens (input + output) its engine reported.',
                style: TextStyle(color: c.muted, fontSize: 13),
              ),
              const SizedBox(height: Space.lg),
              Row(
                children: [
                  _Stat('Today', fmtTokens(totals.today)),
                  _Stat('7 days', fmtTokens(totals.week)),
                  _Stat('30 days', fmtTokens(totals.month)),
                ],
              ),
              const SizedBox(height: Space.lg),
              _Bars(usageBuckets(records, now)),
              if (totals.cost > 0) ...[
                const SizedBox(height: Space.md),
                Text(
                  'Reported cost, 30 days: \$${totals.cost.toStringAsFixed(2)}',
                  style: TextStyle(color: c.muted, fontSize: 13),
                ),
              ],
              if (records.isEmpty)
                Padding(
                  padding: const EdgeInsets.only(top: Space.lg),
                  child: Text(
                    'No usage recorded yet. Only engines that report tokens show here.',
                    style: TextStyle(color: c.faint, fontSize: 13),
                  ),
                ),
              const SectionHeader('Engine limits'),
              limits.when(
                loading: () => const LinearProgressIndicator(),
                error: (e, _) => Text(
                  '${e is StateError ? e.message : e}',
                  style: TextStyle(color: c.faint, fontSize: 13),
                ),
                data: (l) => Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    if (l.message.isNotEmpty)
                      Text(
                        l.message,
                        style: TextStyle(
                          color: c.muted,
                          fontSize: 13,
                          height: 1.4,
                        ),
                      ),
                    for (final e in l.limits.entries)
                      AtlasCard(
                        margin: const EdgeInsets.only(top: Space.sm),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              e.key,
                              style: const TextStyle(
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                            const SizedBox(height: 4),
                            SelectableText(
                              e.value,
                              style: TextStyle(
                                fontFamily: monoFamily,
                                fontFamilyFallback: monoFallback,
                                fontSize: 12,
                                color: c.muted,
                              ),
                            ),
                          ],
                        ),
                      ),
                  ],
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}

class _Stat extends StatelessWidget {
  const _Stat(this.label, this.value);
  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Expanded(
      child: AtlasCard(
        margin: const EdgeInsets.only(right: Space.sm),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              value,
              style: const TextStyle(fontSize: 20, fontWeight: FontWeight.w700),
            ),
            const SizedBox(height: 2),
            Text(label, style: TextStyle(color: c.faint, fontSize: 12)),
          ],
        ),
      ),
    );
  }
}

/// One bar per day, the tallest at full height; a day with nothing is a hairline.
class _Bars extends StatelessWidget {
  const _Bars(this.buckets);
  final List<int> buckets;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final top = buckets.fold<int>(0, (m, v) => v > m ? v : m);
    return Semantics(
      label: 'Tokens per day, last 30 days',
      child: SizedBox(
        height: 64,
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            for (final v in buckets)
              Expanded(
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 1),
                  child: Container(
                    height: top == 0 ? 2 : (v / top * 64).clamp(2, 64),
                    decoration: BoxDecoration(
                      color: v == 0 ? c.borderSubtle : c.accent,
                      borderRadius: BorderRadius.circular(2),
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}
