package com.omni_me.app

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.content.FileProvider
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
        // Force-enable Chrome DevTools remote inspection regardless of build
        // profile. Tauri only sets this for --debug builds, but we're forced
        // to use --release for Android (debug bakes in dev URL via
        // cfg!(debug_assertions)). Process-wide flag covering every WebView.
        WebView.setWebContentsDebuggingEnabled(true)
        super.onCreate(savedInstanceState)
        intent?.let { handleSendIntent(it) }
        // Bridge Android's system-bar insets into CSS custom properties so
        // the frontend can `padding-bottom: var(--safe-area-inset-bottom)`.
        // Implementation lives in `InsetBridge.kt` (same override pattern as
        // this file — see `build.rs::apply_android_overrides`).
        InsetBridge.install(this)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleSendIntent(intent)
    }

    // --- OTA install bridge (app-delivery) ---
    //
    // The in-app updater downloads + sha256-verifies a new APK (Rust
    // `download_android_update`) into the cache dir, then `request_android_install`
    // writes the APK path to a `filesDir/install_request` side-file. We poll that
    // file while foregrounded and, when it appears, launch the system package
    // installer for it. Side-file (not a JS interface) keeps the Kotlin↔Rust
    // contract dirt-simple and dodges the "injected object needs a page reload"
    // trap — same rationale as the share handler above.
    //
    // Installing OVER the existing app requires the new APK be signed with the
    // SAME release key (the keystore CI signs with); a debug-keystore build is
    // rejected by the installer. Android prompts once for "install unknown apps".

    private val installHandler = Handler(Looper.getMainLooper())
    private val installPoll = object : Runnable {
        override fun run() {
            checkInstallRequest()
            installHandler.postDelayed(this, INSTALL_POLL_MS)
        }
    }

    override fun onResume() {
        super.onResume()
        installHandler.removeCallbacks(installPoll)
        installHandler.postDelayed(installPoll, INSTALL_POLL_MS)
    }

    override fun onPause() {
        super.onPause()
        installHandler.removeCallbacks(installPoll)
    }

    private fun checkInstallRequest() {
        val req = File(filesDir, INSTALL_REQUEST_FILE)
        if (!req.exists()) return
        val apkPath = try {
            req.readText().trim()
        } catch (e: Exception) {
            req.delete()
            return
        }
        req.delete()
        if (apkPath.isEmpty()) return
        try {
            val uri: Uri =
                FileProvider.getUriForFile(this, "$packageName.fileprovider", File(apkPath))
            val intent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, "application/vnd.android.package-archive")
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            startActivity(intent)
        } catch (e: Exception) {
            android.util.Log.w("OmniMe", "install intent failed for $apkPath", e)
        }
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
        // Must match `INSTALL_REQUEST_FILE` in commands/update.rs.
        const val INSTALL_REQUEST_FILE = "install_request"
        const val INSTALL_POLL_MS = 1500L
    }
}
