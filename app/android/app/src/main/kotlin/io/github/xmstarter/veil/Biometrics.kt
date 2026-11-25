package io.github.xmstarter.veil

import android.app.Activity
import android.content.Context
import android.hardware.biometrics.BiometricManager
import android.hardware.biometrics.BiometricPrompt
import android.os.Build
import android.os.CancellationSignal
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyInfo
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.GeneralSecurityException
import java.security.KeyStore
import java.security.UnrecoverableKeyException
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.SecretKeyFactory
import javax.crypto.spec.GCMParameterSpec

/**
 * Opens the wallet for a fingerprint or a face instead of the password.
 *
 * The wallet itself knows only its password. So the phone keeps the
 * password: encrypted, in the app's private storage, with a key that
 * Android makes and holds in the phone's secure hardware. The key never
 * leaves that hardware, and it works for one operation, right after the
 * phone has recognised a fingerprint or a face that it counts as strong.
 * When the fingerprints or faces on the phone change, Android destroys
 * the key, and what was kept can be read by nobody.
 *
 * Nothing here is cryptography of the app's own: the key, the cipher and
 * the recognition are Android's (`AndroidKeyStore`, `BiometricPrompt`),
 * called as they are. Android 11 or later, where all of it can be asked
 * for in one way; an older phone opens the wallet with the password.
 *
 * The Dart side is `Device.biometrics` and its neighbours in
 * `lib/core/device.dart`. Everything answers with a map whose `outcome`
 * is `done`, `cancelled`, `changed` (the key is gone or no longer valid)
 * or `failed`, with a `detail`.
 */
class Biometrics(private val activity: Activity) {
    private val kept
        get() = activity.getSharedPreferences(KEPT, Context.MODE_PRIVATE)

