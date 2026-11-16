import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// The band every strip under the app bar sits on: the base colour and a hairline below.
BoxDecoration stripDecoration(AtlasColors c) => BoxDecoration(
  color: c.base,
  border: Border(bottom: BorderSide(color: c.borderSubtle)),
);
