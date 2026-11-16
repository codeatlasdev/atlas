import 'package:url_launcher/url_launcher.dart';

/// Every page outside the app the app links to, in one place: the docs site (built from the repo's `docs/`,
/// one page per file, a section per `##` heading), the privacy policy and the source.
abstract final class AtlasLinks {
  static const site = 'https://atlas.codeatlas.com.br';

  /// `docs/<page>.md` on the site; [section] is a heading's anchor (`## Install a crew server` →
  /// `install-a-crew-server`).
  static Uri docs(String page, [String? section]) =>
      Uri.parse('$site/docs/$page/${section == null ? '' : '#$section'}');

  static final privacy = Uri.parse('$site/privacy/');
  static final crewSetup = docs('crew', 'install-a-crew-server');
  static final crewOnPhone = docs('mobile', 'crew-on-the-phone');
  static final pairing = docs('mobile', 'pairing');
  static final pairingBehindVpn = docs(
    'mobile',
    'pairing-fails-with-a-vpn-or-warp-on',
  );
  static final quickstart = docs('quickstart');
  static final agents = docs('agents');
  static final source = Uri.parse('https://github.com/codeatlasdev/atlas');
}

/// Opens [uri] in the browser (never inside the app: these are pages to read and share).
Future<bool> openLink(Uri uri) =>
    launchUrl(uri, mode: LaunchMode.externalApplication);
