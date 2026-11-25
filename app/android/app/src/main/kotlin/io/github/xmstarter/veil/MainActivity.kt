package io.github.xmstarter.veil

import android.Manifest
import android.app.ActivityManager
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
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
 * handing an address to another app, reading a code with the camera,
 * emptying the clipboard, keeping the app running while a wallet is open
 * (`WalletService`), and opening the wallet for a fingerprint or a face
 * (`Biometrics`). The Dart side of it is `lib/core/device.dart`.
 */
class MainActivity : FlutterActivity() {
    /** The answer a scan under way is owed. */
    private var scanning: MethodChannel.Result? = null

    /** The wallet opened for a fingerprint or a face; see the class. */
    private val biometrics by lazy { Biometrics(this) }

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

                    // Where Android unpacked the native parts of the
                    // package: the one folder whose files an app may
                    // start. The transport program for bridges is there.
                    "libraryDir" -> result.success(applicationInfo.nativeLibraryDir)

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

                    // A wallet is open, with what to show about it, or
                    // none is (no arguments). While one is open the app is
                    // kept running outside the screen.
                    "running" -> {
                        val what = (call.arguments as? Map<*, *>)?.let {
                            Presence(
                                kind = it["kind"] as? String ?: "",
                                title = it["title"] as? String ?: "",
                                text = it["text"] as? String ?: "",
                                busy = it["busy"] == true,
                            )
                        }
                        if (what != null) askForNotices()
                        result.success(WalletService.want(this, what))
                    }

                    // Whether the user, or the phone's maker in the user's
                    // name, has set Android to stop this app outside the
                    // screen. Android before 9 cannot say.
                    "restricted" -> {
                        val manager =
                            getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
                        result.success(
                            Build.VERSION.SDK_INT >= Build.VERSION_CODES.P &&
                                manager.isBackgroundRestricted,
                        )
                    }

                    // Whether the phone can open the wallet for a
                    // fingerprint or a face, and does.
                    "biometrics" -> result.success(biometrics.status())

                    // The wallet's password goes into the phone's keeping,
                    // for a fingerprint or a face, and comes back for one.
                    "keepPassword" -> biometrics.keep(
                        call.argument<String>("password") ?: "",
                        call.argument<String>("title") ?: "",
                        call.argument<String>("cancel") ?: "",
                    ) { result.success(it) }

                    "recallPassword" -> biometrics.recall(
                        call.argument<String>("title") ?: "",
                        call.argument<String>("cancel") ?: "",
                    ) { result.success(it) }

                    "forgetPassword" -> {
                        biometrics.forget()
                        result.success(null)
                    }

                    else -> result.notImplemented()
                }
            }
    }

    /**
     * From Android 13 on an app shows notices only if the user lets it.
     * Asked once, the first time a wallet is open, for the one notice the
     * app has: that a wallet is open. Refused, the app is kept running
     * all the same, and Android lists it among the apps that are active.
     */
    private fun askForNotices() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        val permission = Manifest.permission.POST_NOTIFICATIONS
        if (checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED) return
        val kept = getPreferences(Context.MODE_PRIVATE)
        if (kept.getBoolean(ASKED_FOR_NOTICES, false)) return
        kept.edit().putBoolean(ASKED_FOR_NOTICES, true).apply()
        requestPermissions(arrayOf(permission), NOTICES)
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        // A notice put up before the user said yes was not shown.
        if (requestCode == NOTICES) WalletService.again()
    }

    /**
     * Going back from the first screen puts the app behind the others,
     * as the home button does. Flutter would end the screen here, and the
     * wallet with it. The Dart side sees to it that Back always comes to
     * the app (`onNavigationNotification` in `lib/app.dart`), and this
     * is where it ends up when there is no screen to go back to.
     */
    override fun popSystemNavigator(): Boolean {
        moveTaskToBack(true)
        return true
    }

    /**
     * This screen is never ended from inside the app, whoever asks:
     * ending it would take an open wallet along. When Android deals with
     * Back by itself it ends the screen of an app that was not started
     * from the home screen, and that request arrives here. The app taken
     * off the list of recent apps does not come this way.
     */
    override fun finish() {
        moveTaskToBack(true)
    }

    override fun onDestroy() {
        super.onDestroy()
        // With the screen goes the Dart side. A wallet that is open then
        // has nobody left to talk to it; see `endWithoutScreen`.
        WalletService.endWithoutScreen(this)
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
        const val NOTICES = 0x5caa
        const val ASKED_FOR_NOTICES = "asked_for_notices"
    }
}
