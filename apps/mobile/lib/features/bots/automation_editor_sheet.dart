/// Bottom-sheet editor for creating or editing a bot automation trigger.
/// Returns a [Trigger] when the user taps Save, or null when cancelled.
library;

import 'dart:math';

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/bots/automations.dart';

// ─── public entry point ───────────────────────────────────────────────────────

/// Show the automation editor sheet and return the resulting [Trigger], or null
/// if the user cancelled.  Pass [existing] to edit an existing trigger.
Future<Trigger?> showAutomationEditor(
  BuildContext context, {
  Trigger? existing,
  required String botName,
}) {
  return showModalBottomSheet<Trigger>(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    builder: (_) =>
        _AutomationEditorSheet(existing: existing, botName: botName),
  );
}

// ─── kind enum ───────────────────────────────────────────────────────────────

enum _Kind { schedule, interval, github, file, bot, service }

extension _KindLabel on _Kind {
  String get label => switch (this) {
    _Kind.schedule => 'Schedule',
    _Kind.interval => 'Interval',
    _Kind.github => 'GitHub',
    _Kind.file => 'File',
    _Kind.bot => 'After another bot',
    _Kind.service => 'Service',
  };
}

_Kind _kindOf(TriggerKind k) => switch (k) {
  ScheduleTrigger() => _Kind.schedule,
  IntervalTrigger() => _Kind.interval,
  WebhookTrigger() => _Kind.schedule, // fallback
  GithubTrigger() => _Kind.github,
  FileTrigger() => _Kind.file,
  BotTrigger() => _Kind.bot,
  ServiceTrigger() => _Kind.service,
};

// ─── sheet ───────────────────────────────────────────────────────────────────

class _AutomationEditorSheet extends StatefulWidget {
  const _AutomationEditorSheet({this.existing, required this.botName});
  final Trigger? existing;
  final String botName;

  @override
  State<_AutomationEditorSheet> createState() => _AutomationEditorSheetState();
}

class _AutomationEditorSheetState extends State<_AutomationEditorSheet> {
  late _Kind _kind;

  // Schedule
  late List<int> _days;
  late int _hour;
  late int _minute;
  bool _useCustomCron = false;
  final _customCronCtrl = TextEditingController();

  // Interval
  int _intervalSecs = 3600;
  final _intervalCtrl = TextEditingController();

  // GitHub
  final _githubRepoCtrl = TextEditingController();
  final _githubEvents = <String>{'pr_opened'};

  // File
  final _filePathCtrl = TextEditingController();
  final _fileGlobCtrl = TextEditingController();

  // Bot
  final _fromBotCtrl = TextEditingController();
  String _botOn = 'done';

  // Service
  final _serviceCtrl = TextEditingController();
  String _serviceOn = 'failed';

  // Prompt (shared)
  final _promptCtrl = TextEditingController();

  @override
  void initState() {
    super.initState();
    final ex = widget.existing;
    _kind = ex != null ? _kindOf(ex.kind) : _Kind.schedule;
    _promptCtrl.text = ex?.prompt ?? '';

    if (ex?.kind case ScheduleTrigger(:final cron)) {
      final spec = scheduleFromCron(cron);
      if (spec != null) {
        _days = List.of(spec.days);
        _hour = spec.hour;
        _minute = spec.minute;
      } else {
        _days = [1, 2, 3, 4, 5];
        _hour = 9;
        _minute = 0;
        _useCustomCron = true;
        _customCronCtrl.text = cron;
      }
    } else {
      _days = [1, 2, 3, 4, 5];
      _hour = 9;
      _minute = 0;
    }

    if (ex?.kind case IntervalTrigger(:final everySecs)) {
      _intervalSecs = everySecs;
    }
    _intervalCtrl.text = '${_intervalSecs ~/ 60}';

    if (ex?.kind case GithubTrigger(:final repo, :final events)) {
      _githubRepoCtrl.text = repo;
      _githubEvents
        ..clear()
        ..addAll(events);
    }

    if (ex?.kind case FileTrigger(:final path, :final glob)) {
      _filePathCtrl.text = path;
      _fileGlobCtrl.text = glob ?? '';
    }

    if (ex?.kind case BotTrigger(:final fromBot, :final on)) {
      _fromBotCtrl.text = fromBot;
      _botOn = on;
    }

    if (ex?.kind case ServiceTrigger(:final service, :final on)) {
      _serviceCtrl.text = service;
      _serviceOn = on;
    }
  }