    /** `absent`, `not_set_up`, `off` or `on`. */
    fun status(): String {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return "absent"
        val manager = activity.getSystemService(BiometricManager::class.java)
            ?: return "absent"
        return when (manager.canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_STRONG)) {
            BiometricManager.BIOMETRIC_SUCCESS ->
                if (sealed() != null && key() != null) "on" else "off"
            BiometricManager.BIOMETRIC_ERROR_NONE_ENROLLED -> "not_set_up"
            else -> "absent"
        }
    }

    /**
     * Makes a new key, asks for a fingerprint or a face, and keeps
     * [password] encrypted with the key. Whatever was kept before is gone
     * either way.
     */
    fun keep(password: String, title: String, cancel: String, answer: (Map<String, String>) -> Unit) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
            return answer(failed("this needs Android 11 or later"))
        }
        forget()
        val cipher = try {
            val key = newKey()
            if (!inSecureHardware(key)) {
                forget()
                return answer(failed("this phone has no secure hardware to hold the key"))
            }
            Cipher.getInstance(CIPHER).apply { init(Cipher.ENCRYPT_MODE, key) }
        } catch (refused: GeneralSecurityException) {
            forget()
            return answer(failed(refused))
        }
        ask(title, cancel, cipher, { forget(); answer(it) }) { recognised ->
            val plain = password.toByteArray(Charsets.UTF_8)
            try {
                val sealed = recognised.doFinal(plain)
                kept.edit()
                    .putString(IV, Base64.encodeToString(recognised.iv, Base64.NO_WRAP))
                    .putString(SEALED, Base64.encodeToString(sealed, Base64.NO_WRAP))
                    .apply()
                answer(mapOf("outcome" to "done"))
            } finally {
                plain.fill(0)
            }
        }
    }

    /** Asks for a fingerprint or a face and gives the kept password back. */
    fun recall(title: String, cancel: String, answer: (Map<String, String>) -> Unit) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
            return answer(failed("this needs Android 11 or later"))
        }
        val (iv, sealed) = sealed() ?: return answer(changed())
        val cipher = try {
            val key = key() ?: return answer(changed())
            Cipher.getInstance(CIPHER).apply {
                init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(TAG_BITS, iv))
            }
        } catch (gone: KeyPermanentlyInvalidatedException) {
            // A fingerprint or a face was added, or all were removed.
            return answer(changed())
        } catch (gone: UnrecoverableKeyException) {
            return answer(changed())
        } catch (refused: GeneralSecurityException) {
            return answer(failed(refused))
        }
        ask(title, cancel, cipher, answer) { recognised ->
            val plain = recognised.doFinal(sealed)
            try {
                answer(mapOf("outcome" to "done", "password" to String(plain, Charsets.UTF_8)))
            } finally {
                plain.fill(0)
            }
        }
    }

    /** Destroys the key and what was kept with it. */
    fun forget() {
        kept.edit().clear().apply()
        try {
            store().deleteEntry(KEY)
        } catch (ignored: GeneralSecurityException) {
            // No key, then, which is what was wanted.
        }
    }

    private fun changed(): Map<String, String> {
        forget()
        return mapOf("outcome" to "changed")
    }

    private fun failed(why: Any): Map<String, String> = mapOf(
        "outcome" to "failed",
        "detail" to ((why as? Throwable)?.let { it.message ?: it.javaClass.simpleName } ?: "$why"),
    )

    /**
     * Shows Android's own question for a fingerprint or a face and, once
     * the phone has recognised one, hands the cipher it has unlocked to
     * [then]. Everything else is told to [answer], once.
     */
    private fun ask(
        title: String,
        cancel: String,
        cipher: Cipher,
        answer: (Map<String, String>) -> Unit,
        then: (Cipher) -> Unit,
    ) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return
        var answered = false
        val once = { what: Map<String, String> ->
            if (!answered) {
                answered = true
                answer(what)
            }
        }
        val cancelled = mapOf("outcome" to "cancelled")
        val prompt = BiometricPrompt.Builder(activity)
            .setTitle(title)
            .setNegativeButton(cancel, activity.mainExecutor) { _, _ -> once(cancelled) }
            .setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_STRONG)
            .build()
        prompt.authenticate(
            BiometricPrompt.CryptoObject(cipher),
            CancellationSignal(),
            activity.mainExecutor,
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    val recognised = result.cryptoObject?.cipher
                        ?: return once(failed("the phone gave no key"))
                    try {
                        then(recognised)
                    } catch (refused: GeneralSecurityException) {
                        once(failed(refused))
                    }
                }

                // A finger or a face that is not known leaves the question
                // on the screen; only its end comes here. The words are
                // Android's, in the phone's language.
                override fun onAuthenticationError(code: Int, message: CharSequence) {
                    once(
                        when (code) {
                            BiometricPrompt.BIOMETRIC_ERROR_USER_CANCELED,
                            BiometricPrompt.BIOMETRIC_ERROR_CANCELED,
                            -> cancelled
                            else -> failed(message)
                        },
                    )
                }
            },
        )
    }

    private fun store(): KeyStore = KeyStore.getInstance(STORE).apply { load(null) }

    private fun key(): SecretKey? = try {
        store().getKey(KEY, null) as? SecretKey
    } catch (gone: GeneralSecurityException) {
        null
    }

    private fun sealed(): Pair<ByteArray, ByteArray>? {
        val iv = kept.getString(IV, null) ?: return null
        val sealed = kept.getString(SEALED, null) ?: return null
        return try {
            Base64.decode(iv, Base64.NO_WRAP) to Base64.decode(sealed, Base64.NO_WRAP)
        } catch (damaged: IllegalArgumentException) {
            null
        }
    }

    /**
     * A key for AES in Galois/Counter mode that Android keeps, that
     * cannot be taken out of it, that works only for a fingerprint or a
     * face recognised for that one use, and that Android destroys when
     * the fingerprints or faces on the phone change.
     */
    private fun newKey(): SecretKey {
        check(Build.VERSION.SDK_INT >= Build.VERSION_CODES.R)
        val spec = KeyGenParameterSpec.Builder(
            KEY,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .setUserAuthenticationRequired(true)
            .setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG)
            .setInvalidatedByBiometricEnrollment(true)
            .build()
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, STORE).run {
            init(spec)
            generateKey()
        }
    }

    /** Whether Android holds [key] in the phone's secure hardware. */
    private fun inSecureHardware(key: SecretKey): Boolean {
        val info = SecretKeyFactory.getInstance(key.algorithm, STORE)
            .getKeySpec(key, KeyInfo::class.java) as KeyInfo
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            info.securityLevel == KeyProperties.SECURITY_LEVEL_TRUSTED_ENVIRONMENT ||
                info.securityLevel == KeyProperties.SECURITY_LEVEL_STRONGBOX
        } else {
            @Suppress("DEPRECATION")
            info.isInsideSecureHardware
        }
    }

    private companion object {
        const val STORE = "AndroidKeyStore"
        const val KEY = "veil.wallet.password"
        const val CIPHER = "AES/GCM/NoPadding"
        const val TAG_BITS = 128
        const val KEPT = "biometrics"
        const val IV = "iv"
        const val SEALED = "sealed"
    }
}
