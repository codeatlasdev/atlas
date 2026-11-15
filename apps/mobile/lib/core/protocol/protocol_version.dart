import 'package:atlas_mobile/core/protocol/models.dart';

/// The hostd wire protocol this app speaks: `PROTO` in `crates/atlas-host/src/protocol.rs`.
/// Bump both on any NON-additive wire change (a renamed or removed command or field, a changed
/// meaning); a new command or an optional field is additive and needs no bump.
const int kAppProto = 2;

/// The oldest hostd this app works with. Raise it only when the app starts to need something an
/// older hostd lacks. A hostd whose `info` has no `proto` is protocol 1.
const int kMinHostProto = 2;

/// The computer and this app disagree on the protocol: shown as one state with its action,
/// instead of a `bad request` per command (#302).
class ProtocolMismatch {
  const ProtocolMismatch({
    required this.deviceName,
    required this.hostProto,
    required this.appProto,
    required this.hostIsOlder,
  });

  final String deviceName;
  final int hostProto;
  final int appProto;

  /// True: the computer needs `atlas update`. False: this app needs an update.
  final bool hostIsOlder;

  String get title =>
      hostIsOlder ? 'Update Atlas on $deviceName' : 'Update Atlas Mobile';

  String get message => hostIsOlder
      ? 'Run `atlas update` on $deviceName, then try again '
            '(it speaks protocol $hostProto, this app needs $kMinHostProto).'
      : 'Update Atlas Mobile from the store, then try again '
            '($deviceName needs protocol $hostProto, this app speaks $appProto).';

  /// The one line the compact lists (crew) show.
  String get line => hostIsOlder
      ? 'Update Atlas on $deviceName: `atlas update`'
      : 'Update Atlas Mobile';
}

/// Reads the protocol out of the `data` of hostd's `info` reply; null when compatible.
///
/// A missing or non-integer `proto` is protocol 1 (the field came with 2). A host newer than the
/// app is assumed additive unless it says otherwise with `min_client_proto` above [appProto].
ProtocolMismatch? checkHostProto(
  Map<String, dynamic>? info,
  MacDevice device, {
  int appProto = kAppProto,
  int minHostProto = kMinHostProto,
}) {
  final raw = info?['proto'];
  final host = raw is int ? raw : 1;
  if (host < minHostProto) {
    return ProtocolMismatch(
      deviceName: device.name,
      hostProto: host,
      appProto: appProto,
      hostIsOlder: true,
    );
  }
  final minClient = info?['min_client_proto'];
  if (minClient is int && minClient > appProto) {
    return ProtocolMismatch(
      deviceName: device.name,
      hostProto: host,
      appProto: appProto,
      hostIsOlder: false,
    );
  }
  return null;
}
