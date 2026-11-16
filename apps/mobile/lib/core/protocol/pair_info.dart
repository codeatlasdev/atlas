/// What an `atlas-mobile://pair` QR URL carries.
library;

/// Information parsed from an `atlas-mobile://pair?...` QR URL.
class PairInfo {
  final String nodeId;
  final String code;
  final String name;
  final List<String> addrs;
  final String? relay;

  const PairInfo({
    required this.nodeId,
    required this.code,
    required this.name,
    required this.addrs,
    this.relay,
  });

  /// Parses `atlas-mobile://pair?node=…&code=…&name=…[&addr=…]*[&relay=…]`.
  static PairInfo parse(String url) {
    final uri = Uri.parse(url);
    if (uri.scheme != 'atlas-mobile') {
      throw ArgumentError('Expected atlas-mobile:// scheme, got ${uri.scheme}');
    }
    if (uri.host != 'pair') {
      throw ArgumentError('Expected host "pair", got "${uri.host}"');
    }
    final node = uri.queryParameters['node'];
    final code = uri.queryParameters['code'];
    if (node == null || node.isEmpty) {
      throw ArgumentError('Missing required query parameter "node"');
    }
    if (code == null || code.isEmpty) {
      throw ArgumentError('Missing required query parameter "code"');
    }
    return PairInfo(
      nodeId: node,
      code: code,
      name: uri.queryParameters['name'] ?? '',
      addrs: uri.queryParametersAll['addr'] ?? [],
      relay: uri.queryParameters['relay'],
    );
  }
}
