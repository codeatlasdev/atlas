import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:package_info_plus/package_info_plus.dart';

/// This app's version as the stores show it, `1.2.0 (34)` (name and build number); a development build
/// (bundle id ending in `.dev`) says so. Null when the platform cannot tell (tests without a mock).
final appVersionProvider = FutureProvider<String?>((ref) async {
  try {
    final p = await PackageInfo.fromPlatform();
    final build = p.buildNumber.isEmpty ? '' : ' (${p.buildNumber})';
    final dev = p.packageName.endsWith('.dev') ? ' · development build' : '';
    return '${p.version}$build$dev';
  } on Object {
    return null;
  }
});