  @override
  void dispose() {
    _customCronCtrl.dispose();
    _intervalCtrl.dispose();
    _githubRepoCtrl.dispose();
    _filePathCtrl.dispose();
    _fileGlobCtrl.dispose();
    _fromBotCtrl.dispose();
    _serviceCtrl.dispose();
    _promptCtrl.dispose();
    super.dispose();
  }

  // ── build the TriggerKind from current state ──────────────────────────────

  TriggerKind? _buildKind() {
    return switch (_kind) {
      _Kind.schedule =>
        _useCustomCron
            ? ScheduleTrigger(cron: _customCronCtrl.text.trim())
            : ScheduleTrigger(
                cron: cronFromSchedule(
                  ScheduleSpec(days: _days, hour: _hour, minute: _minute),
                ),
              ),
      _Kind.interval => () {
        final m = int.tryParse(_intervalCtrl.text.trim()) ?? 60;
        return IntervalTrigger(everySecs: max(60, m * 60));
      }(),
      _Kind.github => GithubTrigger(
        repo: _githubRepoCtrl.text.trim(),
        events: _githubEvents.toList(),
      ),
      _Kind.file => FileTrigger(
        path: _filePathCtrl.text.trim(),
        glob: _fileGlobCtrl.text.trim().isNotEmpty
            ? _fileGlobCtrl.text.trim()
            : null,
      ),
      _Kind.bot => BotTrigger(fromBot: _fromBotCtrl.text.trim(), on: _botOn),
      _Kind.service => ServiceTrigger(
        projectDir: '',
        service: _serviceCtrl.text.trim(),
        on: _serviceOn,
      ),
    };
  }

  bool _valid() {
    final prompt = _promptCtrl.text.trim();
    if (prompt.isEmpty) return false;
    return switch (_kind) {
      _Kind.schedule =>
        _useCustomCron
            ? _customCronCtrl.text.trim().isNotEmpty
            : _days.isNotEmpty,
      _Kind.interval => true,
      _Kind.github =>
        _githubRepoCtrl.text.trim().isNotEmpty && _githubEvents.isNotEmpty,
      _Kind.file => _filePathCtrl.text.trim().isNotEmpty,
      _Kind.bot => _fromBotCtrl.text.trim().isNotEmpty,
      _Kind.service => _serviceCtrl.text.trim().isNotEmpty,
    };
  }

  void _save() {
    final k = _buildKind();
    if (k == null || !_valid()) return;
    final tz = DateTime.now().timeZoneName; // device zone name fallback
    final trigger = Trigger(
      id:
          widget.existing?.id ??
          DateTime.now().millisecondsSinceEpoch.toString(),
      kind: k,
      enabled: widget.existing?.enabled ?? true,
      prompt: _promptCtrl.text.trim(),
      tz: tz.isNotEmpty ? tz : null,
    );
    Navigator.of(context).pop(trigger);
  }

  // ── schedule weekday chips ────────────────────────────────────────────────

  static const _presets = [
    ('Every day', [0, 1, 2, 3, 4, 5, 6]),
    ('Weekdays', [1, 2, 3, 4, 5]),
  ];

