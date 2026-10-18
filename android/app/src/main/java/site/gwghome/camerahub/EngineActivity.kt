package site.gwghome.camerahub

import android.Manifest
import android.app.AlertDialog
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.text.InputType
import android.view.View
import android.widget.Button
import android.widget.CheckBox
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.widget.Toast
import java.io.RandomAccessFile
import java.text.DateFormat
import java.util.Date
import java.util.concurrent.Executors

class EngineActivity : HubActivity() {
    private lateinit var config: EngineConfig
    private lateinit var state: EngineState
    private lateinit var status: TextView
    private lateinit var recoveryDetail: TextView
    private lateinit var features: LinearLayout
    private lateinit var resumeVoice: Button
    private lateinit var autoRecovery: CheckBox
    private lateinit var startOnBoot: CheckBox
    private lateinit var port: EditText
    private lateinit var start: Button
    private lateinit var stop: Button
    private val handler = Handler(Looper.getMainLooper())
    private val checks = Executors.newSingleThreadExecutor()
    private var visible = false
    private var collecting = false
    private var refreshGeneration = 0
    private val refresh = object : Runnable {
        override fun run() {
            refreshState()
            handler.postDelayed(this, 2000)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        config = EngineConfig(this)
        state = EngineState(this)
        val ui = NativeUi(this)
        val wrapper = ui.column(16)
        var content = wrapper
        fun text(resource: Int, size: Float = 15f) = ui.text(content, getString(resource), size)
        fun button(resource: Int, action: () -> Unit) =
            ui.button(content, getString(resource), resource == R.string.engine_start, action)
        ui.button(wrapper, "返回") { finish() }
        text(R.string.local_engine, 28f)
        text(R.string.engine_description)
        content = ui.card(wrapper)
        status = text(R.string.engine_stopped, 18f)
        port = ui.field(content, getString(R.string.engine_port),
            savedInstanceState?.getString("port") ?: config.port.toString(), InputType.TYPE_CLASS_NUMBER)
        button(R.string.engine_change_password) { showPasswordDialog() }
        text(R.string.engine_access_hint)
        start = button(R.string.engine_start) { startEngine() }
        stop = button(R.string.engine_stop) {
            // Persist first: stopping during a pending system restart must cancel it too.
            state.update { it.manualStop() }
            if (EngineService.active) {
                startService(Intent(this, EngineService::class.java).setAction(EngineService.STOP))
            } else state.recordStatus("本机服务已停止")
            refreshState()
        }
        button(R.string.engine_console) {
            startActivity(Intent(this, DashboardActivity::class.java).putExtra("server", config.url))
        }
        button(R.string.engine_logs) { showLogs() }
        content = ui.card(wrapper)
        text(R.string.engine_recovery_title, 21f)
        autoRecovery = CheckBox(this).apply {
            setText(R.string.engine_auto_recovery)
            isChecked = state.policy.enabled
            content.addView(this)
        }
        startOnBoot = CheckBox(this).apply {
            setText(R.string.engine_start_on_boot)
            isChecked = state.policy.bootEnabled
            isEnabled = autoRecovery.isChecked
            content.addView(this)
        }
        autoRecovery.setOnCheckedChangeListener { _, checked ->
            state.update { it.copy(enabled = checked) }
            startOnBoot.isEnabled = checked
            notifyPolicyChanged()
        }
        startOnBoot.setOnCheckedChangeListener { _, checked ->
            state.update { it.copy(bootEnabled = checked) }
        }
        recoveryDetail = text(R.string.engine_stopped)
        text(R.string.engine_background_hint)
        content = wrapper
        text(R.string.engine_capabilities_title, 21f)
        text(R.string.engine_capabilities_hint)
        features = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            content.addView(this)
        }
        content = ui.card(wrapper)
        resumeVoice = button(R.string.engine_resume_voice) {
            if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
                requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO), 21)
            } else launchEngine(EngineService.RESUME_VOICE)
        }
        button(R.string.engine_notification_settings) {
            if (Build.VERSION.SDK_INT >= 33 &&
                checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
            ) requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 22)
            else openSettings(Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                .putExtra(Settings.EXTRA_APP_PACKAGE, packageName))
        }
        button(R.string.engine_app_settings) { openSettings(appSettings()) }
        button(R.string.engine_battery_settings) {
            openSettings(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
        }
        button(R.string.engine_tts_settings) {
            openSettings(Intent("com.android.settings.TTS_SETTINGS"))
        }
        text(R.string.engine_models_hint)
        val scroll = ui.scroll(wrapper).apply { id = R.id.root }
        setContentView(scroll)
        fitSystemBars()
        if (savedInstanceState == null && intent.getBooleanExtra("change_password", false)) {
            handler.post { showPasswordDialog() }
        }
    }

    private fun refreshState() {
        val running = EngineService.active
        val policy = state.policy
        status.text = if (running) EngineService.status else if (policy.desiredRunning)
            getString(R.string.engine_previous_status, state.status) else state.status
        start.isEnabled = !running
        stop.isEnabled = running || policy.desiredRunning
        port.isEnabled = !running
        resumeVoice.isEnabled = running
        startOnBoot.isEnabled = policy.enabled
        val failure = state.lastFailure
        recoveryDetail.text = buildString {
            append("连续恢复次数：${policy.attempts}/${RecoveryPolicy.MAX_ATTEMPTS}")
            if (policy.attempts >= RecoveryPolicy.MAX_ATTEMPTS) append(" · 故障后需手动启动")
            if (!policy.desiredRunning) append("\n当前未请求运行，不会自动恢复")
            if (failure.isNotEmpty()) {
                append("\n最近异常（${DateFormat.getDateTimeInstance().format(Date(state.lastFailureAt))}）：\n$failure")
            }
        }
        refreshFeatures()
    }

    private fun refreshFeatures() {
        if (!visible || collecting) return
        collecting = true
        val generation = refreshGeneration
        checks.execute {
            val rows = runCatching { FeatureStatus.collect(applicationContext, config) }.getOrElse {
                listOf(Feature("状态检查失败", it.message.orEmpty(), true))
            }
            handler.post {
                collecting = false
                if (visible && !isDestroyed && generation == refreshGeneration) showFeatures(rows)
            }
        }
    }

    private fun showFeatures(rows: List<Feature>) {
        while (features.childCount > rows.size) features.removeViewAt(features.childCount - 1)
        rows.forEachIndexed { index, feature ->
            val row = if (index < features.childCount) features.getChildAt(index) as TextView
            else TextView(this).also {
                val ui = NativeUi(this)
                it.textSize = 15f
                it.setPadding(ui.dp(16), ui.dp(14), ui.dp(16), ui.dp(14))
                it.setLineSpacing(ui.dp(4).toFloat(), 1f)
                it.background = ui.shape(R.color.surface)
                features.addView(it, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = ui.dp(10) })
            }
            row.text = getString(R.string.engine_feature_detail, feature.title, feature.detail)
            row.setTextColor(getColor(if (feature.attention) R.color.warning else R.color.ink))
        }
    }

    private fun notifyPolicyChanged() {
        if (EngineService.active) startService(Intent(this, EngineService::class.java)
            .setAction(EngineService.POLICY_CHANGED))
        refreshState()
    }

    private fun appSettings() =
        Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName"))

    private fun openSettings(intent: Intent) {
        try {
            startActivity(intent)
        } catch (_: android.content.ActivityNotFoundException) {
            runCatching { startActivity(appSettings()) }
                .onFailure { toast(R.string.engine_settings_missing) }
        }
    }

    private fun startEngine() {
        try {
            config.savePort(port.text.toString().toIntOrNull() ?: 0)
        } catch (error: IllegalArgumentException) {
            Toast.makeText(this, error.message, Toast.LENGTH_LONG).show()
            return
        }
        val permissions = mutableListOf<String>()
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            permissions += Manifest.permission.RECORD_AUDIO
        }
        if (Build.VERSION.SDK_INT >= 33 &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            permissions += Manifest.permission.POST_NOTIFICATIONS
        }
        if (permissions.isEmpty()) launchEngine() else requestPermissions(permissions.toTypedArray(), 20)
    }

    private fun showPasswordDialog() {
        val ui = NativeUi(this)
        val form = ui.column(20)
        ui.text(form, "账号固定为 admin。新密码仅保存在 App 私有配置中。", muted = true)
        val first = ui.field(form, "新密码（至少 5 个字符）", secret = true)
        val second = ui.field(form, "再次输入新密码", secret = true)
        val error = ui.text(form, "", 13f).apply {
            setTextColor(getColor(R.color.warning))
            visibility = View.GONE
        }
        val dialog = AlertDialog.Builder(this)
            .setTitle(R.string.engine_change_password)
            .setView(form)
            .setNegativeButton("取消", null)
            .setPositiveButton("保存", null)
            .create()
        dialog.setOnShowListener {
            dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener {
                val wanted = first.text.toString()
                val repeated = second.text.toString()
                val message = when {
                    wanted.length < 5 -> "密码至少需要 5 个字符"
                    wanted != repeated -> "两次输入的密码不一致"
                    else -> null
                }
                if (message != null) {
                    error.text = message
                    error.visibility = View.VISIBLE
                    return@setOnClickListener
                }
                config.savePassword(wanted)
                HubSessions.clear(ServerAddress.parse(config.url))
                if (EngineService.active) {
                    startService(Intent(this, EngineService::class.java).setAction(EngineService.RESTART))
                    Toast.makeText(this, "密码已保存，正在重新加载本机服务", Toast.LENGTH_LONG).show()
                } else {
                    Toast.makeText(this, "密码已保存，将在下次启动服务时生效", Toast.LENGTH_LONG).show()
                }
                dialog.dismiss()
            }
        }
        dialog.show()
    }

    private fun launchEngine(action: String = EngineService.START) {
        if (action == EngineService.RESUME_VOICE && !EngineService.active) return
        try {
            startForegroundService(Intent(this, EngineService::class.java).setAction(action))
            start.isEnabled = false
        } catch (error: RuntimeException) {
            val failure = "系统未允许启动服务：${error.message}"
            state.recordFailure(failure)
            state.recordStatus(failure)
            Toast.makeText(this, failure, Toast.LENGTH_LONG).show()
        }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == 20) {
            if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
                toast(R.string.engine_no_microphone)
            }
            launchEngine()
        } else if (requestCode == 21) {
            if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) {
                launchEngine(EngineService.RESUME_VOICE)
            } else {
                toast(R.string.engine_permission_settings_hint)
            }
        } else if (requestCode == 22 &&
            Build.VERSION.SDK_INT >= 33 &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            toast(R.string.engine_permission_settings_hint)
            openSettings(Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                .putExtra(Settings.EXTRA_APP_PACKAGE, packageName))
        }
    }

    private fun showLogs() {
        val logs = runCatching {
            RandomAccessFile(config.log, "r").use {
                val length = minOf(it.length(), 24_000L).toInt()
                it.seek(it.length() - length)
                ByteArray(length).also(it::readFully).toString(Charsets.UTF_8)
            }
        }.getOrElse { getString(R.string.engine_no_logs) }
        val text = TextView(this).apply {
            this.text = logs
            textSize = 12f
            setTextIsSelectable(true)
            setPadding(24, 12, 24, 12)
        }
        AlertDialog.Builder(this).setTitle(R.string.engine_logs)
            .setView(ScrollView(this).apply { addView(text) })
            .setPositiveButton(android.R.string.ok, null).show()
    }

    override fun onResume() {
        super.onResume()
        visible = true
        handler.removeCallbacks(refresh)
        handler.post(refresh)
    }

    override fun onPause() {
        visible = false
        refreshGeneration++
        handler.removeCallbacks(refresh)
        super.onPause()
    }

    override fun onDestroy() {
        checks.shutdownNow()
        handler.removeCallbacksAndMessages(null)
        super.onDestroy()
    }

    override fun onSaveInstanceState(outState: Bundle) {
        outState.putString("port", port.text.toString())
        super.onSaveInstanceState(outState)
    }
}
