package br.com.codeatlas.atlas.mobile

import io.flutter.embedding.android.FlutterFragmentActivity
import io.flutter.embedding.engine.FlutterEngine

class MainActivity : FlutterFragmentActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        // Dictation (lib/core/voice/): the phone's own on-device recognizer, see DictationPlugin.
        flutterEngine.plugins.add(DictationPlugin())
    }
}
