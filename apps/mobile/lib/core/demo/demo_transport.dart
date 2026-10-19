import 'package:url_launcher/url_launcher.dart';

/// The privacy policy (PRIVACY.md, its "Atlas Mobile" section).
const privacyUrl =
    'https://github.com/codeatlasdev/atlas/blob/main/PRIVACY.md#atlas-mobile';

Future<void> openPrivacy() =>
    launchUrl(Uri.parse(privacyUrl), mode: LaunchMode.externalApplication);
