package site.gwghome.camerahub

import android.app.AlertDialog
import android.os.Bundle
import android.text.InputType
import android.view.View
import android.widget.AdapterView
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.Spinner
import android.widget.Switch
import org.json.JSONObject
import java.text.DateFormat
import java.util.Date
import java.util.UUID

class AirconActivity : NativePage() {
    override val pollMillis = 5_000L
    private lateinit var status: LinearLayout
    private lateinit var schedules: LinearLayout
    private lateinit var temperature: EditText
    private lateinit var delay: EditText
    private lateinit var autoOff: EditText
    private lateinit var eco: Switch
    private lateinit var mode: Spinner
    private val modes = listOf("cool", "dry", "heat", "auto")
    private val labels = listOf("制冷", "除湿", "制热", "自动")
    private val sendButtons = mutableListOf<Button>()
    private var overview = JSONObject()
    private var draft: Bundle? = null
    private var built = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (isFinishing) return
        draft = savedInstanceState
        page("空调控制")
        ui.button(content, "刷新空调状态") { refresh() }
        refresh()
    }
    override fun refresh() {
        request(work = { api.request("/api/v1/ir") }, done = { body ->
            overview = body.getJSONObject("aircon")
            if (!built) buildForm()
            status.removeAllViews()
            ui.text(status, "发送记录", 20f)
            val state = overview.getJSONObject("state")
            ui.text(status, if (!overview.optBoolean("state_known")) "尚无已知发送状态"
                else if (!state.optBoolean("power")) "上次发送：关闭"
                else "上次发送：${state.optInt("temperature_c")}°C · ${labels.getOrElse(modes.indexOf(state.optString("mode"))) { state.optString("mode") }}" +
                    if (state.optBoolean("eco")) " · ECO" else "")
            ui.text(status, "红外是单向控制，空调实际状态需现场确认。", 12f, true)
            ui.text(status, body.optJSONObject("ir")?.optString("detail").orEmpty(), 13f, true)
            sendButtons.forEach { it.isEnabled = body.optJSONObject("ir")?.optBoolean("available") == true }
            renderSchedules()
        })
    }

    private fun buildForm() {
        built = true
        content.removeAllViews()
        status = ui.card(content)
        val form = ui.card(content)
        val state = overview.getJSONObject("state")
        ui.text(form, "温度与模式", 20f)
        temperature = ui.field(form, "温度（°C）", draft?.getString("temperature") ?: state.optInt("temperature_c", 26).toString(), InputType.TYPE_CLASS_NUMBER)
        ui.text(form, "${overview.optInt("temperature_min", 17)}–${overview.optInt("temperature_max", 30)}°C", 12f, true)
        ui.text(form, "模式", muted = true)
        mode = Spinner(this).apply {
            adapter = ArrayAdapter(this@AirconActivity, android.R.layout.simple_spinner_dropdown_item, labels)
            contentDescription = "空调模式"
            setSelection(draft?.getInt("mode") ?: modes.indexOf(state.optString("mode")).coerceAtLeast(0))
        }
        form.addView(mode)
        eco = ui.toggle(form, "ECO 节能（仅制冷）", draft?.getBoolean("eco") ?: state.optBoolean("eco"))
        mode.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onNothingSelected(parent: AdapterView<*>?) {}
            override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) {
                eco.isEnabled = position == 0
                if (position != 0) eco.isChecked = false
            }
        }
        delay = ui.field(form, "延迟执行（分钟，留空立即）", draft?.getString("delay").orEmpty(), InputType.TYPE_CLASS_NUMBER)
        autoOff = ui.field(form, "开启后自动关闭（分钟，留空不设置）", draft?.getString("autoOff").orEmpty(), InputType.TYPE_CLASS_NUMBER)
        sendButtons += ui.button(form, "发送空调设置", true) {
            runCatching {
                val value = temperature.text.toString().toIntOrNull()
                require(value != null && value in overview.optInt("temperature_min", 17)..overview.optInt("temperature_max", 30)) { "请填写支持范围内的温度" }
                JSONObject().put("command", JSONObject().put("operation", "set")
                    .put("temperature_c", value).put("mode", modes[mode.selectedItemPosition])
                    .put("eco", mode.selectedItemPosition == 0 && eco.isChecked))
                    .put("delay_minutes", minutes(delay, overview.optInt("max_delay_minutes", 43200)))
                    .put("auto_off_minutes", minutes(autoOff, overview.optInt("max_auto_off_minutes", 10080)))
            }.onSuccess { body ->
                submit("发送空调设置", "请确认红外发射口对准空调。定时任务由当前节点执行，节点需保持运行。", body)
            }.onFailure { notice.text = errorText(it) }
        }
        sendButtons += ui.button(form, "立即关闭空调") {
            submit("立即关闭空调", "发送关机指令。已有定时任务仍会按计划执行，可在下方取消。",
                JSONObject().put("command", JSONObject().put("operation", "off")))
        }
        sendButtons += ui.button(form, "定时关闭空调") {
            runCatching {
                val minutes = minutes(delay, overview.optInt("max_delay_minutes", 43200))
                require(minutes != JSONObject.NULL) { "请填写延迟执行分钟数" }
                JSONObject().put("command", JSONObject().put("operation", "off")).put("delay_minutes", minutes)
            }.onSuccess { submit("定时关闭空调", "按填写的延迟时间关闭空调？", it) }
                .onFailure { notice.text = errorText(it) }
        }
        ui.button(content, "刷新空调状态") { refresh() }
        schedules = ui.card(content)
    }
    private fun minutes(field: EditText, max: Int): Any {
        val text = field.text.toString().trim()
        if (text.isEmpty()) return JSONObject.NULL
        val value = text.toIntOrNull()
        require(value != null && value in 1..max) { "定时分钟数应在 1–$max 之间" }
        return value
    }
    private fun submit(title: String, message: String, body: JSONObject) {
        confirm(title, message) {
            body.put("request_id", UUID.randomUUID().toString())
            request("正在提交空调指令…", content, {
                api.request("/api/v1/aircon/commands", "POST", body).getJSONObject("result")
            }) { result ->
                val messageText = result.optString("message")
                AlertDialog.Builder(this).setTitle("操作结果").setMessage(messageText)
                    .setPositiveButton("知道了", null).show()
                refresh()
            }
        }
    }
    private fun renderSchedules() {
        schedules.removeAllViews()
        ui.text(schedules, "定时任务", 20f)
        val tasks = overview.optJSONArray("schedules")?.objects().orEmpty()
        if (tasks.isEmpty()) ui.text(schedules, "暂无定时任务", muted = true)
        for (task in tasks) {
            val time = DateFormat.getDateTimeInstance().format(Date(task.optLong("execute_at_epoch") * 1000))
            ui.text(schedules, "$time · ${task.optString("status")}")
            ui.text(schedules, task.optString("message"), 12f, true)
            if (task.optString("status") == "pending") ui.button(schedules, "取消此任务") {
                submit("取消任务", "取消 $time 的定时任务？", JSONObject().put("command",
                    JSONObject().put("operation", "cancel_schedule").put("schedule_id", task.getString("id"))))
            }.contentDescription = "取消定时任务 ${task.getString("id")}"
        }
        if (tasks.any { it.optString("status") == "pending" }) ui.button(schedules, "取消全部定时任务") {
            submit("取消全部", "取消所有待执行的空调任务？", JSONObject().put("command", JSONObject().put("operation", "cancel_all")))
        }
    }
    override fun onSaveInstanceState(outState: Bundle) {
        if (built) {
            outState.putString("temperature", temperature.text.toString())
            outState.putString("delay", delay.text.toString())
            outState.putString("autoOff", autoOff.text.toString())
            outState.putInt("mode", mode.selectedItemPosition)
            outState.putBoolean("eco", eco.isChecked)
        }
        super.onSaveInstanceState(outState)
    }
}
