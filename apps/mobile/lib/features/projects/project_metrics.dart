/// The project screen's data: one `project_detail` reply parsed, plus the history the phone
/// keeps between polls (the Mac sends only the current state, like the desktop panel).
library;

import 'package:atlas_mobile/core/protocol/models.dart';

/// One service as `project_detail` reports it (status merged with atlas.yaml).
class ServiceRow {
  final String name;
  final String state;
  final int? port;
  final String? health;
  final int restarts;
  final double memMb;
  final double cpu;
  final int? uptimeS;
  final String? note;
  final List<String> dependsOn;
  final String lastLine;

  const ServiceRow({
    required this.name,
    required this.state,
    this.port,
    this.health,
    this.restarts = 0,
    this.memMb = 0,
    this.cpu = 0,
    this.uptimeS,
    this.note,
    this.dependsOn = const [],
    this.lastLine = '',
  });

  bool get up => state == 'running' || state == 'starting';
}

class ProjectAction {
  final String key;
  final String label;
  final String? confirm;
  const ProjectAction({required this.key, required this.label, this.confirm});
}

class ProjectSnapshot {
  final String id;
  final String name;
  final String root;
  final bool running;
  final int? pid;
  final String? setupState;
  final String? setupCommand;
  final int errors5m;
  final List<ServiceRow> services;
  final List<ProjectAction> actions;

  /// Agents whose terminal runs inside this project: `(pane key, status)`.
  final List<(String, AgentStatus)> agents;

  const ProjectSnapshot({
    required this.id,
    required this.name,
    required this.root,
    required this.running,
    this.pid,
    this.setupState,
    this.setupCommand,
    this.errors5m = 0,
    this.services = const [],
    this.actions = const [],
    this.agents = const [],
  });

  int get upCount => services.where((s) => s.up).length;
  int get restarts => services.fold(0, (a, s) => a + s.restarts);
  double get memMb => services.fold(0.0, (a, s) => a + s.memMb);
  double get cpu => services.fold(0.0, (a, s) => a + s.cpu);

  /// Longest service uptime: how long the stack has been up.
  int? get uptimeS => services
      .map((s) => s.uptimeS)
      .whereType<int>()
      .fold<int?>(null, (a, b) => a == null || b > a ? b : a);

  /// Parse a `project_detail` reply. Services come from atlas.yaml (so stopped ones are
  /// listed too) and take their live state from the runner's status when it runs.
  factory ProjectSnapshot.fromDetail(Map<String, dynamic> d) {
    final status = (d['status'] as Map?)?.cast<String, dynamic>() ?? const {};
    final config = (d['config'] as Map?)?.cast<String, dynamic>() ?? const {};
    final lastLines =
        (d['last_lines'] as Map?)?.cast<String, dynamic>() ?? const {};
    final live = <String, Map<String, dynamic>>{
      for (final s in (status['services'] as List?) ?? const [])
        (s as Map)['name'] as String: s.cast<String, dynamic>(),
    };
    num? n(Object? v) => v is num ? v : null;
    final services = <ServiceRow>[
      for (final c in (config['services'] as List?) ?? const [])
        () {
          final cfg = (c as Map).cast<String, dynamic>();
          final name = cfg['name'] as String;
          final l = live[name] ?? const {};
          return ServiceRow(
            name: name,
            state: l['state'] as String? ?? 'stopped',
            port: n(l['port'] ?? cfg['port'])?.toInt(),
            health: l['health'] as String?,
            restarts: n(l['restarts'])?.toInt() ?? 0,
            memMb: n(l['mem_mb'])?.toDouble() ?? 0,
            cpu: n(l['cpu'])?.toDouble() ?? 0,
            uptimeS: n(l['uptime_s'])?.toInt(),
            note: l['note'] as String?,
            dependsOn: [
              for (final dep in (cfg['depends_on'] as List?) ?? const [])
                (dep as Map)['service'] as String,
            ],
            lastLine: _stripLogPrefix(lastLines[name] as String? ?? ''),
          );
        }(),
    ];
    final setup = (status['setup'] as Map?)?.cast<String, dynamic>();
    return ProjectSnapshot(
      id: d['id'] as String? ?? '',
      name: d['name'] as String? ?? '',
      root: d['root'] as String? ?? '',
      running: status['running'] == true,
      pid: n(status['pid'])?.toInt(),
      setupState: setup?['state'] as String?,
      setupCommand: setup?['command'] as String?,
      errors5m: n(d['errors_5m'])?.toInt() ?? 0,
      services: services,
      actions: [
        for (final a in (config['actions'] as List?) ?? const [])
          ProjectAction(
            key: (a as Map)['key'] as String,
            label: a['label'] as String? ?? a['key'] as String,
            confirm: a['confirm'] as String?,
          ),
      ],
      agents: [
        for (final a in (d['agents'] as List?) ?? const [])
          (
            (a as Map)['pane_key'] as String,
            AgentStatus.fromJson((a['status'] as Map).cast<String, dynamic>()),
          ),
      ],
    );
  }
}

/// `[api:12] 12:01:02 [stdout] listening` → `listening`.
String _stripLogPrefix(String line) {
  final m = RegExp(r'^\[[^\]]*\]\s+\d\d:\d\d:\d\d\s+\[(?:stdout|stderr)\]\s?')
      .firstMatch(line);
  return m == null ? line : line.substring(m.end);
}

/// Fixed-size history per service: memory, CPU and state, one sample per poll.
class ProjectHistory {
  ProjectHistory({this.capacity = 120});

  /// Samples kept (120 × 3 s = 6 min).
  final int capacity;
  final Map<String, List<double>> mem = {};
  final Map<String, List<double>> cpu = {};
  final Map<String, List<String>> states = {};

  void add(ProjectSnapshot s) {
    for (final svc in s.services) {
      _push(mem.putIfAbsent(svc.name, () => []), svc.memMb);
      _push(cpu.putIfAbsent(svc.name, () => []), svc.cpu);
      _push(states.putIfAbsent(svc.name, () => []), svc.state);
    }
  }

  void _push<T>(List<T> ring, T v) {
    ring.add(v);
    if (ring.length > capacity) ring.removeAt(0);
  }
}
