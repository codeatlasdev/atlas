package br.com.codeatlas.atlas.mobile

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import android.speech.tts.Voice
import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import java.util.Locale

/**
 * Reading replies aloud on Android: `TextToSpeech` with a voice that is installed and does not need the network,
 * so the text of a reply never leaves the phone. The Dart side is `lib/core/voice/platform_speech.dart`, which
 * documents the wire.
 *
 * The voice is the system's default for the language and the rate is the user's own setting (no `setSpeechRate`
 * call). A language with no such voice is reported as unavailable and `speak` fails with `language`; this class
 * never picks a network voice or another language.
 */
class SpeechPlugin : FlutterPlugin, MethodChannel.MethodCallHandler, EventChannel.StreamHandler {
    private lateinit var context: Context
    private lateinit var methods: MethodChannel
    private lateinit var events: EventChannel
    private val main = Handler(Looper.getMainLooper())
    private var sink: EventChannel.EventSink? = null

    private var tts: TextToSpeech? = null

    /** Callers that arrived while the engine was starting. */
    private var waiting = mutableListOf<(TextToSpeech?) -> Unit>()
    private var starting = false

    /** The utterance Dart waits on; a callback of any other (replaced or stopped) is ignored. */
    private var current: String? = null
    private var counter = 0

    override fun onAttachedToEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        context = binding.applicationContext
        methods = MethodChannel(binding.binaryMessenger, CHANNEL).also { it.setMethodCallHandler(this) }
        events = EventChannel(binding.binaryMessenger, "$CHANNEL/events").also { it.setStreamHandler(this) }
    }

    override fun onDetachedFromEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        current = null
        tts?.stop()
        tts?.shutdown()
        tts = null
        methods.setMethodCallHandler(null)
        events.setStreamHandler(null)
    }

    override fun onListen(arguments: Any?, events: EventChannel.EventSink?) {
        sink = events
    }

    override fun onCancel(arguments: Any?) {
        sink = null
    }

    override fun onMethodCall(call: MethodCall, result: MethodChannel.Result) {
        val tag = call.argument<String>("locale") ?: "en-US"
        when (call.method) {
            "available" -> withEngine { engine -> result.success(engine != null && voiceFor(engine, tag) != null) }
            "speak" -> {
                val text = call.argument<String>("text") ?: ""
                withEngine { engine ->
                    val voice = engine?.let { voiceFor(it, tag) }
                    if (engine == null || voice == null) {
                        result.error("language", "no installed voice for $tag", null)
                    } else {
                        speak(engine, voice, text)
                        result.success(null)
                    }
                }
            }
            "stop" -> {
                current = null
                tts?.stop()
                result.success(null)
            }
            else -> result.notImplemented()
        }
    }

    /** Runs [block] with the started engine, or null when the phone has no text-to-speech engine. */
    private fun withEngine(block: (TextToSpeech?) -> Unit) {
        tts?.let { return block(it) }
        waiting.add(block)
        if (starting) return
        starting = true
        var created: TextToSpeech? = null
        created = TextToSpeech(context) { status ->
            main.post {
                starting = false
                val ready = if (status == TextToSpeech.SUCCESS) created else null
                if (ready != null) {
                    ready.setOnUtteranceProgressListener(progress)
                    tts = ready
                }
                val pending = waiting
                waiting = mutableListOf()
                pending.forEach { it(ready) }
            }
        }
    }

    /** An installed voice that works without the network for [tag]: the engine's default one when it qualifies. */
    private fun voiceFor(engine: TextToSpeech, tag: String): Voice? {
        val locale = Locale.forLanguageTag(tag)
        val usable = (engine.voices ?: emptySet()).filter {
            it.locale.language == locale.language &&
                (locale.country.isEmpty() || it.locale.country == locale.country) &&
                !it.isNetworkConnectionRequired &&
                !it.features.contains(TextToSpeech.Engine.KEY_FEATURE_NOT_INSTALLED)
        }
        val default = engine.defaultVoice
        return usable.firstOrNull { it.name == default?.name } ?: usable.firstOrNull()
    }

    private fun speak(engine: TextToSpeech, voice: Voice, text: String) {
        val id = "atlas-${counter++}"
        current = id
        engine.voice = voice
        // Queue flush: a new reply replaces the one being read.
        if (engine.speak(text, TextToSpeech.QUEUE_FLUSH, null, id) != TextToSpeech.SUCCESS) {
            current = null
            sink?.success(mapOf("type" to "error"))
        }
    }

    private val progress = object : UtteranceProgressListener() {
        override fun onStart(utteranceId: String?) {}

        override fun onDone(utteranceId: String?) = finish(utteranceId, "done")

        @Deprecated("Deprecated in Java")
        override fun onError(utteranceId: String?) = finish(utteranceId, "error")

        override fun onError(utteranceId: String?, errorCode: Int) = finish(utteranceId, "error")
    }

    private fun finish(id: String?, type: String) {
        main.post {
            if (id == null || id != current) return@post
            current = null
            sink?.success(mapOf("type" to type))
        }
    }

    companion object {
        private const val CHANNEL = "br.com.codeatlas.atlas/speech"
    }
}
