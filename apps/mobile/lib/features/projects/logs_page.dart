import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/latest_only.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';

/// Logs, full screen: filter by service, search, and follow live. Lines are split into
/// service tag, time and text; stderr and error lines stand out.
class LogsPage extends ConsumerStatefulWidget {
  const LogsPage({
    super.key,
    required this.device,
    required this.projectId,
    required this.services,
    this.service,
  });
  final MacDevice device;
  final String projectId;
  final List<String> services;
  final String? service;

  @override
  ConsumerState<LogsPage> createState() => _LogsPageState();
}

/// `[api:12] 12:01:02 [stderr] text` → (service, time, stderr, text).
final _logLine = RegExp(
  r'^\[([^:\]]+)(?::\d+)?\]\s+(\d\d:\d\d:\d\d)\s+\[(stdout|stderr)\]\s?(.*)$',
);

class _LogsPageState extends ConsumerState<LogsPage> {
  final _search = TextEditingController();
  late String? _service = widget.service;
  List<String> _lines = const [];
  String? _error;
  bool _loading = true;
  bool _live = true;
  Timer? _timer;
  final _latest = LatestOnly();

  @override
  void initState() {
    super.initState();
    _load();
    _timer = Timer.periodic(const Duration(seconds: 3), (_) {
      if (_live && _search.text.trim().isEmpty) _load(poll: true);
    });
  }

  @override
  void dispose() {
    _latest.close();
    _timer?.cancel();
    _search.dispose();
    super.dispose();
  }

  /// Loads `project_logs` for the current service and search. A live poll ([poll]) skips while the
  /// app is in the background, while the computer is not connected, and while its previous request is
  /// unanswered. A search or a service pick always asks and supersedes anything in flight: only the
  /// newest question's reply is applied, so a slow reply for `a` cannot overwrite the lines for `ab`.
  Future<void> _load({bool poll = false}) async {
    final mux = liveMux(ref, widget.device);
    if (mux == null) return;
    if (poll && !appInForeground()) return;
    final ticket = poll ? _latest.tryBegin() : _latest.begin();
    if (ticket == null) return;
    try {
      if (!poll) setState(() => _loading = true);
      final q = _search.text.trim();
      final resp = await mux.call('project_logs', {
        'id': widget.projectId,
        'service': ?_service,
        'lines': 300,
        if (q.isNotEmpty) 'query': q,
      });
      if (!mounted || !ticket.isCurrent) return;
      setState(() {
        _loading = false;
        if (resp['ok'] == true) {
          _error = null;
          _lines = (resp['data']?['text'] as String? ?? '')
              .split('\n')
              .where((l) => l.trim().isNotEmpty)
              .toList();
        } else {
          _error = resp['error'] as String? ?? 'error';
        }
      });
    } finally {
      ticket.done();
    }
  }

  void _pick(String? service) {
    setState(() => _service = service);
    _load();
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Scaffold(
      appBar: AppBar(
        title: Text(
          _service == null ? 'Logs' : 'Logs · $_service',
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
        ),
        actions: [
          TextButton.icon(
            onPressed: () => setState(() => _live = !_live),
            icon: StatusDot(
              color: _live ? c.success : c.faint,
              pulse: _live,
              size: 6,
            ),
            label: Text(
              _live ? 'Live' : 'Paused',
              style: TextStyle(color: _live ? c.success : c.muted),
            ),
          ),
        ],
      ),
      body: Column(
        children: [
          SizedBox(
            height: 56,
            child: ListView(
              scrollDirection: Axis.horizontal,
              padding: const EdgeInsets.symmetric(horizontal: Space.md),
              children: [
                for (final s in [null, ...widget.services])
                  Padding(
                    padding: const EdgeInsets.only(right: 6, top: 4, bottom: 4),
                    child: ChoiceChip(
                      label: Text(s ?? 'All'),
                      selected: _service == s,
                      showCheckmark: false,
                      onSelected: (_) => _pick(s),
                      side: BorderSide(color: c.borderSubtle),
                      selectedColor: c.raised,
                      backgroundColor: c.base,
                      labelStyle: Theme.of(context).textTheme.bodyMedium!
                          .copyWith(color: _service == s ? c.text : c.muted),
                    ),
                  ),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(
              Space.md,
              Space.xs,
              Space.md,
              Space.sm,
            ),
            child: TextField(
              controller: _search,
              textInputAction: TextInputAction.search,
              decoration: InputDecoration(
                hintText: 'Search (text or /regex/)',
                prefixIcon: Icon(Icons.search_rounded, color: c.faint),
              ),
              onSubmitted: (_) => _load(),
            ),
          ),
          Expanded(
            child: _loading && _lines.isEmpty
                ? const Center(child: CircularProgressIndicator())
                : _error != null
                ? EmptyState(title: 'Could not read logs', message: _error!)
                : _lines.isEmpty
                ? EmptyState(
                    icon: Icon(Icons.subject_rounded, size: 48, color: c.faint),
                    title: 'No log lines yet',
                    message: 'Lines show up here as the services write them.',
                  )
                : ListView.builder(
                    reverse: true,
                    padding: const EdgeInsets.fromLTRB(
                      Space.md,
                      0,
                      Space.md,
                      Space.lg,
                    ),
                    itemCount: _lines.length,
                    itemBuilder: (_, i) => _LogRow(
                      line: _lines[_lines.length - 1 - i],
                      showService: _service == null,
                      services: widget.services,
                    ),
                  ),
          ),
        ],
      ),
    );
  }
}

class _LogRow extends StatelessWidget {
  const _LogRow({
    required this.line,
    required this.showService,
    required this.services,
  });
  final String line;
  final bool showService;
  final List<String> services;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final mono = Theme.of(context).textTheme.bodySmall!.copyWith(
      color: AtlasColors.of(context).text,
      fontFamily: monoFamily,
      fontFamilyFallback: monoFallback,
      height: 1.4,
    );
    final m = _logLine.firstMatch(line);
    if (m == null) {
      return SelectableText(line, style: mono.copyWith(color: c.muted));
    }
    final service = m[1]!, time = m[2]!, err = m[3] == 'stderr', text = m[4]!;
    final bad =
        err ||
        RegExp(
          r'\b(error|panic|fatal|exception)\b',
          caseSensitive: false,
        ).hasMatch(text);
    final i = services.indexOf(service);
    final tag = c.series[(i < 0 ? service.length : i) % c.series.length];
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 1),
      child: SelectableText.rich(
        TextSpan(
          style: mono,
          children: [
            TextSpan(
              text: '$time ',
              style: TextStyle(color: c.muted),
            ),
            if (showService)
              TextSpan(
                text: '$service ',
                style: TextStyle(color: tag, fontWeight: FontWeight.w600),
              ),
            TextSpan(
              text: text,
              style: TextStyle(color: bad ? c.error : c.text),
            ),
          ],
        ),
      ),
    );
  }
}
