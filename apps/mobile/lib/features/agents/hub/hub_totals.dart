import 'package:atlas_mobile/core/protocol/agent_tree.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';

/// What the hub's header adds up over [nodes]: cost, time at work, requests, tool calls,
/// tokens.
class HubTotals {
  const HubTotals({
    this.cost = 0,
    this.activeMs = 0,
    this.requests = 0,
    this.tools = 0,
    this.tokens = 0,
  });
  final double cost;
  final int activeMs;
  final int requests;
  final int tools;
  final int tokens;

  factory HubTotals.of(Iterable<AgentNode> nodes) {
    var cost = 0.0;
    var active = 0, req = 0, tools = 0, tok = 0;
    for (final n in nodes) {
      cost += n.cost;
      active += n.activeMs;
      req += n.requests;
      tools += n.toolCount;
      tok += n.tokens;
    }
    return HubTotals(
      cost: cost,
      activeMs: active,
      requests: req,
      tools: tools,
      tokens: tok,
    );
  }

  /// `$0.31 · 4m 2s active · 12 req · 30 tools · 48.2k tok`, the unknown left out.
  String get line => [
    costText(cost),
    if (activeMs > 0) '${durationText(activeMs)} active',
    if (requests > 0) '$requests req',
    if (tools > 0) '$tools tools',
    if (tokens > 0) '${tokensText(tokens)} tok',
  ].where((s) => s.isNotEmpty).join(' · ');
}
