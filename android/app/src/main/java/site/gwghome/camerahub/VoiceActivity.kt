package site.gwghome.camerahub

import android.app.AlertDialog
import android.content.Intent
import android.os.Bundle
import android.os.SystemClock
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.Switch
import android.widget.TextView
import org.json.JSONArray
import org.json.JSONObject

class VoiceActivity : NativePage() {
    override val pollMillis = 3_000L
    private var config: JSONObject? = null
    private var draft: Bundle? = null
    private var dirty = false
    private var building = false
    private val fields = linkedMapOf<String, EditText>()
    private lateinit var enabled: Switch
    private lateinit var state: TextView
    private lateinit var transcript: TextView
    private lateinit var events: TextView
    private lateinit var commands: LinearLayout
    private var lastTranscriptEpoch = 0L
    private var waitingSince = 0L
    private var previousTranscriptEpoch = 0L
    private var serverEpoch = 0L

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (isFinishing) return
        draft = savedInstanceState
        page("语音命令")
        ui.button(content, "刷新语音状态") { refresh() }
        refresh()
    }

    override fun refresh() {
        request(work = { api.request("/api/v1/voice") }, done = { body ->
            if (config == null) {
                config = draft?.getString("config")?.let(::JSONObject) ?: body.getJSONObject("config")
                buildForm()
            }
            serverEpoch = body.optLong("server_epoch")
            val status = body.getJSONObject("status")
            val stale = status.optLong("updated_epoch") == 0L || serverEpoch - status.optLong("updated_epoch") > 20
            state.text = getString(R.string.voice_state, if (stale) "状态已过期，语音进程可能离线" else
                "${if (status.optBoolean("running")) "运行中" else "未运行"} · ${status.optString("state")}",
                status.optString("last_error"), status.optString("asr_state"))
            lastTranscriptEpoch = status.optLong("asr_transcript_epoch")
            if (waitingSince > 0 && lastTranscriptEpoch > previousTranscriptEpoch) waitingSince = 0
            val timeout = waitingSince > 0 && SystemClock.elapsedRealtime() - waitingSince > 90_000
            transcript.text = getString(R.string.voice_transcript, when {
                timeout -> "尚未获得新的转写结果，请检查节点状态后刷新。"
                waitingSince > 0 -> "转写任务已提交，正在等待节点返回新的结果…"
                else -> status.optString("asr_transcript").ifBlank { "暂无转写结果" }
            }, status.optString("asr_error"))
            if (timeout) waitingSince = 0
            events.text = body.optJSONArray("events")?.objects()?.takeLast(8)?.joinToString("\n\n") {
                "${it.optString("phrase")} · ${if (it.optBoolean("success")) "成功" else "失败"}\n${it.optString("message")}"
            }.orEmpty().ifBlank { "暂无执行记录" }
        })
    }

    private fun buildForm() {
        building = true
        content.removeAllViews()
        fields.clear()
        val current = config!!
        val statusCard = ui.card(content)
        ui.text(statusCard, "识别状态", 20f)
        state = ui.text(statusCard, "")
        transcript = ui.text(statusCard, "暂无转写结果")
        ui.text(statusCard, "语音采集使用这台手机的麦克风。", 12f, true)
        val seconds = ui.field(statusCard, "转写时长（秒，1–15）", "6", InputType.TYPE_CLASS_NUMBER)
        ui.button(statusCard, "开始限时转写") {
            val duration = seconds.text.toString().toIntOrNull()
            if (duration == null || duration !in 1..15) notice.setText(R.string.voice_duration_invalid)
            else if (waitingSince != 0L) notice.text = "当前转写尚未返回，请等待结果"
            else request("正在提交转写任务…", statusCard, {
                api.request("/api/v1/voice/transcribe", "POST", JSONObject().put("window_ms", duration * 1000))
            }) {
                previousTranscriptEpoch = lastTranscriptEpoch
                waitingSince = SystemClock.elapsedRealtime()
                transcript.text = "转写任务已提交，正在等待节点返回新的结果…"
            }
        }
        ui.button(statusCard, "刷新语音状态") { refresh() }
        ui.button(statusCard, "管理语音模型") {
            startActivity(Intent(this, ModelsActivity::class.java).putExtra("server", server.url))
        }
        val form = ui.card(content)
        ui.text(form, "语音参数", 20f)
        enabled = ui.toggle(form, "启用语音", draft?.getBoolean("enabled") ?: current.optBoolean("enabled"))
        enabled.setOnCheckedChangeListener { _, _ -> if (!building) dirty = true }
        field(form, "capture_device", "录音设备", current)
        field(form, "playback_device", "播放设备", current)
        ui.text(form, "Android 节点由系统选择音频设备，上述 ALSA 设备名称仅用于 Linux 节点。", 12f, true)
        field(form, "playback_volume", "播放音量（0–100）", current, true)
        field(form, "capture_rate", "采样率（Hz）", current, true)
        field(form, "request_timeout_ms", "请求超时（毫秒）", current, true)
        field(form, "global_cooldown_ms", "全局冷却（毫秒）", current, true)
        field(form, "failure_reply", "失败回复", current)
        ui.button(form, "保存语音配置", true) { save() }
        ui.button(form, "重新加载语音配置") {
            val reload = { config = null; draft = null; dirty = false; refresh() }
            if (dirty) confirm("重新加载", "丢弃所有未保存的语音修改？", reload) else reload()
        }
        commands = ui.card(content)
        renderCommands()
        val eventCard = ui.card(content)
        ui.text(eventCard, "最近执行结果", 20f)
        events = ui.text(eventCard, "")
        dirty = draft?.getBoolean("dirty") ?: false
        draft = null
        building = false
    }

    private fun field(parent: LinearLayout, key: String, label: String, value: JSONObject, number: Boolean = false) {
        fields[key] = ui.field(parent, label, draft?.getString("field:$key") ?: value.optString(key),
            if (number) InputType.TYPE_CLASS_NUMBER else InputType.TYPE_CLASS_TEXT).also { input ->
            input.contentDescription = "语音参数 $key"
            input.addTextChangedListener(object : TextWatcher {
                override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
                override fun afterTextChanged(s: Editable?) {}
                override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {
                    if (!building) dirty = true
                }
            })
        }
    }
    private fun save() {
        runCatching {
            JSONObject(config.toString()).apply {
                put("enabled", enabled.isChecked)
                fields.forEach { (key, input) ->
                    val text = input.text.toString()
                    if (key in setOf("playback_volume", "capture_rate", "request_timeout_ms", "global_cooldown_ms")) {
                        put(key, text.toLongOrNull() ?: error("请填写有效的数字：${input.contentDescription}"))
                    } else put(key, text)
                }
            }
        }.onSuccess { body ->
            request("正在保存语音配置…", content, { api.request("/api/v1/voice", "PUT", body) }) {
                config = it.getJSONObject("config")
                dirty = false
                buildForm()
                notice.text = "语音配置已保存"
            }
        }.onFailure { notice.text = errorText(it) }
    }

    private fun renderCommands() {
        commands.removeAllViews()
        ui.text(commands, "触发命令", 20f)
        ui.text(commands, "命令修改暂存于本页，完成后点击「保存语音配置」。", 12f, true)
        val values = config!!.getJSONArray("commands")
        for (index in 0 until values.length()) {
            val command = values.getJSONObject(index)
            val id = command.getString("id")
            ui.text(commands, "${command.optString("phrase")} · ${if (command.optBoolean("enabled")) "启用" else "停用"}", 18f)
            ui.text(commands, id, 12f, true)
            ui.button(commands, "编辑命令") { editCommand(index) }.contentDescription = "编辑语音命令 $id"
            ui.button(commands, "测试命令") {
                if (dirty) notice.text = "请先保存语音配置，再测试已保存的命令"
                else testCommand(command)
            }.contentDescription = "测试语音命令 $id"
            ui.button(commands, "删除命令") {
                confirm("删除命令", "移除 $id？保存配置后生效。") {
                    values.remove(index); dirty = true; renderCommands()
                }
            }.contentDescription = "删除语音命令 $id"
        }
        ui.button(commands, "新增语音命令") {
            if (values.length() >= 32) notice.setText(R.string.voice_command_limit) else editCommand(null)
        }
        ui.button(commands, "保存语音配置", true) { save() }
    }

    private fun editCommand(index: Int?) {
        val values = config!!.getJSONArray("commands")
        val original = if (index != null) values.getJSONObject(index) else JSONObject()
            .put("id", "command-${System.currentTimeMillis()}").put("enabled", false)
            .put("phrase", "").put("phrases", JSONArray()).put("reply", "好的")
            .put("method", "GET").put("url", "").put("body", "")
            .put("boosting_score", 1.5).put("trigger_threshold", 0.05).put("cooldown_ms", 500)
        val form = ui.column(20)
        val commandEnabled = ui.toggle(form, "启用此命令", original.optBoolean("enabled"))
        val inputs = linkedMapOf<String, EditText>()
        val labels = linkedMapOf("id" to "命令 ID", "phrases" to "触发短语（每行一个，最多 8 个）",
            "reply" to "成功回复", "method" to "HTTP 方法（GET / POST）", "url" to "请求 URL",
            "body" to "POST JSON", "boosting_score" to "增强分数（0–10）",
            "trigger_threshold" to "触发阈值（0.01–0.95）", "cooldown_ms" to "命令冷却（毫秒）")
        labels.forEach { (key, label) ->
            val text = if (key == "phrases") original.optJSONArray("phrases")?.let { phrases ->
                (0 until phrases.length()).joinToString("\n") { phrases.getString(it) }
            } ?: original.optString("phrase") else original.optString(key)
            inputs[key] = ui.field(form, label, text).apply {
                contentDescription = "命令字段 $key"
                if (key in setOf("phrases", "body")) {
                    isSingleLine = false
                    inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE
                    minLines = 2
                }
            }
        }
        val feedback = ui.text(form, "", muted = true)
        val dialog = AlertDialog.Builder(this).setTitle(if (index == null) "新增语音命令" else "编辑语音命令")
            .setView(ui.scroll(form)).setNegativeButton("取消", null).setPositiveButton("暂存修改", null).create()
        dialog.setOnShowListener {
            dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener {
                runCatching {
                    JSONObject(original.toString()).apply {
                        put("enabled", commandEnabled.isChecked)
                        inputs.forEach { (key, input) ->
                            val text = input.text.toString().trim()
                            when (key) {
                                "phrases" -> {
                                    val phrases = text.lines().map(String::trim).filter(String::isNotBlank)
                                    require(phrases.size in 1..8) { "请填写 1–8 个触发短语" }
                                    put("phrases", JSONArray(phrases)); put("phrase", phrases.first())
                                }
                                "cooldown_ms" -> put(key, text.toLongOrNull() ?: error("命令冷却需为整数"))
                                "boosting_score", "trigger_threshold" -> {
                                    val number = text.toDoubleOrNull()
                                    require(number != null && number.isFinite()) { "请填写有效的分数或阈值" }
                                    put(key, number)
                                }
                                "method" -> put(key, text.uppercase(java.util.Locale.ROOT))
                                else -> put(key, text)
                            }
                        }
                    }
                }.onSuccess {
                    if (index == null) values.put(it) else values.put(index, it)
                    dirty = true; renderCommands(); dialog.dismiss()
                }.onFailure { feedback.text = errorText(it) }
            }
        }
        dialog.show()
    }

    private fun testCommand(command: JSONObject) {
        val form = ui.column(20)
        ui.text(form, "测试由当前节点执行，结果在「最近执行结果」查看。", muted = true)
        val callUrl = ui.toggle(form, "实际调用命令 URL", false)
        val speak = ui.toggle(form, "在节点播报回复", false)
        val feedback = ui.text(form, "")
        val dialog = AlertDialog.Builder(this).setTitle("测试 ${command.optString("phrase")}").setView(form)
            .setNegativeButton("取消", null).setPositiveButton("提交测试", null).create()
        dialog.setOnShowListener {
            dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener {
                if (!callUrl.isChecked && !speak.isChecked) {
                    feedback.setText(R.string.voice_test_action_required)
                } else if (!busy) {
                    val body = JSONObject().put("command_id", command.getString("id"))
                        .put("call_url", callUrl.isChecked).put("speak_reply", speak.isChecked)
                    dialog.dismiss()
                    request("正在提交命令测试…", content, {
                        api.request("/api/v1/voice/test", "POST", body)
                    }) { notice.text = "测试已排队，请查看最近执行结果" }
                }
            }
        }
        dialog.show()
    }
    override fun handleBack() {
        if (dirty) confirm("离开语音配置", "丢弃未保存的修改？") { finish() } else finish()
    }
    override fun onSaveInstanceState(outState: Bundle) {
        config?.let {
            outState.putString("config", it.toString())
            outState.putBoolean("dirty", dirty)
            outState.putBoolean("enabled", enabled.isChecked)
            fields.forEach { (key, input) -> outState.putString("field:$key", input.text.toString()) }
        }
        super.onSaveInstanceState(outState)
    }
}