  Widget _scheduleBuilder(BuildContext context) {
    final c = AtlasColors.of(context);
    if (_useCustomCron) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _label('Cron expression', c),
          const SizedBox(height: 6),
          Row(
            children: [
              Expanded(
                child: TextField(
                  controller: _customCronCtrl,
                  decoration: const InputDecoration(hintText: '0 9 * * 1-5'),
                  style: TextStyle(fontFamily: monoFamily, color: c.text),
                  onChanged: (_) => setState(() {}),
                ),
              ),
              TextButton(
                onPressed: () => setState(() => _useCustomCron = false),
                child: const Text('Builder'),
              ),
            ],
          ),
        ],
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _label('Days', c),
        const SizedBox(height: 6),
        // Preset chips
        Wrap(
          spacing: 6,
          children: [
            for (final (label, days) in _presets)
              FilterChip(
                label: Text(label, style: const TextStyle(fontSize: 12)),
                selected:
                    _days.length == days.length && days.every(_days.contains),
                onSelected: (_) => setState(() => _days = List.of(days)),
              ),
          ],
        ),
        const SizedBox(height: 8),
        // Weekday chips
        Wrap(
          spacing: 4,
          children: [
            for (int i = 0; i < 7; i++)
              FilterChip(
                label: Text(
                  ['Su', 'M', 'Tu', 'W', 'Th', 'F', 'Sa'][i],
                  style: const TextStyle(fontSize: 11),
                ),
                selected: _days.contains(i),
                onSelected: (on) => setState(() {
                  on ? _days.add(i) : _days.remove(i);
                  _days.sort();
                }),
                visualDensity: VisualDensity.compact,
                padding: EdgeInsets.zero,
              ),
          ],
        ),
        const SizedBox(height: 12),
        _label('Time', c),
        const SizedBox(height: 6),
        Row(
          children: [
            _timeDropdown(
              value: _hour,
              max: 23,
              label: (v) => v.toString().padLeft(2, '0'),
              onChanged: (v) => setState(() => _hour = v),
            ),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 6),
              child: Text(':', style: TextStyle(color: c.muted, fontSize: 18)),
            ),
            _timeDropdown(
              value: _minute,
              max: 59,
              step: 5,
              label: (v) => v.toString().padLeft(2, '0'),
              onChanged: (v) => setState(() => _minute = v),
            ),
            const Spacer(),
            TextButton(
              onPressed: () => setState(() => _useCustomCron = true),
              child: const Text('Custom cron'),
            ),
          ],
        ),
      ],
    );
  }

  Widget _timeDropdown({
    required int value,
    required int max,
    int step = 1,
    required String Function(int) label,
    required void Function(int) onChanged,
  }) {
    final values = [for (int i = 0; i <= max; i += step) i];
    // If current value not in steps, add it
    if (!values.contains(value)) {
      values
        ..add(value)
        ..sort();
    }
    return DropdownButton<int>(
      value: value,
      items: values
          .map((v) => DropdownMenuItem(value: v, child: Text(label(v))))
          .toList(),
      onChanged: (v) => onChanged(v!),
      underline: const SizedBox(),
      isDense: true,
    );
  }

  // ── interval builder ──────────────────────────────────────────────────────

  Widget _intervalBuilder(BuildContext context) {
    final c = AtlasColors.of(context);
    return Row(
      children: [
        SizedBox(
          width: 80,
          child: TextField(
            controller: _intervalCtrl,
            keyboardType: TextInputType.number,
            decoration: InputDecoration(
              hintText: '60',
              hintStyle: TextStyle(color: c.faint),
              isDense: true,
            ),
            onChanged: (_) => setState(() {}),
          ),
        ),
        const SizedBox(width: 8),
        Text('minutes', style: TextStyle(color: c.muted)),
        const Spacer(),
        ...[15, 30, 60, 120].map(
          (m) => Padding(
            padding: const EdgeInsets.only(left: 4),
            child: ActionChip(
              label: Text(
                m < 60 ? '${m}m' : '${m ~/ 60}h',
                style: const TextStyle(fontSize: 11),
              ),
              onPressed: () => setState(() {
                _intervalSecs = m * 60;
                _intervalCtrl.text = '$m';
              }),
              visualDensity: VisualDensity.compact,
              padding: EdgeInsets.zero,
            ),
          ),
        ),
      ],
    );
  }

  // ── github builder ────────────────────────────────────────────────────────

  static const _githubEventOptions = [
    ('pr_opened', 'PR opened'),
    ('pr_review_requested', 'Review requested'),
    ('issue_opened', 'Issue opened'),
    ('ci_failed', 'CI fails'),
    ('release', 'Release published'),
  ];

  Widget _githubBuilder(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _label('Repository (owner/repo)', c),
        const SizedBox(height: 6),
        TextField(
          controller: _githubRepoCtrl,
          decoration: const InputDecoration(
            hintText: 'owner/repo',
            isDense: true,
          ),
          onChanged: (_) => setState(() {}),
        ),
        const SizedBox(height: 12),
        _label('Events', c),
        const SizedBox(height: 6),
        Wrap(
          spacing: 4,
          runSpacing: 4,
          children: [
            for (final (key, label) in _githubEventOptions)
              FilterChip(
                label: Text(label, style: const TextStyle(fontSize: 12)),
                selected: _githubEvents.contains(key),
                onSelected: (on) => setState(() {
                  on ? _githubEvents.add(key) : _githubEvents.remove(key);
                }),
                visualDensity: VisualDensity.compact,
              ),
          ],
        ),
      ],
    );
  }

  // ── file builder ──────────────────────────────────────────────────────────

  Widget _fileBuilder(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _label('Path', c),
        const SizedBox(height: 6),
        TextField(
          controller: _filePathCtrl,
          decoration: const InputDecoration(hintText: '/src', isDense: true),
          onChanged: (_) => setState(() {}),
        ),
        const SizedBox(height: 10),
        _label('Glob (optional)', c),
        const SizedBox(height: 6),
        TextField(
          controller: _fileGlobCtrl,
          decoration: const InputDecoration(hintText: '**/*.ts', isDense: true),
        ),
      ],
    );
  }

  // ── bot builder ───────────────────────────────────────────────────────────

  Widget _botBuilder(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _label('Bot name', c),
        const SizedBox(height: 6),
        TextField(
          controller: _fromBotCtrl,
          decoration: const InputDecoration(
            hintText: 'Reviewer',
            isDense: true,
          ),
          onChanged: (_) => setState(() {}),
        ),
        const SizedBox(height: 10),
        SegmentedButton<String>(
          segments: const [
            ButtonSegment(value: 'done', label: Text('When it finishes')),
            ButtonSegment(value: 'failed', label: Text('When it fails')),
          ],
          selected: {_botOn},
          onSelectionChanged: (s) => setState(() => _botOn = s.first),
          showSelectedIcon: false,
        ),
      ],
    );
  }

  // ── service builder ───────────────────────────────────────────────────────

  Widget _serviceBuilder(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _label('Service name', c),
        const SizedBox(height: 6),
        TextField(
          controller: _serviceCtrl,
          decoration: const InputDecoration(hintText: 'api', isDense: true),
          onChanged: (_) => setState(() {}),
        ),
        const SizedBox(height: 10),
        SegmentedButton<String>(
          segments: const [
            ButtonSegment(value: 'failed', label: Text('When it fails')),
            ButtonSegment(value: 'recovered', label: Text('When it recovers')),
          ],
          selected: {_serviceOn},
          onSelectionChanged: (s) => setState(() => _serviceOn = s.first),
          showSelectedIcon: false,
        ),
      ],
    );
  }

  // ── build ─────────────────────────────────────────────────────────────────

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final isNew = widget.existing == null;

    return Padding(
      padding: EdgeInsets.fromLTRB(
        Space.lg,
        0,
        Space.lg,
        Space.lg + MediaQuery.viewInsetsOf(context).bottom,
      ),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            // Header
            Padding(
              padding: const EdgeInsets.symmetric(vertical: Space.md),
              child: Row(
                children: [
                  Text(
                    isNew ? 'New automation' : 'Edit automation',
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                  const Spacer(),
                  TextButton(
                    onPressed: () => Navigator.of(context).pop(),
                    child: const Text('Cancel'),
                  ),
                ],
              ),
            ),

            // Kind picker
            _label('Trigger', c),
            const SizedBox(height: 8),
            Wrap(
              spacing: 6,
              runSpacing: 6,
              children: [
                for (final k in _Kind.values)
                  ChoiceChip(
                    label: Text(k.label, style: const TextStyle(fontSize: 12)),
                    selected: _kind == k,
                    onSelected: (_) => setState(() => _kind = k),
                    visualDensity: VisualDensity.compact,
                  ),
              ],
            ),
            const SizedBox(height: Space.lg),

            // Kind-specific builder
            switch (_kind) {
              _Kind.schedule => _scheduleBuilder(context),
              _Kind.interval => _intervalBuilder(context),
              _Kind.github => _githubBuilder(context),
              _Kind.file => _fileBuilder(context),
              _Kind.bot => _botBuilder(context),
              _Kind.service => _serviceBuilder(context),
            },

            const SizedBox(height: Space.lg),

            // Prompt
            _label('Prompt', c),
            const SizedBox(height: 6),
            TextField(
              controller: _promptCtrl,
              minLines: 2,
              maxLines: 5,
              decoration: InputDecoration(
                hintText: 'What should ${widget.botName} do when this fires?',
                isDense: true,
              ),
              onChanged: (_) => setState(() {}),
            ),
            const SizedBox(height: Space.xl),

            // Save
            SizedBox(
              width: double.infinity,
              child: FilledButton(
                onPressed: _valid() ? _save : null,
                child: Text(isNew ? 'Add automation' : 'Save'),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _label(String text, AtlasColors c) => Text(
    text,
    style: TextStyle(fontSize: 12, color: c.muted, fontWeight: FontWeight.w500),
  );
}
