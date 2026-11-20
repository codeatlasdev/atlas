/// The app's translated text: `context.l10n.some_key`, from the one catalog in `i18n/messages` (the `mobile`
/// surface; `scripts/mobile-check.sh` writes the ARB files and `flutter pub get` the class). English and
/// Brazilian Portuguese; the phone's language picks.
library;

import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/l10n/app_localizations.dart';

export 'package:atlas_mobile/l10n/app_localizations.dart';

/// What `MaterialApp` needs: the app's messages plus the Material widgets' own (back tooltip, dates).
const List<LocalizationsDelegate<dynamic>> atlasLocalizationsDelegates = [
  AppLocalizations.delegate,
  ...GlobalMaterialLocalizations.delegates,
];

final _english = lookupAppLocalizations(const Locale('en'));

extension AtlasL10n on BuildContext {
  /// The app's messages for this context's language. A subtree outside the app's `MaterialApp` (a widget test
  /// or a preview that has no delegates) speaks English instead of throwing.
  AppLocalizations get l10n =>
      Localizations.of<AppLocalizations>(this, AppLocalizations) ?? _english;
}
