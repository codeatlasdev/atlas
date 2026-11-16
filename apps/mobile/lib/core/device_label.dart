import 'package:device_info_plus/device_info_plus.dart';
import 'package:flutter/foundation.dart';

/// How a paired computer lists this phone (Atlas Desktop → Settings → Mobile): its model and system, e.g.
/// `iPhone 15 Pro · iOS 26.1` or `Pixel 8 · Android 15`, so two phones never read the same. hostd keeps the
/// first 64 characters. The user can rename it there; this is only the default.
Future<String> deviceLabel([DeviceInfoPlugin? plugin]) async {
  final info = plugin ?? DeviceInfoPlugin();
  try {
    switch (defaultTargetPlatform) {
      case TargetPlatform.iOS:
        final d = await info.iosInfo;
        return phoneLabel(
          model: d.modelName.isNotEmpty ? d.modelName : d.model,
          system: '${d.systemName} ${d.systemVersion}',
        );
      case TargetPlatform.android:
        final d = await info.androidInfo;
        return phoneLabel(
          model: d.model,
          system: 'Android ${d.version.release}',
          maker: d.manufacturer,
        );
      default:
        return fallbackDeviceLabel;
    }
  } on Object {
    return fallbackDeviceLabel;
  }
}

/// What a phone that cannot tell its model is called.
const fallbackDeviceLabel = 'Atlas Mobile';

/// `<model> · <system>`; an Android model that does not start with its maker gets it in front
/// (`SM-S918B` → `Samsung SM-S918B`), and blank parts are left out.
@visibleForTesting
String phoneLabel({
  required String model,
  required String system,
  String? maker,
}) {
  var m = model.trim();
  final mk = maker?.trim() ?? '';
  if (mk.isNotEmpty && !m.toLowerCase().startsWith(mk.toLowerCase())) {
    final cap = mk[0].toUpperCase() + mk.substring(1);
    m = m.isEmpty ? cap : '$cap $m';
  }
  final parts = [m, system.trim()].where((p) => p.isNotEmpty).toList();
  return parts.isEmpty ? fallbackDeviceLabel : parts.join(' · ');
}
