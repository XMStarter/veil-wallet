package io.github.xmstarter.veil

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.system.Os
import android.view.WindowManager
import com.journeyapps.barcodescanner.ScanIntentResult
import com.journeyapps.barcodescanner.ScanOptions
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

/**
 * The one screen Android starts. Everything the user sees is drawn by
 * Flutter; this class answers what the Dart side cannot do by itself:
 * where the app may keep its files, keeping a screen out of pictures,
 * handing an address to another app, reading a code with the camera, and
 * emptying the clipboard. The Dart side of it is `lib/core/device.dart`.
 */
class MainActivity : FlutterActivity() {
    /** The answer a scan under way is owed. */
    private var scanning: MethodChannel.Result? = null

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

                    // While a recovery phrase is on the screen the system
                    // takes no picture of the window: no screenshot, no
                    // recording, no preview among the recent apps.
                    "secure" -> {
                        val flag = WindowManager.LayoutParams.FLAG_SECURE
                        if (call.arguments == true) {
                            window.addFlags(flag)
                        } else {
                            window.clearFlags(flag)
                        }
                        result.success(null)
                    }

                    // The address goes to whichever app the user picks
                    // from the system's own list.
                    "share" -> {
                        val send = Intent(Intent.ACTION_SEND)
                            .setType("text/plain")
                            .putExtra(Intent.EXTRA_TEXT, call.arguments as String)
                        startActivity(Intent.createChooser(send, null))
                        result.success(null)
                    }

                    "clearClipboard" -> {
                        val clipboard =
                            getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                            clipboard.clearPrimaryClip()
                        } else {
                            clipboard.setPrimaryClip(ClipData.newPlainText("", ""))
                        }
                        result.success(null)
                    }

                    // A code read with the camera, on this device and by
                    // nothing else: the picture goes nowhere. The scanning
                    // screen asks for the camera itself the first time.
                    "scan" -> {
                        scanning?.success(null)
                        scanning = result
                        val options = ScanOptions()
                            .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                            .setPrompt(call.arguments as? String ?: "")
                            .setBeepEnabled(false)
                            .setOrientationLocked(true)
                        @Suppress("DEPRECATION")
                        startActivityForResult(options.createScanIntent(this), SCAN)
                    }

                    else -> result.notImplemented()
                }
            }
    }

    @Deprecated("Deprecated in Java")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        if (requestCode != SCAN) {
            @Suppress("DEPRECATION")
            super.onActivityResult(requestCode, resultCode, data)
            return
        }
        // Null when the user went back or did not allow the camera.
        scanning?.success(ScanIntentResult.parseActivityResult(resultCode, data).contents)
        scanning = null
    }

    private companion object {
        const val SCAN = 0x5ca9
    }
}
