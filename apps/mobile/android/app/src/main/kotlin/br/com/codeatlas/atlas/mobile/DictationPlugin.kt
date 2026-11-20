package br.com.codeatlas.atlas.mobile

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.speech.RecognitionListener
import android.speech.RecognitionSupport
import android.speech.RecognitionSupportCallback
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import androidx.core.content.ContextCompat
import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel

/**
 * Dictation on Android: speech to text that stays on the phone. The Dart side is
 * `lib/core/voice/platform_dictation.dart`, which documents the wire.
 *
 * On-device means `SpeechRecognizer.createOnDeviceSpeechRecognizer` (Android 12+): the recognizer runs in the
 * system's on-device service and cannot reach a server. A phone, or a language, without it is reported as
 * unsupported; this class never falls back to `createSpeechRecognizer` (the default service, which may use
 * the network) unless the app asked for `online`, which is the person's labelled opt-in.
 */
class DictationPlugin : FlutterPlugin, MethodChannel.MethodCallHandler, EventChannel.StreamHandler {
    private lateinit var context: Context
    private lateinit var methods: MethodChannel
    private lateinit var events: EventChannel
    private val main = Handler(Looper.getMainLooper())
    private var sink: EventChannel.EventSink? = null

    /** The running session; null when nothing holds the microphone. */
    private var session: Session? = null

    /** Makes a late callback of a finished session harmless. */
    private var generation = 0

