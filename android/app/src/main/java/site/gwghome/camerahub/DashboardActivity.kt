package site.gwghome.camerahub

import android.app.AlertDialog
import android.content.Intent
import android.graphics.Typeface
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.View
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import org.json.JSONObject
import java.util.Locale

class DashboardActivity : ApiActivity() {
    private lateinit var content: LinearLayout
    private lateinit var scroll: ScrollView
    private lateinit var navigation: LinearLayout
    private val tabs = mutableListOf<Button>()
    private var selected = 0
    private var authenticated = false
    private var visible = false
    private val handler = Handler(Looper.getMainLooper())
    private val poll = object : Runnable {
        override fun run() {
            if (visible && authenticated && selected == 0 && !busy) loadOverview()
            handler.postDelayed(this, 10_000)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (isFinishing) return
        val root = ui.column().apply { id = R.id.root }
        val header = ui.column(12)
        ui.text(header, "本机节点", 18f)
        ui.text(header, "此设备 · ${server.url}", 12f, true)
        notice = ui.text(header, "", 13f, true).apply { accessibilityLiveRegion = View.ACCESSIBILITY_LIVE_REGION_POLITE }
        root.addView(header)
        content = ui.column(16)
        scroll = ui.scroll(content)
        root.addView(scroll, LinearLayout.LayoutParams(-1, 0, 1f))
        navigation = ui.row(root).apply {
            setPadding(ui.dp(10), ui.dp(8), ui.dp(10), ui.dp(8))
            setBackgroundColor(getColor(R.color.surface))
        }
        listOf("概览", "配置", "工具").forEachIndexed { index, label ->
            tabs += ui.button(navigation, label) {
                if (!busy) select(index)
            }.also(ui::compact)
        }
        setContentView(root)
        fitSystemBars()
        selected = savedInstanceState?.getInt("tab") ?: 0
        autoLoginLocal()
    }

    private fun showLocalUnavailable(message: String) {
        authenticated = false
        notice.text = message
        navigation.visibility = View.GONE
        content.removeAllViews()
        ui.title(content, "无法打开本机节点", "检查本机服务状态后重试")
        val card = ui.card(content)
        ui.text(card, message, muted = true)
        ui.button(card, "重试", true) { autoLoginLocal() }
        ui.button(card, "本机服务设置") {
            startActivity(Intent(this, EngineActivity::class.java))
        }
        ui.button(card, "修改登录密码") {
            startActivity(Intent(this, EngineActivity::class.java)
                .putExtra("change_password", true))
        }
    }

    private fun autoLoginLocal() {
        authenticated = false
        navigation.visibility = View.GONE
        content.removeAllViews()
        ui.title(content, "正在打开本机节点", "使用 App 私有凭据建立本机会话")
        val panel = ui.card(content)
        ui.text(panel, "正在连接 ${server.url}")
        ui.text(panel, "无需重复输入账号和密码。", muted = true)
        ui.button(panel, "本机服务设置") {
            startActivity(Intent(this, EngineActivity::class.java))
        }
        val body = JSONObject().put("username", "admin")
            .put("password", EngineConfig(this).password)
        request("正在验证本机服务…", panel, {
            val deadline = SystemClock.elapsedRealtime() + 20_000
            while (true) {
                try {
                    api.request("/api/v1/auth/login", "POST", body)
                    check(HubSessions.cookie(server) != null) { "服务未返回登录凭据" }
                    return@request api.request("/api/v1/info")
                } catch (error: Exception) {
                    HubSessions.clear(server)
                    if (error is ApiException || !EngineService.active ||
                        SystemClock.elapsedRealtime() >= deadline
                    ) throw error
                    Thread.sleep(500)
                }
            }
        }, failed = { error ->
            showLocalUnavailable(errorText(error))
        }) {
            authenticated = true
            if (intent.getBooleanExtra("return_after_login", false)) finish() else select(selected)
        }
    }

    override fun unauthorized() {
        if (authenticated) autoLoginLocal()
    }

    private fun select(index: Int) {
        selected = index.coerceIn(0, 2)
        navigation.visibility = View.VISIBLE
        tabs.forEachIndexed { i, button ->
            button.isSelected = i == selected
            button.alpha = if (i == selected) 1f else 0.6f
            button.typeface = Typeface.create("sans-serif", if (i == selected) Typeface.BOLD else Typeface.NORMAL)
        }
        content.removeAllViews()
        scroll.scrollTo(0, 0)
        when (selected) {
            0 -> loadOverview()
            1 -> showConfiguration()
            else -> showTools()
        }
    }

    private fun loadOverview() {
        if (content.childCount == 0) ui.button(content, "重试加载") { loadOverview() }
        val previousScroll = scroll.scrollY
        request(work = {
            val endpoints = listOf("info", "devices", "media/status", "system/status", "components", "ir", "ai/status")
            endpoints.associateWith { path ->
                try { api.request("/api/v1/$path") } catch (error: Exception) {
                    if (error is ApiException && error.status == 401 || path == "info") throw error
                    JSONObject().put("load_error", errorText(error))
                }
            }
        }, done = { data ->
            content.removeAllViews()
            ui.title(content, "运行概览", "设备、服务与快捷控制")
            val info = data.getValue("info")
            val devices = data.getValue("devices").optJSONArray("devices")
            val media = data.getValue("media/status").optJSONArray("devices")
            val online = (0 until (devices?.length() ?: 0)).count { devices!!.getJSONObject(it).optBoolean("online") }
            val recording = (0 until (media?.length() ?: 0)).count { media!!.getJSONObject(it).optBoolean("recording") }
            val summary = ui.card(content)
            ui.text(summary, "$online 台在线  ·  $recording 台录像", 22f)
            ui.text(summary, "录像策略：${if (info.optBoolean("record_enabled")) "开启" else "关闭"} · 保留 ${info.optLong("retain_days")} 天", muted = true)
            ui.button(summary, "刷新状态") { loadOverview() }

            val system = data.getValue("system/status")
            val host = ui.card(content)
            ui.text(host, "主机资源", 18f)
            if (system.has("load_error")) ui.text(host, system.getString("load_error"), muted = true)
            else {
                val cpu = system.optJSONObject("cpu")
                val memory = system.optJSONObject("mem")
                val disk = system.optJSONObject("disk")
                ui.text(host, "已运行 ${NativeUi.duration(system.optJSONObject("process_uptime")?.optLong("seconds") ?: 0)}")
                ui.text(host, "CPU  " + if (cpu?.optBoolean("valid") == true)
                    String.format(Locale.ROOT, "%.0f%%", cpu.optDouble("percent")) else "系统暂未提供")
                if (memory?.optBoolean("valid") == true) ui.text(host,
                    "内存可用 ${NativeUi.bytes(memory.optDouble("available_kb") * 1024)}")
                if (disk?.optBoolean("valid") == true) ui.text(host,
                    "磁盘可用 ${NativeUi.bytes(disk.optDouble("available_bytes"))}")
            }
            val deviceCard = ui.card(content)
            ui.text(deviceCard, "摄像头", 18f)
            if (devices == null) ui.text(deviceCard, data.getValue("devices").optString("load_error", "暂无设备"))
            else if (devices.length() == 0) ui.text(deviceCard, "开发板连接后会自动出现在这里。", muted = true)
            else for (i in 0 until devices.length()) {
                val d = devices.getJSONObject(i)
                ui.text(deviceCard, "${d.optString("device_id")} · ${if (d.optBoolean("online")) "在线" else "离线"}", 16f)
                ui.text(deviceCard, "最近心跳 ${d.optLong("age_seconds")} 秒前", 12f, true)
            }
            ui.button(deviceCard, "查看直播") { openWeb("camera/live") }
            val components = data.getValue("components").optJSONObject("components")
            if (components != null) showComponents(components)
            else ui.text(content, data.getValue("components").optString("load_error", "组件状态不可用"))
            showInfrared(data.getValue("ir"))
            val ai = data.getValue("ai/status").optJSONObject("ai")
            val aiCard = ui.card(content)
            ui.text(aiCard, "AI 检测", 18f)
            ui.text(aiCard, when {
                ai == null -> data.getValue("ai/status").optString("load_error", "状态不可用")
                !ai.optBoolean("enabled") -> "已关闭"
                ai.optBoolean("running") -> "运行中 · 已推理 ${ai.optLong("inference_count")} 次"
                else -> ai.optString("last_error").ifBlank { "等待模型或运行库就绪" }
            })
            ui.button(aiCard, "录像与 AI 设置") { openConfig("settings") }
            scroll.post { scroll.scrollTo(0, previousScroll) }
            notice.text = getString(R.string.native_updated, java.text.DateFormat.getTimeInstance().format(java.util.Date()))
        })
    }

    private fun showComponents(components: JSONObject) {
        ui.text(content, "服务组件", 20f)
        val labels = mapOf("voice" to "语音识别", "tts" to "音色合成", "ddns" to "DDNS", "ir" to "红外")
        labels.forEach { (id, label) ->
            val service = components.optJSONObject(id) ?: return@forEach
            val card = ui.card(content)
            ui.text(card, "$label · ${if (service.optBoolean("healthy")) "就绪" else if (service.optBoolean("running")) "运行中" else "未运行"}", 18f)
            ui.text(card, service.optString("detail"), muted = true)
            val controllable = components.optBoolean("control_available")
            val row = ui.row(card)
            listOf("start" to "启动", "restart" to "重启", "stop" to "停止").forEach { (action, title) ->
                ui.button(row, title) {
                    val execute = {
                        request("正在$title$label…", content, {
                            api.request("/api/v1/components/$id/$action", "POST", JSONObject())
                        }) { loadOverview() }
                    }
                    if (action == "start") execute()
                    else AlertDialog.Builder(this).setTitle("$title$label")
                        .setMessage("此操作会中断该组件当前工作。")
                        .setNegativeButton("取消", null).setPositiveButton(title) { _, _ -> execute() }.show()
                }.also {
                    ui.compact(it)
                    it.contentDescription = "$title$label"
                    it.isEnabled = controllable && service.optBoolean("installed") &&
                        service.optString("state") !in listOf("maintenance", "stopping") &&
                        (if (action == "start") !service.optBoolean("running") else service.optBoolean("running"))
                }
            }
            ui.toggle(card, "随主服务自动启动", service.optBoolean("autostart")).apply {
                contentDescription = "$label 随主服务自动启动"
                isEnabled = controllable
                setOnClickListener {
                    val wanted = isChecked
                    isChecked = !wanted
                    request("正在保存…", content, {
                        api.request("/api/v1/components/$id/autostart", "PUT", JSONObject().put("enabled", wanted))
                    }) { loadOverview() }
                }
            }
        }
    }

    private fun showInfrared(body: JSONObject) {
        val card = ui.card(content)
        ui.text(card, "空调快捷控制", 18f)
        val ir = body.optJSONObject("ir")
        ui.text(card, ir?.optString("detail") ?: body.optString("load_error", "红外状态不可用"), muted = true)
        ui.text(card, "显示的是发送记录；空调实际状态需现场确认。", 12f, true)
        ui.button(card, "温度、模式与定时") { openNative(AirconActivity::class.java) }
        val actions = ir?.optJSONArray("actions") ?: return
        for (i in 0 until actions.length()) {
            val action = actions.getJSONObject(i)
            val label = action.optString("label")
            val id = action.optString("id")
            if (!id.matches(Regex("[a-z0-9-]+"))) continue
            ui.button(card, label) {
                AlertDialog.Builder(this).setTitle(label).setMessage("请确认红外发射口对准空调。")
                    .setNegativeButton("取消", null).setPositiveButton("发送") { _, _ ->
                        request("正在发送…", content, {
                            api.request("/api/v1/ir/actions/$id", "POST", JSONObject())
                        }) { notice.text = getString(R.string.native_ir_sent, label) }
                    }.show()
            }.isEnabled = ir.optBoolean("available")
        }
    }

    private fun showConfiguration() {
        ui.title(content, "节点配置", "配置应用于当前连接的服务")
        val card = ui.card(content)
        ui.navigation(card, "录像与 AI 设置") { openConfig("settings") }
        ui.navigation(card, "DDNS 动态解析") { openConfig("ddns") }
        ui.navigation(card, "QQ 机器人") { openConfig("qq") }
        ui.navigation(card, "语音参数与命令") { openNative(VoiceActivity::class.java) }
        ui.navigation(card, "模型管理") { openNative(ModelsActivity::class.java) }
        val local = ui.card(content)
        ui.text(local, "这台手机", 18f)
        ui.text(local, "管理 App 登录密码、内置服务、权限和后台恢复。", muted = true)
        ui.navigation(local, "修改登录密码") {
            startActivity(Intent(this, EngineActivity::class.java)
                .putExtra("change_password", true))
        }
        ui.navigation(local, "本机服务设置") {
            startActivity(Intent(this, EngineActivity::class.java))
        }
    }

    private fun showTools() {
        ui.title(content, "媒体与工具", "文件、语音与设备控制")
        val card = ui.card(content)
        ui.navigation(card, "AI 相册") { openNative(MediaActivity::class.java, "photos") }
        ui.navigation(card, "录像文件") { openNative(MediaActivity::class.java, "records") }
        ui.navigation(card, "语音命令") { openNative(VoiceActivity::class.java) }
        ui.navigation(card, "模型管理") { openNative(ModelsActivity::class.java) }
        ui.navigation(card, "空调控制") { openNative(AirconActivity::class.java) }
        listOf("实时直播（Web）" to "camera/live", "时间轴回放（Web）" to "camera/playback",
            "音色工作室（Web）" to "voice/profile", "协议评测（Web）" to "camera/evaluation").forEach { (title, page) ->
            ui.navigation(card, title) { openWeb(page) }
        }
    }

    private fun openNative(activity: Class<out android.app.Activity>, kind: String = "") {
        startActivity(Intent(this, activity).putExtra("server", server.url).putExtra("kind", kind))
    }

    private fun openConfig(kind: String) {
        startActivity(Intent(this, SettingsActivity::class.java).putExtra("server", server.url).putExtra("kind", kind))
    }

    private fun openWeb(page: String) {
        startActivity(Intent(this, ConsoleActivity::class.java).putExtra("server", server.url).putExtra("page", "/#$page"))
    }

    override fun handleBack() {
        if (authenticated && selected != 0 && !busy) select(0) else finish()
    }
    override fun onResume() {
        super.onResume()
        visible = true
        if (authenticated && HubSessions.cookie(server) == null) {
            autoLoginLocal()
        }
        handler.postDelayed(poll, 10_000)
    }
    override fun onPause() {
        visible = false
        handler.removeCallbacks(poll)
        super.onPause()
    }
    override fun onSaveInstanceState(outState: Bundle) {
        outState.putInt("tab", selected)
        super.onSaveInstanceState(outState)
    }
}
