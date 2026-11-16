import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/models.dart';

/// What a computer says about its Atlas: the installed version, what it can do, and whether a newer one is
/// offered (`info.update`, from the computer's own update check; absent on an Atlas that does not report it).
class HostInfo {
  const HostInfo({
    required this.version,
    required this.caps,
    this.offered,
    this.channel,
  });

  /// `1.5.0-alpha.25`.
  final String version;
  final List<String> caps;

  /// The version its channel offers, when its update check knows one.
  final String? offered;
  final String? channel;

  /// This Atlas can update itself when the phone asks (`atlas_update`).
  bool get canUpdateRemotely => caps.contains('update');

  /// A newer version is offered than the one installed.
  bool get updateAvailable => offered != null && offered != version;

  factory HostInfo.fromJson(Map<String, dynamic> j) {
    final update = (j['update'] as Map?)?.cast<String, dynamic>();
    return HostInfo(
      version: j['version'] as String? ?? 'unknown',
      caps: [for (final c in (j['caps'] as List?) ?? const []) '$c'],
      offered: update?['offered'] as String?,
      channel: update?['channel'] as String?,
    );
  }
}

/// [HostInfo] of [device], read once per connection (a reconnect reads it again: an update changes it).
final hostInfoProvider = FutureProvider.autoDispose.family<HostInfo, MacDevice>(
  (ref, device) async {
    final mux = await requireMux(ref, device);
    final resp = await mux.call('info');
    if (resp['ok'] != true) {
      throw StateError(resp['error'] as String? ?? 'the computer refused');
    }
    return HostInfo.fromJson((resp['data'] as Map).cast<String, dynamic>());
  },
);
