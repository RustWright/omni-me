package com.omni_me.app

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
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
        // cfg!(debug_assertions)). This flag is process-wide so it covers
        // every WebView the app creates.
        WebView.setWebContentsDebuggingEnabled(true)
        super.onCreate(savedInstanceState)
        intent?.let { handleSendIntent(it) }
        installInsetBridge()
    }

    /*
     * Bridge Android's actual system-bar insets into the WebView as CSS
     * custom properties. Android Chromium WebView auto-forwards display
     * cutouts and the top status bar to env(safe-area-inset-*) but does NOT
     * forward the bottom navigation/gesture bar — a long-standing limitation.
     *
     * The fix: listen for WindowInsets on the content root, find the WebView,
     * inject JS that sets --safe-area-inset-{top,bottom,left,right} on
     * document.documentElement. The frontend CSS reads these via
     * var(--safe-area-inset-bottom, env(safe-area-inset-bottom, 0px)) so
     * non-Android platforms still get the env() value (which works on iOS,
     * desktop browsers).
     *
     * Density conversion: WindowInsets returns px in physical pixels;
     * CSS pixels are logical (density-independent). Divide by density so the
     * CSS value matches what other CSS lengths use.
     */
    // Cached most-recent CSS-pixel inset values, set by the layout listener
    // and re-applied on a schedule (see installInsetBridge).
    private var cachedTopPx = 0
    private var cachedBottomPx = 0
    private var cachedLeftPx = 0
    private var cachedRightPx = 0

    private fun installInsetBridge() {
        val root = findViewById<ViewGroup>(android.R.id.content) ?: return
        ViewCompat.setOnApplyWindowInsetsListener(root) { _, insets ->
            val sysBars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
            val navBars = insets.getInsets(WindowInsetsCompat.Type.navigationBars())
            val tappable = insets.getInsets(WindowInsetsCompat.Type.tappableElement())
            val density = resources.displayMetrics.density
            // Different Android versions / nav modes report the bottom area
            // under different inset types — pick the largest so we get a
            // non-zero value regardless. 3-button nav fills `navigationBars`,
            // gesture nav puts the visual hint area under `tappableElement`.
            val effectiveBottom = maxOf(sysBars.bottom, navBars.bottom, tappable.bottom)
            cachedTopPx = (sysBars.top / density).toInt()
            cachedBottomPx = (effectiveBottom / density).toInt()
            cachedLeftPx = (sysBars.left / density).toInt()
            cachedRightPx = (sysBars.right / density).toInt()
            applyInsetsToWebView(root)
            insets
        }
        // The inset listener typically fires only in the first ~150ms, before
        // the real app document exists (Tauri's WebView swaps documents during
        // early load and the 54MB WASM bundle parse takes 5-10s). Re-apply on
        // a schedule so a later mutation lands on the loaded document.
        val handler = Handler(Looper.getMainLooper())
        for (delayMs in listOf(500L, 1500L, 3000L, 6000L, 10000L)) {
            handler.postDelayed({ applyInsetsToWebView(root) }, delayMs)
        }
    }

    private fun applyInsetsToWebView(root: ViewGroup) {
        val webView = findWebView(root) ?: return
        webView.evaluateJavascript(
            """(function(){var r=document.documentElement.style;""" +
                """r.setProperty('--safe-area-inset-top','${cachedTopPx}px');""" +
                """r.setProperty('--safe-area-inset-bottom','${cachedBottomPx}px');""" +
                """r.setProperty('--safe-area-inset-left','${cachedLeftPx}px');""" +
                """r.setProperty('--safe-area-inset-right','${cachedRightPx}px');})();""",
            null,
        )
    }

    private fun findWebView(view: View): WebView? {
        if (view is WebView) return view
        if (view is ViewGroup) {
            for (i in 0 until view.childCount) {
                findWebView(view.getChildAt(i))?.let { return it }
            }
        }
        return null
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
