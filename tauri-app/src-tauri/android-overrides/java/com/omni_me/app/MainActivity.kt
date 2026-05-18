package com.omni_me.app

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream

/*
 * Phase 3.3: receive shared receipts/statements from other Android apps.
 *
 * Tauri's stock MainActivity just hands `onCreate` to TauriActivity, which
 * starts the WebView. We extend it to also catch SEND intents — when the
 * user shares a file into Omni-Me from Gallery/Drive/Gmail, we stash the
 * bytes plus a small metadata sidecar inside the app's private filesDir.
 * The WASM frontend pulls the pair on mount via the
 * `take_pending_share_intent` Tauri command and routes into the capture
 * flow with the bytes pre-loaded.
 *
 * Why side-files rather than a JNI callback: keeps the Kotlin↔Rust contract
 * dirt-simple (Rust just reads two files) and survives the case where the
 * intent fires before the WebView is ready to receive events.
 */
class MainActivity : TauriActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        intent?.let { handleSendIntent(it) }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleSendIntent(intent)
    }

    private fun handleSendIntent(intent: Intent) {
        if (intent.action != Intent.ACTION_SEND) return
        val uri: Uri = intent.getParcelableExtra(Intent.EXTRA_STREAM) ?: return
        val mime = intent.type ?: contentResolver.getType(uri) ?: "application/octet-stream"

        try {
            val bytesFile = File(filesDir, SHARE_BYTES_FILE)
            contentResolver.openInputStream(uri)?.use { input ->
                FileOutputStream(bytesFile).use { output -> input.copyTo(output) }
            } ?: return

            val filename = queryDisplayName(uri) ?: bytesFile.name
            val meta = JSONObject().apply {
                put("mime", mime)
                put("filename", filename)
                put("size", bytesFile.length())
            }
            File(filesDir, SHARE_META_FILE).writeText(meta.toString())
        } catch (e: Exception) {
            android.util.Log.w("OmniMe", "share intent capture failed", e)
        }
    }

    private fun queryDisplayName(uri: Uri): String? {
        return try {
            contentResolver.query(uri, null, null, null, null)?.use { cursor ->
                val idx = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                if (idx >= 0 && cursor.moveToFirst()) cursor.getString(idx) else null
            }
        } catch (_: Exception) {
            null
        }
    }

    companion object {
        const val SHARE_BYTES_FILE = "share_intent.bin"
        const val SHARE_META_FILE = "share_intent.json"
    }
}
