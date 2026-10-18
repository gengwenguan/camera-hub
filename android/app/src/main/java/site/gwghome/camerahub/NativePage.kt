package site.gwghome.camerahub

import android.app.AlertDialog
import android.content.Intent
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.View
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import org.json.JSONArray
import org.json.JSONObject

/** Shared navigation and visible-only polling for native tools. Editors keep their own drafts. */
abstract class NativePage : ApiActivity() {
    protected lateinit var content: LinearLayout
    protected lateinit var scroll: ScrollView
    private lateinit var login: Button
    protected open val pollMillis = 0L
    private var visible = false
    private val handler = Handler(Looper.getMainLooper())
    private val poll = object : Runnable {
        override fun run() {
            if (!visible) return
            if (!busy && HubSessions.cookie(server) != null) {
                pollingRequest = true
                try { refresh() } finally { pollingRequest = false }
            }
            handler.postDelayed(this, pollMillis)
        }
    }

    protected fun page(title: String) {
        val root = ui.column().apply { id = R.id.root }
        val header = ui.column(16)
        ui.button(header, "返回") { handleBack() }
        ui.text(header, title, 24f)
        ui.text(header, server.url, 12f, true)
        notice = ui.text(header, "", 13f, true).apply {
            accessibilityLiveRegion = View.ACCESSIBILITY_LIVE_REGION_POLITE
        }
        login = ui.button(header, "恢复本机连接") {
            startActivity(Intent(this, DashboardActivity::class.java)
                .putExtra("server", server.url).putExtra("return_after_login", true))
        }.apply { visibility = View.GONE }
        root.addView(header)
        content = ui.column(16)
        scroll = ui.scroll(content)
        root.addView(scroll, LinearLayout.LayoutParams(-1, 0, 1f))
        setContentView(root)
        fitSystemBars()
    }

    protected abstract fun refresh()
    protected fun confirm(title: String, message: String, action: () -> Unit) {
        AlertDialog.Builder(this).setTitle(title).setMessage(message)
            .setNegativeButton("取消", null).setPositiveButton("确定") { _, _ -> action() }.show()
    }
    protected fun openWeb(page: String) {
        startActivity(Intent(this, ConsoleActivity::class.java)
            .putExtra("server", server.url).putExtra("page", "/#$page"))
    }
    override fun unauthorized() {
        notice.text = "本机会话已过期，请恢复连接"
        login.visibility = View.VISIBLE
    }
    override fun onResume() {
        super.onResume()
        if (isFinishing) return
        visible = true
        if (login.visibility == View.VISIBLE && HubSessions.cookie(server) != null) {
            login.visibility = View.GONE
            notice.text = "本机连接已恢复，请继续操作或刷新"
        }
        if (pollMillis > 0) handler.postDelayed(poll, pollMillis)
    }
    override fun onPause() {
        visible = false
        handler.removeCallbacks(poll)
        super.onPause()
    }
}

internal fun JSONArray.objects(): List<JSONObject> = (0 until length()).map { getJSONObject(it) }
internal fun segment(value: String) = android.net.Uri.encode(value)
