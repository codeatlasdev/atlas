import 'dart:async';
import 'dart:convert';

import 'package:atlas_mobile/core/demo/demo_agents.dart';
import 'package:atlas_mobile/core/demo/demo_bus.dart';
import 'package:atlas_mobile/core/demo/demo_crew_server.dart';
import 'package:atlas_mobile/core/demo/demo_git.dart';
import 'package:atlas_mobile/core/demo/demo_projects.dart';
import 'package:atlas_mobile/core/demo/demo_terminal.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/transport/transport.dart';

/// The demo computer's identity. Never dialled: [demoTransport] answers for it.
const demoDevice = MacDevice(nodeId: 'demo', name: 'Demo MacBook');

bool isDemo(MacDevice d) => d.nodeId == demoDevice.nodeId;

/// A computer that lives in the app: it answers the same protocol as `atlas hostd`
/// (replies by id, pushes with `ev`), so every screen runs its real code against it.
/// "Explore a demo" on the pairing screen opens it (App Review, and anyone without a
/// computer at hand); tests use it as a self-contained Transport.
Transport demoTransport({List<Map<String, String>> integrations = const []}) =>
    _DemoComputer(integrations);

/// The demo computer: a transport that answers hostd's commands from the parts it is made
/// of (projects, agents, terminals, the crew server), each of which owns its own data.
class _DemoComputer implements Transport {
  /// `agent_integrations` agents, in hostd's shape (`{id, label, state}`).
  final List<Map<String, String>> _integrations;

  _DemoComputer(this._integrations) {
    agents = DemoAgents(bus);
    projects = DemoProjects(bus, agents);
    terminal = DemoTerminal(bus, agents);
    crew = DemoCrewServer(bus);
    _tick = Timer.periodic(
      const Duration(seconds: 3),
      (_) => projects.jitter(),
    );
  }

  final bus = DemoBus();
  late final Timer _tick;
  late final DemoAgents agents;
  late final DemoProjects projects;
  late final DemoTerminal terminal;
  late final DemoCrewServer crew;
  final _git = DemoGit();

  @override
  Stream<String> get incomingLines => bus.lines;

  @override
  Future<void> close() async {
    if (!bus.isOpen) return;
    _tick.cancel();
    await bus.close();
  }

  @override
  Future<void> sendLine(String line) async {
    if (!bus.isOpen) throw StateError('closed');
    final req = jsonDecode(line) as Map<String, dynamic>;
    final cmd = req['cmd'] as String?;
    final args = (req['args'] as Map?)?.cast<String, dynamic>() ?? const {};
    // Each part answers the commands it owns; the rest are this computer's own.
    final r =
        projects.handle(cmd ?? '', args) ??
        agents.handle(cmd ?? '', args) ??
        terminal.handle(cmd ?? '', args) ??
        crew.handle(cmd ?? '', args) ??
        _own(cmd, args);
    final reply = r.error == null
        ? {'id': req['id'], 'ok': true, 'data': r.data}
        : {'id': req['id'], 'ok': false, 'error': r.error};
    scheduleMicrotask(() => bus.emit(reply));
  }

  /// The commands that belong to no part: the computer itself, its repository, the agents
  /// it can launch (which reaches into the projects) and the push stubs.
  DemoReply _own(String? cmd, Map<String, dynamic> args) {
    Object? data;
    String? error;
    switch (cmd) {
      case 'agent_integrations':
        data = {'agents': _integrations};
      case 'agent_launch':
        final wt = args['worktree'] as String?;
        if (wt != null && !projects.hasWorktree(wt)) {
          error = 'unknown worktree $wt in this project';
        } else {
          data = agents.launch(
            args['agent'] as String? ?? 'omp',
            args['prompt'] as String?,
          );
        }
      case 'agent_approve':
        final pane = args['pane'] as String?;
        final req = args['req'] as String?;
        error = pane != null && pane.startsWith('crew:')
            ? crew.approve(pane, req)
            : agents.approve(pane, req, args['allow'] == true);
        data = error == null ? <String, Object?>{} : null;
      case 'git_status':
        data = _git.status();
      case 'git_diff':
        data = _git.diff(args['path'] as String?, args['staged'] == true);
      case 'git_stage' || 'git_unstage':
        error = _git.stage(
          (args['paths'] as List?)?.cast<String>() ?? const [],
          on: cmd == 'git_stage',
        );
        data = error == null ? <String, Object?>{} : null;
      case 'git_commit_message':
        data = _git.message();
      case 'git_commit':
        final r = _git.commit(args['message'] as String? ?? '');
        data = r.data;
        error = r.error;
      case 'git_push':
        data = _git.push();
      case 'git_pr_create':
        data = _git.pr(args['title'] as String? ?? '');
      case 'push_register':
        data = <String, Object?>{};
      case 'push_unregister':
        data = <String, Object?>{};
      case 'info':
        data = {
          'proto': kAppProto,
          'caps': ['mobile', 'crew'],
          'version': '1.0.0',
          'hostname': 'Demo MacBook',
        };
      default:
        data = <String, Object?>{};
    }
    return (data: data, error: error);
  }
}
