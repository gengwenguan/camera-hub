package site.gwghome.camerahub

import android.Manifest
import android.annotation.SuppressLint
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import android.os.SystemClock
import android.system.Os
import android.system.OsConstants
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class EngineService : Service() {
    companion object {
        const val START = "site.gwghome.camerahub.START"
        const val STOP = "site.gwghome.camerahub.STOP"
        const val RESTORE = "site.gwghome.camerahub.RESTORE"
        const val RESTART = "site.gwghome.camerahub.RESTART"
        const val RESUME_VOICE = "site.gwghome.camerahub.RESUME_VOICE"
        const val POLICY_CHANGED = "site.gwghome.camerahub.POLICY_CHANGED"
        @Volatile var active = false
            private set
        @Volatile var microphoneEnabled = false
            private set
        @Volatile var microphoneError: String? = null
            private set
        @SuppressLint("StaticFieldLeak") // Application context only; closed and cleared in onDestroy.
        @Volatile var hardware: HardwareBridge? = null
            private set
        @Volatile var status = "本机服务未启动"
            private set
        const val CHANNEL = "engine"
        private const val NOTIFICATION = 10
    }

    private val main = Handler(Looper.getMainLooper())
    private val processLock = Any()
    private val stopSignal = CountDownLatch(1)
    private lateinit var config: EngineConfig
    private lateinit var state: EngineState
    private var wakeLock: PowerManager.WakeLock? = null
    @Volatile private var process: Process? = null
    private var processStopDeadline = 0L // Guarded by processLock; shared by all stop paths.
    @Volatile private var stopping = false
    @Volatile private var restartRequested = false
    private var destroyed = false

    override fun onCreate() {
        super.onCreate()
        config = EngineConfig(this)
        state = EngineState(this)
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(CHANNEL, "本机 camera-hub 服务", NotificationManager.IMPORTANCE_LOW),
        )
    }

    @SuppressLint("WakelockTimeout") // Released with this explicitly started foreground service.
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == STOP) {
            state.update { it.manualStop() }
            stopEngine()
            return START_NOT_STICKY
        }
        if (intent?.action == RESTART && active) {
            state.update { it.manualStart() }
            restartRequested = true
            setStatus("正在重新加载本机服务…")
            requestProcessStop()
            return restartMode()
        }
        if (intent?.action == POLICY_CHANGED) {
            if (!active) stopSelf()
            return restartMode()
        }
        if (stopping) return START_NOT_STICKY
        val manual = intent?.action == START || intent?.action == RESTART ||
            intent?.action == RESUME_VOICE
        if (active) {
            if (manual) enableMicrophone()
            return restartMode()
        }
        if (!manual && intent != null && intent.action != RESTORE) {
            stopSelf()
            return START_NOT_STICKY
        }
        val delay = if (manual) {
            state.update { it.manualStart() }
            0L
        } else {
            state.reserveRetry() ?: run {
                setStatus(when {
                    !state.policy.desiredRunning -> "本机服务已停止"
                    !state.policy.enabled -> "自动恢复已关闭，请手动启动"
                    else -> "连续恢复达到上限，已暂停；请查看日志后手动启动"
                })
                stopSelf()
                return START_NOT_STICKY
            }
        }
        active = true
        microphoneEnabled = false
        microphoneError = null
        setStatus(if (manual) "正在启动本机服务…" else "正在恢复本机服务…")
        try {
            // Boot and sticky recreation restore only the network/IR service. Microphone
            // eligibility is obtained separately from a visible Activity.
            foreground(false)
            if (manual) enableMicrophone()
            check(config.root.isDirectory || config.root.mkdirs()) { "无法创建 App 数据目录" }
            hardware = HardwareBridge(applicationContext, config.root) { microphoneEnabled }
            wakeLock = getSystemService(PowerManager::class.java)
                .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "camera-hub:engine")
                .also { it.acquire() }
            Thread({ supervise(delay) }, "camera-hub-engine").start()
        } catch (error: Exception) {
            val failure = "启动失败：${error.message}"
            state.recordFailure(failure)
            setStatus(failure)
            stopSelf()
            return START_NOT_STICKY
        }
        return restartMode()
    }

    private fun restartMode() =
        if (state.policy.enabled && state.policy.desiredRunning) START_STICKY else START_NOT_STICKY

    private fun foreground(microphone: Boolean) {
        if (Build.VERSION.SDK_INT >= 29) {
            var types = ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE
            if (Build.VERSION.SDK_INT >= 30 && microphone) {
                types = types or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
            }
            startForeground(NOTIFICATION, notification(status), types)
        } else {
            startForeground(NOTIFICATION, notification(status))
        }
    }

    private fun enableMicrophone() {
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) return
        try {
            foreground(true)
            microphoneEnabled = true
            microphoneError = null
        } catch (error: RuntimeException) {
            microphoneEnabled = false
            microphoneError = "语音恢复失败：${error.message}"
        }
        updateNotification()
    }

    private fun supervise(firstDelay: Long) {
        var delay = firstDelay
        try {
            while (!stopping) {
                if (delay > 0) {
                    setStatus("将在 ${delay / 1000} 秒后恢复 · 第 ${state.policy.attempts}/${RecoveryPolicy.MAX_ATTEMPTS} 次")
                    val deadline = SystemClock.elapsedRealtime() + delay
                    while (!stopping && !restartRequested &&
                        state.policy.enabled && state.policy.desiredRunning &&
                        SystemClock.elapsedRealtime() < deadline
                    ) {
                        stopSignal.await(minOf(250L, deadline - SystemClock.elapsedRealtime()).coerceAtLeast(1),
                            TimeUnit.MILLISECONDS)
                    }
                    if (stopping) break
                    if (restartRequested) {
                        restartRequested = false
                        delay = 0
                        continue
                    }
                    if (!state.policy.enabled || !state.policy.desiredRunning) {
                        setStatus("自动恢复已关闭，服务已停止")
                        break
                    }
                }
                try {
                    setStatus("正在启动本机服务…")
                    runEngine()
                } catch (error: Exception) {
                    if (!stopping) {
                        val failure = "服务异常：${error.message}"
                        state.recordFailure(failure)
                        setStatus(failure)
                    }
                } finally {
                    cleanupProcess()
                }
                if (stopping) break
                if (restartRequested) {
                    restartRequested = false
                    delay = 0
                    continue
                }
                val nextDelay = state.reserveRetry()
                if (nextDelay == null) {
                    setStatus(if (state.policy.enabled) "连续恢复失败，已暂停；请查看日志后手动启动"
                        else "服务已停止，自动恢复未开启")
                    break
                }
                delay = nextDelay
            }
        } finally {
            main.post { if (!destroyed) stopSelf() }
        }
    }

    private fun runEngine() {
        val builder = config.prepare()
        val child = synchronized(processLock) {
            if (stopping) return
            builder.start().also {
                process = it
                processStopDeadline = 0L
            }
        }
        if (restartRequested) {
            requestProcessStop()
            return
        }
        var ready = false
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(20)
        while (!stopping && !restartRequested && child.isAlive && System.nanoTime() < deadline) {
            if (runCatching { HealthClient.check(ServerAddress.parse(config.url), 1500) }.isSuccess) {
                ready = true
                break
            }
            if (child.waitFor(250, TimeUnit.MILLISECONDS)) break
        }
        if (stopping || restartRequested) return
        check(ready) { "HTTP 服务未就绪，请查看本机日志" }
        setStatus("本机服务运行中 · ${config.url}")
        var healthySince = SystemClock.elapsedRealtime()
        var nextHealthCheck = healthySince + 15_000
        var failures = 0
        // Observe stop/exit promptly without increasing the HTTP polling frequency.
        while (!stopping && !restartRequested && !child.waitFor(250, TimeUnit.MILLISECONDS)) {
            if (stopping) break
            if (SystemClock.elapsedRealtime() < nextHealthCheck) continue
            if (runCatching { HealthClient.check(ServerAddress.parse(config.url), 1500) }.isSuccess) {
                failures = 0
                if (SystemClock.elapsedRealtime() - healthySince >= RecoveryPolicy.STABLE_MS &&
                    state.policy.attempts > 0
                ) state.update { it.stable() }
            } else {
                healthySince = SystemClock.elapsedRealtime()
                check(++failures < 3) { "本机 HTTP 健康检查连续失败 3 次" }
            }
            nextHealthCheck = SystemClock.elapsedRealtime() + 15_000
        }
        if (!stopping && !restartRequested) {
            error("服务已退出（${child.exitValue()}），请查看本机日志")
        }
    }

    private fun requestProcessStop() = synchronized(processLock) {
        if (process != null && processStopDeadline == 0L) {
            processStopDeadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(10)
            process?.destroy()
        }
    }

    private fun cleanupProcess() {
        val (child, deadline) = synchronized(processLock) {
            requestProcessStop()
            process to processStopDeadline
        }
        if (child != null && child.isAlive) {
            child.waitFor((deadline - System.nanoTime()).coerceAtLeast(0), TimeUnit.NANOSECONDS)
        }
        synchronized(processLock) {
            // Also kill orphan workers when the core exits by itself.
            killGroup()
            child?.destroyForcibly()
            process = null
        }
    }

    private fun stopEngine() {
        stopping = true
        microphoneEnabled = false
        stopSignal.countDown()
        setStatus("正在停止本机服务…")
        requestProcessStop()
        if (!active) stopSelf()
    }

    private fun killGroup() {
        val pid = runCatching { config.pidFile.readText().trim().toInt() }.getOrNull()
        if (pid != null && pid > 1) runCatching { Os.kill(-pid, OsConstants.SIGKILL) }
        config.pidFile.delete()
    }

    private fun notification(text: String): Notification {
        val open = PendingIntent.getActivity(this, 0, Intent(this, EngineActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val stop = PendingIntent.getService(this, 1,
            Intent(this, EngineService::class.java).setAction(STOP), PendingIntent.FLAG_IMMUTABLE)
        return Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_camera_hub)
            .setContentTitle("camera-hub 本机服务")
            .setContentText(if (microphoneEnabled) text else "$text · 语音待恢复")
            .setContentIntent(open)
            .setOngoing(true)
            .apply {
                if (!microphoneEnabled) addAction(Notification.Action.Builder(null, "恢复语音", open).build())
            }
            .addAction(Notification.Action.Builder(null, "停止服务", stop).build())
            .build()
    }

    private fun setStatus(value: String) {
        status = value
        state.recordStatus(value)
        updateNotification()
    }

    private fun updateNotification() {
        main.post {
            if (active && !destroyed && !stopping) {
                getSystemService(NotificationManager::class.java).notify(NOTIFICATION, notification(status))
            }
        }
    }

    override fun onDestroy() {
        destroyed = true
        stopping = true
        stopSignal.countDown()
        microphoneEnabled = false
        synchronized(processLock) {
            killGroup()
            process?.destroyForcibly()
            process = null
        }
        hardware?.close()
        hardware = null
        wakeLock?.let { if (it.isHeld) it.release() }
        wakeLock = null
        active = false
        if (!state.policy.desiredRunning) setStatus("本机服务已停止")
        else if (status.startsWith("本机服务运行中") || status.startsWith("正在启动")) {
            setStatus("服务已中断，等待系统恢复或手动启动")
        }
        stopForeground(STOP_FOREGROUND_REMOVE)
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null
}
