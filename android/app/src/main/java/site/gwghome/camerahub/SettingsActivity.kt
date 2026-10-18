package site.gwghome.camerahub

import android.app.AlertDialog
import android.content.Intent
import android.os.Bundle
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.view.View
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.Spinner
import android.widget.Switch
import org.json.JSONArray
import org.json.JSONObject

class SettingsActivity : ApiActivity() {
    private lateinit var content: LinearLayout
    private lateinit var kind: String
    private var loaded = JSONObject()
    private var dirty = false
    private var building = false
    private var draft: Bundle? = null
    private val fields = linkedMapOf<String, EditText>()
    private val switches = linkedMapOf<String, Switch>()
    private val secretFields = setOf("secret_key", "app_secret")
    private val records = mutableListOf<Pair<EditText, EditText>>()
    private val aliases = linkedMapOf<String, EditText>()
    private var defaultGroup: Spinner? = null
    private var groupIds = listOf<String>()
    private lateinit var login: Button
    private val titles = mapOf("settings" to "录像与 AI", "ddns" to "DDNS 动态解析", "qq" to "QQ 机器人")

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (isFinishing) return
        kind = intent.getStringExtra("kind").orEmpty()
        if (kind !in titles) { finish(); return }
        draft = savedInstanceState
        val root = ui.column().apply { id = R.id.root }
        val header = ui.column(16)
        ui.button(header, "返回") { handleBack() }
        ui.text(header, titles.getValue(kind), 24f)
        ui.text(header, server.url, 12f, true)
        notice = ui.text(header, "", 13f, true).apply { accessibilityLiveRegion = View.ACCESSIBILITY_LIVE_REGION_POLITE }
        login = ui.button(header, "恢复本机连接") {
            startActivity(Intent(this, DashboardActivity::class.java)
                .putExtra("server", server.url).putExtra("return_after_login", true))
        }.apply { visibility = View.GONE }
        root.addView(header)
        content = ui.column(16)
        root.addView(ui.scroll(content), LinearLayout.LayoutParams(-1, 0, 1f))
        setContentView(root)
        fitSystemBars()
        load()
    }

    private fun load() {
        if (content.childCount == 0) ui.button(content, "重试加载") { load() }
        request(work = { api.request("/api/v1/$kind") }, done = { result ->
            loaded = if (kind == "settings") result else result.getJSONObject(kind)
            render()
        })
    }

    private fun render() {
        building = true
        fields.clear(); switches.clear(); records.clear(); aliases.clear()
        defaultGroup = null
        content.removeAllViews()
        when (kind) {
            "settings" -> renderSettings(loaded.getJSONObject("settings"))
            "ddns" -> renderDdns(loaded.getJSONObject("config"))
            "qq" -> renderQq(loaded.getJSONObject("config"))
        }
        ui.button(content, "保存配置", true) { save() }
        ui.button(content, "重新加载") {
            if (dirty) confirm("重新加载", "丢弃本页未保存的修改？") { draft = null; load() }
            else load()
        }
        building = false
        dirty = draft?.getBoolean("dirty") ?: false
        draft = null
    }

    private fun field(parent: LinearLayout, key: String, label: String, value: Any,
                      number: Boolean = false, decimal: Boolean = false, secret: Boolean = false): EditText {
        val type = if (number) InputType.TYPE_CLASS_NUMBER or
            (if (decimal) InputType.TYPE_NUMBER_FLAG_DECIMAL else 0) else InputType.TYPE_CLASS_TEXT
        val saved = if (secret) null else draft?.getString("field:$key")
        return ui.field(parent, label, saved ?: value.toString(), type, secret).also {
            fields[key] = it
            watch(it)
        }
    }

    private fun watch(input: EditText) {
        input.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {
                if (!building) dirty = true
            }
            override fun afterTextChanged(s: Editable?) {}
        })
    }

    private fun toggle(parent: LinearLayout, key: String, label: String, value: Boolean) =
        ui.toggle(parent, label, draft?.getBoolean("toggle:$key", value) ?: value).also {
            switches[key] = it
            it.setOnCheckedChangeListener { _, _ -> if (!building) dirty = true }
        }

    private fun renderSettings(config: JSONObject) {
        val recording = ui.card(content)
        ui.text(recording, "录像保存", 20f)
        toggle(recording, "record_enabled", "保存录像", config.optBoolean("record_enabled"))
        field(recording, "segment_seconds", "分片时长（秒，10–3600）", config.getLong("segment_seconds"), true)
        field(recording, "retain_days", "保留天数（1–365）", config.getLong("retain_days"), true)
        field(recording, "max_gib", "容量上限（GiB，0.0625–512）", config.getDouble("max_bytes") / 1073741824, true, true)
        ui.text(recording, "缩短保留时间或降低容量可能清理已有录像。", 12f, true)
        val ai = ui.card(content)
        ui.text(ai, "人形检测", 20f)
        toggle(ai, "ai_enabled", "启用 AI 检测", config.optBoolean("ai_enabled"))
        field(ai, "ai_interval_ms", "推理间隔（毫秒，500–60000）", config.getLong("ai_interval_ms"), true)
        field(ai, "ai_threshold", "置信度（0.05–0.95）", config.getDouble("ai_threshold"), true, true)
        field(ai, "ai_min_person_area_ratio", "最小人物框面积比例（0–1）", config.getDouble("ai_min_person_area_ratio"), true, true)
        field(ai, "ai_min_snapshot_seconds", "抓拍冷却（秒，1–3600）", config.getLong("ai_min_snapshot_seconds"), true)
        field(ai, "ai_snapshot_max_count", "单设备照片上限（1–100000）", config.getLong("ai_snapshot_max_count"), true)
        field(ai, "ai_snapshot_quality", "JPEG 质量（1–100）", config.getInt("ai_snapshot_quality"), true)
    }

    private fun renderDdns(config: JSONObject) {
        // Keep the draft's revision so rotation cannot bypass server conflict detection.
        if (draft?.getBoolean("dirty") == true && draft?.containsKey("ddns_revision") == true) {
            config.put("revision", draft!!.getLong("ddns_revision"))
        }
        val status = loaded.optJSONObject("status") ?: JSONObject()
        val state = ui.card(content)
        ui.text(state, if (loaded.optBoolean("worker_online")) "同步状态" else "DDNS 进程离线", 18f)
        ui.text(state, status.optString("detail").ifBlank { "等待运行状态" }, muted = true)
        ui.text(state, status.optString("prefix"), 13f, true)
        if (status.optString("last_error").isNotEmpty()) ui.text(state, status.optString("last_error"), muted = true)
        val credentials = ui.card(content)
        ui.text(credentials, "DNSPod 凭据", 20f)
        toggle(credentials, "enabled", "启用 IPv6 动态解析", config.optBoolean("enabled"))
        field(credentials, "domain", "主域名", config.optString("domain"))
        field(credentials, "secret_id", "SecretId", config.optString("secret_id"))
        field(credentials, "secret_key", "SecretKey", "", secret = true)
        ui.text(credentials, if (config.optBoolean("secret_key_configured"))
            "密钥已配置，留空保留；更换 SecretId 时请重新填写。" else "尚未配置密钥", 12f, true)
        val network = ui.card(content)
        ui.text(network, "网络与周期", 20f)
        field(network, "interface", "公网网卡", config.optString("interface", "wlan0"))
        field(network, "ttl", "TTL（秒）", config.getLong("ttl"), true)
        field(network, "interval_seconds", "检查周期（秒）", config.getLong("interval_seconds"), true)
        field(network, "force_seconds", "强制对账周期（秒）", config.getLong("force_seconds"), true)
        val rows = ui.card(content)
        ui.text(rows, "AAAA 记录", 20f)
        ui.text(rows, "记录须已存在于 DNSPod；填写名称和固定 IPv6 后 64 位。", 12f, true)
        val container = ui.column()
        rows.addView(container)
        val values = draft?.getString("records")?.let(::JSONArray) ?: config.getJSONArray("records")
        for (i in 0 until values.length()) {
            val item = values.getJSONObject(i)
            addRecord(container, item.optString("name"), item.optString("iid"))
        }
        ui.button(rows, "新增记录") { addRecord(container, "", ""); dirty = true }
        ui.button(rows, "预览地址") {
            validated {
                val payload = JSONObject().put("domain", text("domain")).put("interface", text("interface"))
                    .put("records", recordValues())
                request("正在预览…", content, { api.request("/api/v1/ddns/preview", "POST", payload) }) { result ->
                    val preview = result.getJSONObject("preview")
                    val list = preview.getJSONArray("records")
                    val lines = (0 until list.length()).joinToString("\n\n") {
                        val record = list.getJSONObject(it)
                        "${record.optString("fqdn")}\n${record.optString("address")}"
                    }
                    AlertDialog.Builder(this).setTitle(preview.optString("prefix"))
                        .setMessage(lines).setPositiveButton("关闭", null).show()
                }
            }
        }
        ui.button(state, "立即对账") {
            if (dirty) notice.text = "请先保存配置"
            else request("正在请求对账…", content, {
                api.request("/api/v1/ddns/reconcile", "POST", JSONObject())
            }) { notice.text = "已请求对账，请稍后刷新查看结果" }
        }.isEnabled = config.optBoolean("enabled") && loaded.optBoolean("worker_online")
        ui.button(credentials, "清除密钥") {
            confirm("清除 DNSPod 密钥", "同时关闭 DDNS，现有解析记录不会删除。") { save(clearSecret = true) }
        }.isEnabled = config.optBoolean("secret_key_configured")
    }

    private fun addRecord(parent: LinearLayout, name: String, iid: String) {
        val row = ui.card(parent)
        val a = ui.field(row, "记录名（@ 或子域名）", name).also(::watch)
        val b = ui.field(row, "IPv6 后 64 位", iid).also(::watch)
        val pair = a to b
        records.add(pair)
        ui.button(row, "删除这条记录") {
            records.remove(pair)
            parent.removeView(row)
            dirty = true
        }
    }

    private fun renderQq(config: JSONObject) {
        val status = loaded.optJSONObject("status") ?: JSONObject()
        val state = ui.card(content)
        ui.text(state, if (status.optBoolean("online")) "机器人在线" else "机器人未连接", 18f)
        ui.text(state, status.optString("detail"), muted = true)
        if (status.optString("last_error").isNotBlank()) ui.text(state, status.optString("last_error"))
        val credentials = ui.card(content)
        ui.text(credentials, "机器人凭据", 20f)
        toggle(credentials, "enabled", "启用 QQ 机器人", config.optBoolean("enabled"))
        field(credentials, "app_id", "AppID", config.optString("app_id"), true)
        field(credentials, "app_secret", "AppSecret", "", secret = true)
        ui.text(credentials, if (config.optBoolean("secret_configured"))
            "密钥已配置，留空保留；更换 AppID 时请重新填写。" else "尚未配置密钥", 12f, true)
        ui.button(credentials, "清除密钥") {
            confirm("清除 AppSecret", "清除后机器人将断开连接。") { save(clearSecret = true) }
        }.isEnabled = config.optBoolean("secret_configured")
        val groups = ui.card(content)
        ui.text(groups, "消息目标", 20f)
        val values = config.optJSONArray("groups") ?: JSONArray()
        groupIds = listOf("") + (0 until values.length()).map { values.getJSONObject(it).getString("openid") }
        val names = listOf("请选择默认群") + (0 until values.length()).map {
            values.getJSONObject(it).optString("name").ifBlank { "未命名群" }
        }
        ui.text(groups, "默认群", 13f, true)
        defaultGroup = Spinner(this).also {
            it.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_dropdown_item, names)
            groups.addView(it, LinearLayout.LayoutParams(-1, ui.dp(52)))
            it.setSelection(groupIds.indexOf(draft?.getString("default_group") ?: config.optString("default_group")).coerceAtLeast(0))
            it.onItemSelectedListener = object : android.widget.AdapterView.OnItemSelectedListener {
                override fun onNothingSelected(parent: android.widget.AdapterView<*>?) {}
                override fun onItemSelected(parent: android.widget.AdapterView<*>?, view: View?, position: Int, id: Long) {
                    if (!building && groupIds[position] != config.optString("default_group")) dirty = true
                }
            }
        }
        if (values.length() == 0) ui.text(groups, "机器人加入群后会自动登记目标群。", muted = true)
        for (i in 0 until values.length()) {
            val group = values.getJSONObject(i)
            val id = group.getString("openid")
            val input = ui.field(groups, "群别名 · ${id.takeLast(6)}",
                draft?.getString("alias:$id") ?: group.optString("name"))
            watch(input)
            aliases[id] = input
        }
    }

    private fun text(key: String) = fields.getValue(key).text.toString().trim()
    private fun number(key: String, min: Double, max: Double): Double {
        val value = text(key).toDoubleOrNull()
        if (value == null || !value.isFinite() || value !in min..max) {
            fields.getValue(key).error = "请输入 $min–$max 范围内的数值"
            fields.getValue(key).requestFocus()
            error("请检查标记的数值")
        }
        return value
    }
    private fun integer(key: String, min: Long, max: Long): Long {
        val value = number(key, min.toDouble(), max.toDouble())
        require(value == value.toLong().toDouble()) { "请输入整数" }
        return value.toLong()
    }
    private fun recordValues(): JSONArray = JSONArray().apply {
        records.forEach { (a, b) ->
            put(JSONObject().put("name", a.text.toString().trim()).put("iid", b.text.toString().trim()))
        }
    }

    private fun payload(clearSecret: Boolean): JSONObject {
        val config = loaded.getJSONObject(if (kind == "settings") "settings" else "config")
        return when (kind) {
            "settings" -> JSONObject().apply {
                for (key in listOf("ai_enabled", "record_enabled")) put(key, switches.getValue(key).isChecked)
                put("segment_seconds", integer("segment_seconds", 10, 3600))
                put("retain_days", integer("retain_days", 1, 365))
                put("max_bytes", (number("max_gib", 0.0625, 512.0) * 1073741824).toLong())
                put("ai_interval_ms", integer("ai_interval_ms", 500, 60000))
                put("ai_threshold", number("ai_threshold", 0.05, 0.95))
                put("ai_min_person_area_ratio", number("ai_min_person_area_ratio", 0.0, 1.0))
                put("ai_min_snapshot_seconds", integer("ai_min_snapshot_seconds", 1, 3600))
                put("ai_snapshot_max_count", integer("ai_snapshot_max_count", 1, 100000))
                put("ai_snapshot_quality", integer("ai_snapshot_quality", 1, 100))
            }
            "ddns" -> JSONObject().apply {
                put("revision", config.getLong("revision"))
                put("enabled", !clearSecret && switches.getValue("enabled").isChecked)
                for (key in listOf("domain", "secret_id", "interface")) put(key, text(key))
                put("secret_key", if (clearSecret) "" else text("secret_key"))
                put("clear_secret", clearSecret)
                put("ttl", integer("ttl", 1, 604800))
                val interval = integer("interval_seconds", 10, 86400)
                put("interval_seconds", interval)
                put("force_seconds", integer("force_seconds", interval, 2592000))
                put("records", recordValues())
            }
            else -> JSONObject().apply {
                put("enabled", !clearSecret && switches.getValue("enabled").isChecked)
                put("app_id", text("app_id"))
                put("app_secret", if (clearSecret) "" else text("app_secret"))
                put("clear_secret", clearSecret)
                put("default_group", groupIds[defaultGroup?.selectedItemPosition?.coerceAtLeast(0) ?: 0])
                put("group_aliases", JSONObject().apply { aliases.forEach { (id, input) -> put(id, input.text.toString().trim()) } })
            }
        }
    }

    private fun save(clearSecret: Boolean = false) = validated {
        val body = payload(clearSecret)
        val execute = {
            request("正在保存…", content, { api.request("/api/v1/$kind", "PUT", body) }) { result ->
                secretFields.forEach { fields[it]?.text?.clear() }
                dirty = false
                draft = null
                loaded.put(if (kind == "settings") "settings" else "config",
                    result.getJSONObject(if (kind == "settings") "settings" else "config"))
                render()
                notice.text = "配置已保存"
            }
        }
        val old = loaded.optJSONObject("settings")
        if (kind == "settings" && old != null &&
            (body.getLong("retain_days") < old.getLong("retain_days") ||
                body.getLong("max_bytes") < old.getLong("max_bytes") ||
                body.getLong("ai_snapshot_max_count") < old.getLong("ai_snapshot_max_count"))) {
            confirm("调整存储上限", "保存后可能清理超出限制的录像或照片。") { execute() }
        } else execute()
    }

    private fun validated(action: () -> Unit) {
        try { action() } catch (error: Exception) {
            notice.text = error.message
            notice.setTextColor(getColor(R.color.warning))
        }
    }
    private fun confirm(title: String, message: String, action: () -> Unit) {
        AlertDialog.Builder(this).setTitle(title).setMessage(message)
            .setNegativeButton("取消", null).setPositiveButton("确认") { _, _ -> action() }.show()
    }
    override fun unauthorized() {
        notice.text = "本机会话已过期，恢复连接后可继续编辑。"
        login.visibility = View.VISIBLE
    }
    override fun onResume() {
        super.onResume()
        if (::login.isInitialized && login.visibility == View.VISIBLE && HubSessions.cookie(server) != null) {
            login.visibility = View.GONE
            if (fields.isEmpty()) load() else notice.text = "本机连接已恢复，可以继续保存"
        }
    }
    override fun handleBack() {
        if (busy) {
            notice.text = "操作进行中，请稍候"
        } else if (dirty) confirm("离开配置页", "放弃未保存的修改？") { finish() }
        else finish()
    }
    override fun onSaveInstanceState(outState: Bundle) {
        fields.filterKeys { it !in secretFields }.forEach { (key, input) -> outState.putString("field:$key", input.text.toString()) }
        switches.forEach { (key, input) -> outState.putBoolean("toggle:$key", input.isChecked) }
        if (kind == "ddns") {
            outState.putString("records", recordValues().toString())
            loaded.optJSONObject("config")?.let { outState.putLong("ddns_revision", it.optLong("revision")) }
        }
        aliases.forEach { (id, input) -> outState.putString("alias:$id", input.text.toString()) }
        defaultGroup?.let { outState.putString("default_group", groupIds[it.selectedItemPosition.coerceAtLeast(0)]) }
        outState.putBoolean("dirty", dirty)
        super.onSaveInstanceState(outState)
    }
}
