package com.omni_me.app

import android.app.Activity
import android.graphics.Rect
import android.os.Build
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
 * positioned against the bottom (the nav drawer's last items, the content
 * scroll area's last row, future commit buttons) sits behind the system
 * nav bar. The same listener also surfaces the IME (keyboard) inset (1.9).
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
    // Soft-keyboard (IME) occlusion height in CSS px (1.9). 0 when the keyboard
    // is hidden. Surfaced separately from the static system-bar insets because
    // it changes dynamically as the keyboard shows/hides, and the web side needs
    // it to keep the caret scrolled above the keyboard (1.10).
    private var cachedKeyboardPx = 0
    // Display density, cached so the gesture-exclusion re-applies (below) can
    // convert dp without re-reading resources.
    private var density = 1f

    // Left-edge strip (dp) reserved for the drawer-open swipe (1.12); must match
    // `EDGE_SWIPE_START_PX` on the web side. The OS caps total exclusion height
    // per edge (~200dp), so this is best-effort — the hamburger is the
    // guaranteed opener.
    private const val EDGE_SWIPE_DP = 24

    fun install(activity: Activity) {
        val root = activity.findViewById<ViewGroup>(android.R.id.content) ?: return
        density = activity.resources.displayMetrics.density

        // One listener handles every inset type — adding the IME here (rather
        // than registering a second `setOnApplyWindowInsetsListener`, which would
        // *replace* this one) keeps system-bar and keyboard handling chained.
        ViewCompat.setOnApplyWindowInsetsListener(root) { _, insets ->
            val sysBars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
            val navBars = insets.getInsets(WindowInsetsCompat.Type.navigationBars())
            val tappable = insets.getInsets(WindowInsetsCompat.Type.tappableElement())
            val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
            // Different Android versions / nav modes report the bottom area
            // under different inset types — pick the largest so we get a
            // non-zero value regardless. 3-button nav fills `navigationBars`,
            // gesture nav puts the visual hint area under `tappableElement`.
            val effectiveBottom = maxOf(sysBars.bottom, navBars.bottom, tappable.bottom)
            cachedTopPx = (sysBars.top / density).toInt()
            cachedBottomPx = (effectiveBottom / density).toInt()
            cachedLeftPx = (sysBars.left / density).toInt()
            cachedRightPx = (sysBars.right / density).toInt()
            // `ime().bottom` already includes the nav-bar height when the
            // keyboard is up, so it's the full bottom occlusion; 0 when hidden.
            cachedKeyboardPx = (ime.bottom / density).toInt()
            applyToWebView(root)
            applyGestureExclusion(root)
            insets
        }

        // The inset listener typically fires only in the first ~150ms, before
        // the real app document exists (Tauri's WebView swaps documents during
        // early load and the 54MB WASM bundle parse takes 5-10s). Re-apply on
        // a schedule so a later mutation lands on the loaded document.
        val handler = Handler(Looper.getMainLooper())
        for (delayMs in listOf(500L, 1500L, 3000L, 6000L, 10000L)) {
            handler.postDelayed({
                applyToWebView(root)
                applyGestureExclusion(root)
            }, delayMs)
        }
    }

    // 1.12: reserve the left-edge strip for the drawer-open swipe so the system
    // back-gesture doesn't intercept it. Set on the content root (gesture
    // exclusion is view-local); re-applied on the schedule because `root.height`
    // is 0 until the first layout. API 29+; the OS clamps the height for us.
    private fun applyGestureExclusion(root: ViewGroup) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q || root.height == 0) return
        val edgePx = (EDGE_SWIPE_DP * density).toInt()
        root.systemGestureExclusionRects = listOf(Rect(0, 0, edgePx, root.height))
    }

    private fun applyToWebView(root: ViewGroup) {
        val webView = findWebView(root) ?: return
        webView.evaluateJavascript(
            """(function(){var r=document.documentElement.style;""" +
                """r.setProperty('--safe-area-inset-top','${cachedTopPx}px');""" +
                """r.setProperty('--safe-area-inset-bottom','${cachedBottomPx}px');""" +
                """r.setProperty('--safe-area-inset-left','${cachedLeftPx}px');""" +
                """r.setProperty('--safe-area-inset-right','${cachedRightPx}px');""" +
                """r.setProperty('--keyboard-inset-bottom','${cachedKeyboardPx}px');""" +
                // On edge-to-edge the visual viewport doesn't resize when the IME
                // opens, so the web layer has no native event for the keyboard. Emit
                // one here, right after the inset CSS var updates, so editor.js can
                // re-scroll the caret above the keyboard.
                """window.dispatchEvent(new Event('omni:keyboardinset'));})();""",
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
