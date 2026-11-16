import 'package:atlas_mobile/core/protocol/agent_tree.dart';

/// How the hub lists agents: one list, or the spawn tree.
enum HubLayout { flat, byParent }

/// Which agents the hub lists.
enum HubFilter {
  all,
  running,
  done,
  failed;

  bool admits(AgentNode n) => switch (this) {
    HubFilter.all => true,
    HubFilter.running => n.running || n.status == 'pending',
    HubFilter.done => n.done,
    HubFilter.failed => n.failed,
  };
}
