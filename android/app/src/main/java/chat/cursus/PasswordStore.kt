package chat.cursus

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * "Remember password" on this phone: the database password encrypted with a
 * key that never leaves the Android Keystore (the desktop apps use the
 * system keyring / Keychain the same way).
 */
class PasswordStore(context: Context) {
    private val prefs = context.getSharedPreferences("cursus.lock", Context.MODE_PRIVATE)

    fun load(): String? = runCatching {
        val blob = prefs.getString(KEY, null) ?: return null
        val (iv, data) = blob.split(':').map { Base64.decode(it, Base64.NO_WRAP) }
        val cipher = Cipher.getInstance(TRANSFORM)
        cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, iv))
        String(cipher.doFinal(data), Charsets.UTF_8)
    }.getOrNull()

    fun save(password: String) {
        runCatching {
            val cipher = Cipher.getInstance(TRANSFORM)
            cipher.init(Cipher.ENCRYPT_MODE, key())
            val data = cipher.doFinal(password.toByteArray(Charsets.UTF_8))
            val blob = Base64.encodeToString(cipher.iv, Base64.NO_WRAP) + ":" +
                Base64.encodeToString(data, Base64.NO_WRAP)
            prefs.edit().putString(KEY, blob).apply()
        }
    }

    fun delete() {
        prefs.edit().remove(KEY).apply()
    }

    private fun key(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build()
        )
        return gen.generateKey()
    }

    private companion object {
        const val ALIAS = "cursus-db-password"
        const val KEY = "password"
        const val TRANSFORM = "AES/GCM/NoPadding"
    }
}
