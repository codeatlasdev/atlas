/// The "is it you?" check (Face ID, fingerprint, device passcode) before something that cannot be taken
/// back leaves the phone: typing into a Mac's terminal, handing a crewmate a secret. One seam so tests run
/// the real gate with an authenticator that says yes or no, never the plugin.
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:local_auth/local_auth.dart';

/// Asks the person to prove it is them; `false` = they did not (or could not). Throws when the device
/// has no way to ask (no passcode set): the caller says so instead of sending anyway.
typedef DeviceAuth = Future<bool> Function(String reason);

Future<bool> _platformAuth(String reason) =>
    LocalAuthentication().authenticate(localizedReason: reason);

final deviceAuthProvider = Provider<DeviceAuth>((_) => _platformAuth);
