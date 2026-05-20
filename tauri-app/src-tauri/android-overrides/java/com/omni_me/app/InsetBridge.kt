package com.omni_me.app

import android.app.Activity
import android.os.Handler
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

/*
 * Android Chromium WebView auto-forwards display cutouts and the top status
 * bar to env(safe-area-inset-*) but NOT the bottom navigation/gesture bar —
 * a long-standing Chromium limitation. Without this bridge, anything
 * `fixed; bottom: 0` (the BottomNav, future commit buttons, etc.) sits
 * behind the system nav bar.
 *
 * This file lives outside the `generated/` subdir, so Tauri's android-init
 * template generation will never touch it. `MainActivity.kt` calls `install`
 * once during onCreate — if MainActivity ever gets regenerated, restoring
 * the bridge is a one-line edit.
 *
 * See `project_android_safe_area_inset_bridge.md` in user memory for the
 * full design rationale (timing race, multi-inset-type sampling, density
 * conversion).
 */
object InsetBridge {
    // Cached most-recent CSS-pixel inset values, set by the layout listener
    // and re-applied on the postDelayed schedule below.
    private var cachedTopPx = 0
    private var cachedBottomPx = 0
    private var cachedLeftPx = 0
    private var cachedRightPx = 0

    fun install(activity: Activity) {
        val root = activity.findViewById<ViewGroup>(android.R.id.content) ?: return
        val density = activity.resources.displayMetrics.density

        ViewCompat.setOnApplyWindowInsetsListener(root) { _, insets ->
            val sysBars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
            val navBars = insets.getInsets(WindowInsetsCompat.Type.navigationBars())
            val tappable = insets.getInsets(WindowInsetsCompat.Type.tappableElement())
            // Different Android versions / nav modes report the bottom area
            // under different inset types — pick the largest so we get a
            // non-zero value regardless. 3-button nav fills `navigationBars`,
            // gesture nav puts the visual hint area under `tappableElement`.
            val effectiveBottom = maxOf(sysBars.bottom, navBars.bottom, tappable.bottom)
            cachedTopPx = (sysBars.top / density).toInt()
            cachedBottomPx = (effectiveBottom / density).toInt()
            cachedLeftPx = (sysBars.left / density).toInt()
            cachedRightPx = (sysBars.right / density).toInt()
            applyToWebView(root)
            insets
        }

        // The inset listener typically fires only in the first ~150ms, before
        // the real app document exists (Tauri's WebView swaps documents during
        // early load and the 54MB WASM bundle parse takes 5-10s). Re-apply on
        // a schedule so a later mutation lands on the loaded document.
        val handler = Handler(Looper.getMainLooper())
        for (delayMs in listOf(500L, 1500L, 3000L, 6000L, 10000L)) {
            handler.postDelayed({ applyToWebView(root) }, delayMs)
        }
    }

    private fun applyToWebView(root: ViewGroup) {
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
}
