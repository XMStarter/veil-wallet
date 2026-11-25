package io.github.xmstarter.veil

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.os.Process

/**
 * What the app shows while a wallet is open: the words of its notice, and
 * whether the wallet is at work on something a sleeping phone would cut
 * off. The Dart side decides all of it (`Presence` in
 * `lib/core/device.dart`); nothing of the wallet's contents is in it.
 */
data class Presence(
    val kind: String,
    val title: String,
    val text: String,
    val busy: Boolean,
)

/**
 * Keeps the app running while a wallet is open and the app is not in
 * front. Android stops an app it cannot see within minutes; a service
 * "in the foreground", which is one with a notice the user can see, is
 * left to run. The wallet itself is not here: it lives in the core, which
 * the Dart side drives, in the same process. This class only holds that
 * process up, and the phone awake while the wallet is busy.
 *
 * It is started when a wallet is opened, with the app in front, which is
 * the only moment Android lets such a service start, and stops when the
 * wallet is closed. It never starts by itself and is not brought back by
 * the system: without the Dart side there would be no wallet to keep.
 */
class WalletService : Service() {
    private var awake: PowerManager.WakeLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        running = this
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // Whoever asked Android for a service in the foreground owes it
        // the notice within seconds, wanted or not: the wallet may have
        // been closed again in the moment between the asking and this.
        val what = wanted ?: shown ?: Presence("Veil", "Veil", "", false)
        try {
            val notice = notice(what)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                startForeground(NOTICE, notice, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
            } else {
                startForeground(NOTICE, notice)
            }
        } catch (refused: RuntimeException) {
            // Android does not let the app run outside the screen: the
            // user has restricted it, or its hours for today are used up.
            // The wallet works on for as long as the app is in front.
            stopSelf()
            return START_NOT_STICKY
        }
        if (wanted == null) {
            stop()
        } else {
            shown = what
            stayAwake(what.busy)
        }
        return START_NOT_STICKY
    }

    /** Shows what is [wanted] now, on a service that is already up. */
    fun show() {
        val what = wanted ?: return
        shown = what
        (getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager)
            .notify(NOTICE, notice(what))
        stayAwake(what.busy)
    }

    /** The wallet is closed: the notice goes and the service ends. */
    fun stop() {
        stayAwake(false)
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    /**
     * Android gives a service of this kind six hours a day outside the
     * screen (from Android 15 on) and says here that they are over. The
     * service has to end within seconds. The wallet stays open; Android
     * may now stop the app like any other, and the service is asked for
     * again when the app is next in front, which also gives it new hours.
     */
    override fun onTimeout(startId: Int, fgsType: Int) {
        stop()
    }

    /**
     * The user has taken the app off the list of recent apps. Its screen
     * is gone and with it the Dart side, the only one that can talk to
     * the wallet, so the wallet cannot be kept and cannot be closed in
     * order either. The process ends, as it would for any app without
     * this service.
     */
    override fun onTaskRemoved(rootIntent: Intent?) {
        endWithoutScreen(this)
    }

    override fun onDestroy() {
        stayAwake(false)
        if (running === this) running = null
        super.onDestroy()
    }

    /**
     * Keeps the processor running with the screen off while the wallet is
     * busy, and lets the phone sleep when it is not. Each telling holds
     * for [AWAKE_FOR] and no longer; the Dart side says "still busy" every
     * minute, so a wallet that has stopped reporting lets the phone sleep.
     */
    private fun stayAwake(busy: Boolean) {
        if (busy) {
            val lock = awake
                ?: (getSystemService(Context.POWER_SERVICE) as PowerManager)
                    .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "veil:wallet")
                    .also {
                        // One holder: a new telling replaces the one before
                        // and its time, it does not add to it.
                        it.setReferenceCounted(false)
                        awake = it
                    }
            lock.acquire(AWAKE_FOR)
        } else {
            awake?.let { if (it.isHeld) it.release() }
        }
    }

    private fun notice(what: Presence): Notification {
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            // Quiet: no sound, no badge, and nothing on a locked screen,
            // where it would tell whoever picks the phone up that a
            // wallet is open on it. Made again each time, which is how
            // its name follows the app's language.
            val channel = NotificationChannel(
                CHANNEL,
                what.kind,
                NotificationManager.IMPORTANCE_LOW,
            )
            channel.setShowBadge(false)
            channel.lockscreenVisibility = Notification.VISIBILITY_SECRET
            (getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager)
                .createNotificationChannel(channel)
            Notification.Builder(this, CHANNEL)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this).setPriority(Notification.PRIORITY_LOW)
        }
        // Touching the notice brings the app to the front as it is.
        val open = PendingIntent.getActivity(
            this,
            0,
            packageManager.getLaunchIntentForPackage(packageName)
                ?: Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        builder
            .setSmallIcon(R.drawable.ic_notice)
            .setContentTitle(what.title)
            .setContentText(what.text)
            .setContentIntent(open)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setShowWhen(false)
            .setCategory(Notification.CATEGORY_SERVICE)
            .setVisibility(Notification.VISIBILITY_SECRET)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            // Shown at once, not after the ten seconds Android would wait.
            builder.setForegroundServiceBehavior(Notification.FOREGROUND_SERVICE_IMMEDIATE)
        }
        return builder.build()
    }

    companion object {
        private const val NOTICE = 1
        private const val CHANNEL = "wallet"

        /**
         * How long one telling that the wallet is busy keeps the phone
         * awake. Longer than the Dart side waits before it says so again
         * (`Device.busyAgainAfter`), with room for a state that does not
         * change for a while, such as waiting for a node that is slow.
         */
        private const val AWAKE_FOR = 10 * 60 * 1000L

        /** What the Dart side last asked to be shown; null for no wallet. */
        var wanted: Presence? = null
            private set

        /** What was last put on the screen. */
        private var shown: Presence? = null

        /** The service, while Android has it up. */
        private var running: WalletService? = null

        /**
         * A wallet is open and [what] is to be shown, or none is (null).
         * Called on the main thread, like everything of this class.
         * Returns whether the app is kept running as asked.
         */
        fun want(context: Context, what: Presence?): Boolean {
            wanted = what
            val service = running
            if (what == null) {
                // A service Android has not brought up yet ends itself
                // when it comes up and finds nothing wanted.
                service?.stop()
                return true
            }
            if (service != null) {
                service.show()
                return true
            }
            val start = Intent(context, WalletService::class.java)
            return try {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                    context.startForegroundService(start)
                } else {
                    context.startService(start)
                }
                true
            } catch (refused: RuntimeException) {
                // Not allowed from where the app is now: it is not in
                // front, or the user has restricted it.
                false
            }
        }

        /** Whether the notice is up, for it to be put up again. */
        fun again() {
            running?.show()
        }

        /**
         * The app's screen is gone for good while a wallet is open. The
         * wallet is in this process with nobody left to talk to it; it
         * would go on holding its files and running Tor unseen, and the
         * next start of the app would find the wallet "open elsewhere".
         * Ending the process is what Android does to an app that is taken
         * off the list of recent apps, and what the wallet is built to
         * survive: it is as it was last saved, which is every two minutes
         * of a scan, after every synchronisation, and at once for a
         * payment whose outcome is unknown.
         */
        fun endWithoutScreen(context: Context) {
            if (wanted == null) return
            running?.stop()
            Process.killProcess(Process.myPid())
        }
    }
}
