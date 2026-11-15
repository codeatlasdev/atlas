/// The audit of a crewmate (tool calls, approvals, task lifecycle, setting changes) or of the crew itself
/// (crewmates created or removed, pause and resume): what the desktop's Settings → Audit shows, newest first,
/// filtered by kind. Tapping an entry opens it whole with a Copy button.
library;

import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/async_view.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/crew/ops/crew_ops_provider.dart';

class CrewAuditScreen extends ConsumerStatefulWidget {
  /// [crewmateId] null reads the crew-wide log.
  const CrewAuditScreen({
    super.key,
    required this.device,
    this.crewmateId,
    required this.title,
  });

  final MacDevice device;
  final String? crewmateId;

  /// Whose audit: the crewmate's name, or the crew server's.
  final String title;

  @override
  ConsumerState<CrewAuditScreen> createState() => _CrewAuditScreenState();
}

class _CrewAuditScreenState extends ConsumerState<CrewAuditScreen> {
  AuditGroup? _group;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final scope = (device: widget.device, crewmateId: widget.crewmateId);
    final audit = ref.watch(auditProvider(scope));
    return Scaffold(
      appBar: AppBar(
        title: const Text('Audit'),
        actions: [
          IconButton(
            tooltip: 'Refresh',
            icon: const Icon(Icons.refresh_rounded),
            onPressed: () => ref.invalidate(auditProvider(scope)),
          ),
        ],
      ),
      body: AsyncView(
        value: audit,
        errorTitle: "Couldn't read the audit",
        onRetry: () => ref.invalidate(auditProvider(scope)),
        data: (entries) {
          final shown = filterAudit(entries, _group);
          // The crew-wide log has none of the crewmate kinds to filter by.
          final filterable = widget.crewmateId != null;
          return ListView(
            padding: const EdgeInsets.fromLTRB(
              Space.lg,
              Space.sm,
              Space.lg,
              Space.xxl,
            ),
            children: [
              Text(
                widget.crewmateId == null
                    ? '${widget.title}: every change to the crew.'
                    : '${widget.title}: everything it ran or asked to run.',
                style: TextStyle(color: c.muted, fontSize: 13),
              ),
              if (filterable) ...[
                const SizedBox(height: Space.md),
                _Filters(
                  value: _group,
                  onChanged: (g) => setState(() => _group = g),
                ),
              ],
              const SizedBox(height: Space.md),
              if (shown.isEmpty)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: Space.xxl),
                  child: Text(
                    _group == null
                        ? 'Nothing recorded yet.'
                        : 'Nothing recorded under ${_group!.label}.',
                    textAlign: TextAlign.center,
                    style: TextStyle(color: c.muted),
                  ),
                )
              else
                for (final e in shown)
                  _EntryRow(entry: e, onTap: () => _open(e)),
            ],
          );
        },
      ),
    );
  }

  void _open(AuditEntry e) {
    showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      builder: (_) => _EntrySheet(entry: e),
    );
  }
}

class _Filters extends StatelessWidget {
  const _Filters({required this.value, required this.onChanged});
  final AuditGroup? value;
  final void Function(AuditGroup?) onChanged;

  @override
  Widget build(BuildContext context) {
    return SingleChildScrollView(
      scrollDirection: Axis.horizontal,
      child: Row(
        children: [
          _Chip('All', selected: value == null, onTap: () => onChanged(null)),
          for (final g in AuditGroup.values) ...[
            const SizedBox(width: Space.sm),
            _Chip(g.label, selected: value == g, onTap: () => onChanged(g)),
          ],
        ],
      ),
    );
  }
}

class _Chip extends StatelessWidget {
  const _Chip(this.label, {required this.selected, required this.onTap});
  final String label;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Semantics(
      button: true,
      selected: selected,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: Container(
          constraints: const BoxConstraints(minHeight: 36),
          alignment: Alignment.center,
          padding: const EdgeInsets.symmetric(horizontal: 14),
          decoration: BoxDecoration(
            color: selected ? c.accent.withValues(alpha: .16) : c.base,
            borderRadius: BorderRadius.circular(999),
            border: Border.all(color: selected ? c.accent : c.borderSubtle),
          ),
          child: Text(
            label,
            style: TextStyle(
              fontSize: 13,
              fontWeight: FontWeight.w600,
              color: selected ? c.accent : c.muted,
            ),
          ),
        ),
      ),
    );
  }
}

String _time(int ts) {
  final d = DateTime.fromMillisecondsSinceEpoch(ts * 1000);
  String two(int n) => n.toString().padLeft(2, '0');
  return '${d.year}-${two(d.month)}-${two(d.day)} ${two(d.hour)}:${two(d.minute)}:${two(d.second)}';
}

class _EntryRow extends StatelessWidget {
  const _EntryRow({required this.entry, required this.onTap});
  final AuditEntry entry;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return AtlasCard(
      onTap: onTap,
      margin: const EdgeInsets.only(bottom: Space.sm),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            entry.summary,
            style: TextStyle(color: c.text, fontSize: 14, height: 1.35),
          ),
          const SizedBox(height: 4),
          Text(
            '${_time(entry.ts)} · ${auditKindLabel(entry.kind)}',
            style: TextStyle(color: c.muted, fontSize: 12),
          ),
        ],
      ),
    );
  }
}

class _EntrySheet extends StatelessWidget {
  const _EntrySheet({required this.entry});
  final AuditEntry entry;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Padding(
      padding: const EdgeInsets.all(Space.lg),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            auditKindLabel(entry.kind),
            style: Theme.of(context).textTheme.titleMedium,
          ),
          const SizedBox(height: 2),
          Text(_time(entry.ts), style: TextStyle(color: c.muted, fontSize: 12)),
          const SizedBox(height: Space.md),
          Flexible(
            child: SingleChildScrollView(
              child: SelectableText(
                const JsonEncoder.withIndent('  ').convert(entry.raw),
                style: TextStyle(
                  fontFamily: monoFamily,
                  fontFamilyFallback: monoFallback,
                  fontSize: 12,
                  color: c.text,
                  height: 1.4,
                ),
              ),
            ),
          ),
          const SizedBox(height: Space.md),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              TextButton.icon(
                icon: const Icon(Icons.copy_rounded, size: 16),
                label: const Text('Copy'),
                onPressed: () {
                  Clipboard.setData(ClipboardData(text: entry.copyText));
                  ScaffoldMessenger.of(context)
                      .showSnackBar(const SnackBar(content: Text('Copied')));
                },
              ),
            ],
          ),
        ],
      ),
    );
  }
}
