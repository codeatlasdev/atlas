/// Crewmate automation triggers (hostd `Trigger` / `TriggerKind`).
library;

/// The kind discriminant of a crewmate automation trigger.
/// JSON: `{"type": "schedule", "cron": "…"}` etc. (serde tag = "type").
sealed class TriggerKind {
  const TriggerKind();

  factory TriggerKind.fromJson(Map<String, dynamic> j) {
    final type = j['type'] as String? ?? '';
    return switch (type) {
      'schedule' => ScheduleTrigger(cron: j['cron'] as String? ?? ''),
      'interval' => IntervalTrigger(
        everySecs: (j['every_secs'] as num?)?.toInt() ?? 60,
      ),
      'webhook' => WebhookTrigger(token: j['token'] as String? ?? ''),
      'github' => GithubTrigger(
        repo: j['repo'] as String? ?? '',
        events: (j['events'] as List?)?.cast<String>() ?? const [],
        extra: {
          for (final e in j.entries)
            if (e.key != 'type' && e.key != 'repo' && e.key != 'events')
              e.key: e.value,
        },
      ),
      'file' => FileTrigger(
        path: j['path'] as String? ?? '',
        glob: j['glob'] as String?,
      ),
      'crewmate' => CrewmateTrigger(
        fromCrewmate: j['from_crewmate'] as String? ?? '',
        on: j['on'] as String? ?? 'done',
      ),
      'service' => ServiceTrigger(
        projectDir: j['project_dir'] as String? ?? '',
        service: j['service'] as String? ?? '',
        on: j['on'] as String? ?? 'failed',
      ),
      _ => ScheduleTrigger(cron: ''),
    };
  }

  Map<String, dynamic> toJson();
}

class ScheduleTrigger extends TriggerKind {
  const ScheduleTrigger({required this.cron});
  final String cron;
  @override
  Map<String, dynamic> toJson() => {'type': 'schedule', 'cron': cron};
}

class IntervalTrigger extends TriggerKind {
  const IntervalTrigger({required this.everySecs});
  final int everySecs;
  @override
  Map<String, dynamic> toJson() => {
    'type': 'interval',
    'every_secs': everySecs,
  };
}

class WebhookTrigger extends TriggerKind {
  const WebhookTrigger({required this.token});
  final String token;
  @override
  Map<String, dynamic> toJson() => {'type': 'webhook', 'token': token};
}

class GithubTrigger extends TriggerKind {
  const GithubTrigger({
    required this.repo,
    required this.events,
    this.extra = const {},
  });
  final String repo;
  final List<String> events;

  /// The fields the editor has no control for (authors, allow_auto, handle, assignee, ...),
  /// written back untouched: saving an edit replaces the whole automation on the server, so
  /// dropping them reset `authors` and `allow_auto` to their defaults.
  final Map<String, dynamic> extra;
  @override
  Map<String, dynamic> toJson() => {
    ...extra,
    'type': 'github',
    'repo': repo,
    'events': events,
  };
}

class FileTrigger extends TriggerKind {
  const FileTrigger({required this.path, this.glob});
  final String path;
  final String? glob;
  @override
  Map<String, dynamic> toJson() => {
    'type': 'file',
    'path': path,
    if (glob != null) 'glob': glob,
  };
}

class CrewmateTrigger extends TriggerKind {
  const CrewmateTrigger({required this.fromCrewmate, required this.on});
  final String fromCrewmate;
  final String on; // 'done' | 'failed'
  @override
  Map<String, dynamic> toJson() => {
    'type': 'crewmate',
    'from_crewmate': fromCrewmate,
    'on': on,
  };
}

class ServiceTrigger extends TriggerKind {
  const ServiceTrigger({
    required this.projectDir,
    required this.service,
    required this.on,
  });
  final String projectDir;
  final String service;
  final String on; // 'failed' | 'recovered'
  @override
  Map<String, dynamic> toJson() => {
    'type': 'service',
    'project_dir': projectDir,
    'service': service,
    'on': on,
  };
}

/// A crewmate automation trigger (schedule, interval, webhook, github, file, crewmate, service).
class Trigger {
  const Trigger({
    required this.id,
    required this.kind,
    required this.enabled,
    required this.prompt,
    this.lastFired,
    this.tz,
    this.nextFire,
  });
  final String id;
  final TriggerKind kind;
  final bool enabled;
  final String prompt;
  final int? lastFired;
  final String? tz;

  /// Unix seconds of the next scheduled fire; computed by hostd on crewmate_list.
  final int? nextFire;

  factory Trigger.fromJson(Map<String, dynamic> j) => Trigger(
    id: j['id'] as String? ?? '',
    kind: TriggerKind.fromJson((j['kind'] as Map).cast<String, dynamic>()),
    enabled: j['enabled'] as bool? ?? true,
    prompt: j['prompt'] as String? ?? '',
    lastFired: (j['last_fired'] as num?)?.toInt(),
    tz: j['tz'] as String?,
    nextFire: (j['next_fire'] as num?)?.toInt(),
  );

  Map<String, dynamic> toJson() => {
    'id': id,
    'kind': kind.toJson(),
    'enabled': enabled,
    'prompt': prompt,
    if (lastFired != null) 'last_fired': lastFired,
    if (tz != null) 'tz': tz,
  };

  Trigger copyWith({
    bool? enabled,
    String? prompt,
    TriggerKind? kind,
    String? tz,
  }) => Trigger(
    id: id,
    kind: kind ?? this.kind,
    enabled: enabled ?? this.enabled,
    prompt: prompt ?? this.prompt,
    lastFired: lastFired,
    tz: tz ?? this.tz,
    nextFire: nextFire,
  );
}
