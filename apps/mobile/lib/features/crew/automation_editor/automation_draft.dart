/// What the automation editor is editing: the fields of every trigger kind, the kind
/// picked, and the [Trigger] they add up to. A draft starts from an existing trigger (an
/// edit) or from the defaults (a new one).
library;

import 'dart:math';

import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/crew_triggers.dart';
import 'package:atlas_mobile/features/crew/automations.dart';

enum AutomationKind {
  schedule('Schedule'),
  interval('Interval'),
  github('GitHub'),
  file('File'),
  crewmate('After another crewmate'),
  service('Service');

  const AutomationKind(this.label);
  final String label;

  static AutomationKind of(TriggerKind k) => switch (k) {
    ScheduleTrigger() => AutomationKind.schedule,
    IntervalTrigger() => AutomationKind.interval,
    WebhookTrigger() => AutomationKind.schedule, // fallback
    GithubTrigger() => AutomationKind.github,
    FileTrigger() => AutomationKind.file,
    CrewmateTrigger() => AutomationKind.crewmate,
    ServiceTrigger() => AutomationKind.service,
  };
}

class AutomationDraft {
  AutomationDraft(this.existing) {
    final ex = existing;
    kind = ex != null ? AutomationKind.of(ex.kind) : AutomationKind.schedule;
    prompt.text = ex?.prompt ?? '';

    if (ex?.kind case ScheduleTrigger(:final cron)) {
      final spec = scheduleFromCron(cron);
      if (spec != null) {
        days = List.of(spec.days);
        hour = spec.hour;
        minute = spec.minute;
      } else {
        useCustomCron = true;
        customCron.text = cron;
      }
    }

    if (ex?.kind case IntervalTrigger(:final everySecs)) {
      intervalSecs = everySecs;
    }
    intervalMinutes.text = '${intervalSecs ~/ 60}';

    if (ex?.kind case GithubTrigger(:final repo, :final events)) {
      githubRepo.text = repo;
      githubEvents
        ..clear()
        ..addAll(events);
    }

    if (ex?.kind case FileTrigger(:final path, :final glob)) {
      filePath.text = path;
      fileGlob.text = glob ?? '';
    }

    if (ex?.kind case CrewmateTrigger(fromCrewmate: final from, :final on)) {
      fromCrewmate.text = from;
      crewmateOn = on;
    }

    if (ex?.kind case ServiceTrigger(service: final name, :final on)) {
      service.text = name;
      serviceOn = on;
    }
  }

  final Trigger? existing;
  late AutomationKind kind;

  // Schedule
  List<int> days = [1, 2, 3, 4, 5];
  int hour = 9;
  int minute = 0;
  bool useCustomCron = false;
  final customCron = TextEditingController();

  // Interval
  int intervalSecs = 3600;
  final intervalMinutes = TextEditingController();

  // GitHub
  final githubRepo = TextEditingController();
  final githubEvents = <String>{'pr_opened'};

  // File
  final filePath = TextEditingController();
  final fileGlob = TextEditingController();

  // Crewmate
  final fromCrewmate = TextEditingController();
  String crewmateOn = 'done';

  // Service
  final service = TextEditingController();
  String serviceOn = 'failed';

  // Prompt (shared)
  final prompt = TextEditingController();

  void dispose() {
    customCron.dispose();
    intervalMinutes.dispose();
    githubRepo.dispose();
    filePath.dispose();
    fileGlob.dispose();
    fromCrewmate.dispose();
    service.dispose();
    prompt.dispose();
  }

  /// The trigger kind the current fields describe.
  TriggerKind buildKind() {
    return switch (kind) {
      AutomationKind.schedule =>
        useCustomCron
            ? ScheduleTrigger(cron: customCron.text.trim())
            : ScheduleTrigger(
                cron: cronFromSchedule(
                  ScheduleSpec(days: days, hour: hour, minute: minute),
                ),
              ),
      AutomationKind.interval => () {
        final m = int.tryParse(intervalMinutes.text.trim()) ?? 60;
        return IntervalTrigger(everySecs: max(60, m * 60));
      }(),
      AutomationKind.github => GithubTrigger(
        repo: githubRepo.text.trim(),
        events: githubEvents.toList(),
        // What the sheet has no control for comes back untouched (see GithubTrigger.extra).
        extra: switch (existing?.kind) {
          GithubTrigger(:final extra) => extra,
          _ => const {},
        },
      ),
      AutomationKind.file => FileTrigger(
        path: filePath.text.trim(),
        glob: fileGlob.text.trim().isNotEmpty ? fileGlob.text.trim() : null,
      ),
      AutomationKind.crewmate => CrewmateTrigger(
        fromCrewmate: fromCrewmate.text.trim(),
        on: crewmateOn,
      ),
      AutomationKind.service => ServiceTrigger(
        projectDir: '',
        service: service.text.trim(),
        on: serviceOn,
      ),
    };
  }

  /// A prompt and the fields of the picked kind are filled in.
  bool get valid {
    if (prompt.text.trim().isEmpty) return false;
    return switch (kind) {
      AutomationKind.schedule =>
        useCustomCron ? customCron.text.trim().isNotEmpty : days.isNotEmpty,
      AutomationKind.interval => true,
      AutomationKind.github =>
        githubRepo.text.trim().isNotEmpty && githubEvents.isNotEmpty,
      AutomationKind.file => filePath.text.trim().isNotEmpty,
      AutomationKind.crewmate => fromCrewmate.text.trim().isNotEmpty,
      AutomationKind.service => service.text.trim().isNotEmpty,
    };
  }

  /// The trigger to save: the edited one keeps its id and enabled flag.
  Trigger toTrigger() {
    final tz = DateTime.now().timeZoneName; // device zone name fallback
    return Trigger(
      id: existing?.id ?? DateTime.now().millisecondsSinceEpoch.toString(),
      kind: buildKind(),
      enabled: existing?.enabled ?? true,
      prompt: prompt.text.trim(),
      tz: tz.isNotEmpty ? tz : null,
    );
  }
}
