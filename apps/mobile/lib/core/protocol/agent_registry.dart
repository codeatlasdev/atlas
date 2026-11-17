import 'package:atlas_mobile/core/protocol/agent_registry_data.dart';

export 'package:atlas_mobile/core/protocol/agent_registry_data.dart';

/// The built-in agent with registry id [id], or null (a custom agent, or none).
BuiltinAgent? builtinAgentOf(String id) {
  for (final a in builtinAgents) {
    if (a.id == id) return a;
  }
  return null;
}