    override fun onAttachedToEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        context = binding.applicationContext
        methods = MethodChannel(binding.binaryMessenger, CHANNEL).also { it.setMethodCallHandler(this) }
        events = EventChannel(binding.binaryMessenger, "$CHANNEL/events").also { it.setStreamHandler(this) }
    }

    override fun onDetachedFromEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        generation++
        session?.cancel()
        session = null
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
            "support" -> support(tag, result)
            "start" -> start(tag, call.argument<Boolean>("online") ?: false, result)
            "stop" -> {
                val gen = generation
                session?.stop { text -> emit(gen, mapOf("type" to "final", "text" to text)) }
                result.success(null)
            }
            "cancel" -> {
                generation++
                session?.cancel()
                session = null
                result.success(null)
            }
            else -> result.notImplemented()
        }
    }

    private fun emit(gen: Int, event: Map<String, Any>) {
        main.post {
            if (gen != generation) return@post
            when (event["type"]) {
                "final" -> session = null
                "error" -> {
                    session?.cancel()
                    session = null
                }
            }
            sink?.success(event)
        }
    }

    // ── support ──────────────────────────────────────────────────────────────────────────────────────────

    private fun recognizerIntent(tag: String, online: Boolean): Intent =
        Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
            putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            putExtra(RecognizerIntent.EXTRA_LANGUAGE, tag)
            putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
            putExtra(RecognizerIntent.EXTRA_PREFER_OFFLINE, !online)
            putExtra(RecognizerIntent.EXTRA_CALLING_PACKAGE, context.packageName)
            if (Build.VERSION.SDK_INT >= 33) putExtra(RecognizerIntent.EXTRA_SEGMENTED_SESSION, true)
        }

    /** What this phone can do for [tag]; never records and never prompts. */
    private fun support(tag: String, result: MethodChannel.Result) {
        val online = SpeechRecognizer.isRecognitionAvailable(context)
        fun answer(onDevice: Boolean, downloading: Boolean) = result.success(
            mapOf(
                "on_device" to if (onDevice) "android" else null,
                "online" to online,
                "speech_permission" to false,
                "downloading" to downloading,
            ),
        )
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S ||
            !SpeechRecognizer.isOnDeviceRecognitionAvailable(context)
        ) {
            return answer(onDevice = false, downloading = false)
        }
        if (Build.VERSION.SDK_INT < 33) {
            // 12 and 12L cannot say which languages are installed: the session reports `language` if not.
            return answer(onDevice = true, downloading = false)
        }
        val recognizer = SpeechRecognizer.createOnDeviceSpeechRecognizer(context)
        recognizer.checkRecognitionSupport(
            recognizerIntent(tag, online = false),
            ContextCompat.getMainExecutor(context),
            object : RecognitionSupportCallback {
                override fun onSupportResult(support: RecognitionSupport) {
                    val installed = support.installedOnDeviceLanguages.any { sameLanguage(it, tag) }
                    val downloadable = !installed &&
                        (support.supportedOnDeviceLanguages + support.pendingOnDeviceLanguages)
                            .any { sameLanguage(it, tag) }
                    // The system's own language pack (a model, never audio): ask it to fetch the pack.
                    if (downloadable) recognizer.triggerModelDownload(recognizerIntent(tag, online = false))
                    recognizer.destroy()
                    answer(onDevice = installed, downloading = downloadable)
                }

                override fun onError(error: Int) {
                    recognizer.destroy()
                    answer(onDevice = false, downloading = false)
                }
            },
        )
    }

    private fun sameLanguage(a: String, b: String) =
        a.replace('_', '-').equals(b, ignoreCase = true) ||
            a.substringBefore('-').equals(b.substringBefore('-'), ignoreCase = true)

    // ── session ──────────────────────────────────────────────────────────────────────────────────────────

    private fun start(tag: String, online: Boolean, result: MethodChannel.Result) {
        if (session != null) return result.error("busy", "a dictation is already running", null)
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            return result.error("permission", "the microphone permission is not granted", null)
        }
        val onDevice = !online
        if (onDevice && (Build.VERSION.SDK_INT < Build.VERSION_CODES.S ||
                !SpeechRecognizer.isOnDeviceRecognitionAvailable(context))
        ) {
            return result.error("language", "this phone has no on-device recognizer", null)
        }
        if (online && !SpeechRecognizer.isRecognitionAvailable(context)) {
            return result.error("failed", "this phone has no speech recognizer", null)
        }
        generation++
        val gen = generation
        val next = Session(
            recognizer = if (onDevice) SpeechRecognizer.createOnDeviceSpeechRecognizer(context)
            else SpeechRecognizer.createSpeechRecognizer(context),
            intent = recognizerIntent(tag, online),
            main = main,
            report = { text -> emit(gen, mapOf("type" to "partial", "text" to text)) },
            fail = { code -> emit(gen, mapOf("type" to "error", "code" to code)) },
            finished = { text -> emit(gen, mapOf("type" to "final", "text" to text)) },
        )
        session = next
        next.begin()
        result.success(null)
    }

    /**
     * One dictation. The recognizer ends a segment at every pause: from Android 13 the segmented session keeps
     * one stream open; before that the session listens again after each result. Both keep what was said
     * ([committed]) and show the segment being heard ([heard]).
     */
    private class Session(
        private val recognizer: SpeechRecognizer,
        private val intent: Intent,
        private val main: Handler,
        private val report: (String) -> Unit,
        private val fail: (String) -> Unit,
        private val finished: (String) -> Unit,
    ) : RecognitionListener {
        private val committed = StringBuilder()
        private var heard = ""
        private var stopping: ((String) -> Unit)? = null
        private var done = false

        private val text: String
            get() = listOf(committed.toString().trim(), heard.trim()).filter { it.isNotEmpty() }.joinToString(" ")

        fun begin() {
            recognizer.setRecognitionListener(this)
            recognizer.startListening(intent)
        }

        fun stop(onFinal: (String) -> Unit) {
            if (done) return onFinal(text)
            stopping = onFinal
            recognizer.stopListening()
            // A recognizer that never answers must not hold the person: what was heard is what there is.
            main.postDelayed({ settle() }, STOP_GRACE_MS)
        }

        fun cancel() {
            done = true
            stopping = null
            main.removeCallbacksAndMessages(null)
            recognizer.cancel()
            recognizer.destroy()
        }

        private fun settle() {
            val callback = stopping ?: return
            stopping = null
            done = true
            val out = text
            recognizer.destroy()
            callback(out)
        }

        private fun commit(segment: String) {
            if (segment.isNotBlank()) {
                if (committed.isNotEmpty()) committed.append(' ')
                committed.append(segment.trim())
            }
            heard = ""
        }

        private fun first(results: Bundle?): String =
            results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull() ?: ""

        override fun onPartialResults(partialResults: Bundle?) {
            heard = first(partialResults)
            report(text)
        }

        // Android 13+: a segment ended; the session stays open until stopListening.
        override fun onSegmentResults(segmentResults: Bundle) {
            commit(first(segmentResults))
            report(text)
        }

        override fun onEndOfSegmentedSession() {
            if (stopping != null) settle() else if (!done) endByItself()
        }

        override fun onResults(results: Bundle?) {
            commit(first(results))
            report(text)
            if (stopping != null) return settle()
            // Before Android 13 a pause ends the recognition: listen again, the words stay.
            if (!done) main.postDelayed({ if (!done) recognizer.startListening(intent) }, RESTART_MS)
        }

        override fun onError(error: Int) {
            if (done) return
            when (error) {
                // Silence or nothing recognised: not a failure while the person is still dictating.
                SpeechRecognizer.ERROR_NO_MATCH, SpeechRecognizer.ERROR_SPEECH_TIMEOUT -> {
                    if (stopping != null) settle()
                    else main.postDelayed({ if (!done) recognizer.startListening(intent) }, RESTART_MS)
                }
                SpeechRecognizer.ERROR_CLIENT -> if (stopping != null) settle()
                SpeechRecognizer.ERROR_RECOGNIZER_BUSY -> fail("busy")
                SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> fail("permission")
                SpeechRecognizer.ERROR_AUDIO -> fail("interrupted")
                SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED, SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE ->
                    fail("language")
                else -> fail("failed")
            }
        }

        /** The engine ended the whole session without being asked (it never does on-device; online may). */
        private fun endByItself() {
            done = true
            val out = text
            recognizer.destroy()
            finished(out)
        }

        override fun onReadyForSpeech(params: Bundle?) {}
        override fun onBeginningOfSpeech() {}
        override fun onRmsChanged(rmsdB: Float) {}
        override fun onBufferReceived(buffer: ByteArray?) {}
        override fun onEndOfSpeech() {}
        override fun onEvent(eventType: Int, params: Bundle?) {}
    }

    companion object {
        const val CHANNEL = "br.com.codeatlas.atlas/dictation"
        private const val STOP_GRACE_MS = 2500L
        private const val RESTART_MS = 150L
    }
}
