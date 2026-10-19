package site.gwghome.camerahub

import android.Manifest
import android.app.Activity
import android.app.KeyguardManager
import android.content.ActivityNotFoundException
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.view.View
import android.view.WindowInsets
import android.widget.Toast
import android.window.OnBackInvokedDispatcher

abstract class HubActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (Build.VERSION.SDK_INT >= 33) {
            onBackInvokedDispatcher.registerOnBackInvokedCallback(
                OnBackInvokedDispatcher.PRIORITY_DEFAULT,
            ) { handleBack() }
        }
    }

    override fun onResume() {
        super.onResume()
        resumeMicrophone()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) resumeMicrophone()
    }

    private fun resumeMicrophone() {
        if (!EngineService.active || EngineService.microphoneEnabled ||
            getSystemService(KeyguardManager::class.java).isKeyguardLocked ||
            checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED
        ) return
        runCatching {
            startForegroundService(Intent(this, EngineService::class.java)
                .setAction(EngineService.RESUME_VOICE))
        }.onFailure {
            EngineState(this).recordFailure("自动恢复语音失败：${it.message}")
        }
    }

    @Suppress("DEPRECATION") // Platform equivalent of AndroidX edge-to-edge support.
    protected fun fitSystemBars() {
        val root = findViewById<View>(R.id.root)
        if (Build.VERSION.SDK_INT >= 30) {
            window.setDecorFitsSystemWindows(false)
            root.setOnApplyWindowInsetsListener { view, insets ->
                val bars = insets.getInsets(
                    WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout() or
                        WindowInsets.Type.ime(),
                )
                view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
                insets
            }
            root.requestApplyInsets()
        } else {
            root.fitsSystemWindows = true
        }
    }

    protected open fun handleBack() = finish()

    @Suppress("DEPRECATION")
    @Deprecated("Used by Android 8–12; newer versions use OnBackInvokedDispatcher.")
    override fun onBackPressed() = handleBack()

    protected fun openBrowser(url: String) {
        val uri = Uri.parse(url)
        if (uri.scheme != "https" && uri.scheme != "http") return
        try {
            startActivity(Intent(Intent.ACTION_VIEW, uri).addCategory(Intent.CATEGORY_BROWSABLE))
        } catch (_: ActivityNotFoundException) {
            toast(R.string.browser_missing)
        }
    }

    protected fun toast(textId: Int) = Toast.makeText(this, textId, Toast.LENGTH_LONG).show()
}
