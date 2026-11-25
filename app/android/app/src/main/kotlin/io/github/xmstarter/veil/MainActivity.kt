package io.github.xmstarter.veil

import android.os.Bundle
import android.system.Os
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

/**
 * The one screen Android starts. Everything the user sees is drawn by
 * Flutter; this class only answers what the Dart side cannot find out by
 * itself: where on this device the app may keep its files.
 */
class MainActivity : FlutterActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        // Code that is not written for Android looks for a place for
        // temporary files in TMPDIR and, where that is not set, in folders
        // an app may not write to. The app's own cache folder is such a
        // place, and private like the rest of its storage.
        Os.setenv("TMPDIR", cacheDir.absolutePath, true)
        super.onCreate(savedInstanceState)
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "veil/device")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    // The app's private storage: no other app can read it,
                    // and it goes when the app is removed.
                    "filesDir" -> result.success(filesDir.absolutePath)
                    else -> result.notImplemented()
                }
            }
    }
}
