package site.gwghome.camerahub

import android.Manifest
import android.app.ActivityManager
import android.app.NotificationManager
import android.content.Context
import android.content.pm.PackageManager
import android.hardware.ConsumerIrManager
import android.media.AudioManager
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.Build
import android.os.PowerManager
import android.os.StatFs
import android.text.format.Formatter
import java.io.File

data class Feature(val title: String, val detail: String, val attention: Boolean = false)

/** Passive checks only: opening the status page never records audio or transmits IR. */
object FeatureStatus {
    fun microphoneBlock(context: Context): String? = when {
        !context.packageManager.hasSystemFeature(PackageManager.FEATURE_MICROPHONE) ->
            "不可用 · 设备未声明麦克风"
        context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED ->
            "待授权 · 请允许麦克风权限"
        context.getSystemService(AudioManager::class.java).isMicrophoneMute ->
            "系统麦克风已静音 · 请检查系统隐私开关或麦克风设置"
        else -> null
    }

    fun collect(context: Context, config: EngineConfig): List<Feature> {
        val native = File(context.applicationInfo.nativeLibraryDir)
        val runtime = listOf("libonnxruntime.so", "libsherpa-onnx-c-api.so")
            .all { File(native, it).isFile }
        fun model(title: String, directory: String, names: List<String>, extra: Boolean = true): Feature {
            val ready = runtime && extra && names.all { File(config.root, "models/$directory/$it").isFile }
            return Feature(title, if (ready) "资源齐全 · 在控制台启用，运行状态以组件页面为准"
                else "资源未齐 · 在控制台「组件与资源」安装", !ready)
        }
        val microphoneIssue = microphoneBlock(context)
        val bridge = EngineService.hardware
        val microphone = when {
            microphoneIssue != null -> Feature("麦克风", microphoneIssue, true)
            !EngineService.active -> Feature("麦克风", "权限已授予 · 启动服务后可启用语音")
            !EngineService.microphoneEnabled -> Feature("麦克风",
                EngineService.microphoneError ?: "待恢复语音 · 点击下方「授权 / 恢复语音」", true)
            bridge?.capturing == true && bridge.captureSilenced ->
                Feature("麦克风", "采集被系统静音 · 检查隐私开关或其他 App 的录音占用", true)
            bridge?.capturing == true -> Feature("麦克风", "正在采集 · Android AudioRecord")
            bridge?.captureError != null -> Feature("麦克风", "采集失败 · ${bridge.captureError}", true)
            else -> Feature("麦克风", "权限与前台服务已就绪 · 等待语音组件采集")
        }
        val ir = runCatching { context.getSystemService(ConsumerIrManager::class.java)?.hasIrEmitter() == true }
        val infrared = ir.fold(
            onSuccess = { available -> Feature("红外", if (available)
                "系统提供红外发射器 · 无需 Root，发射效果需实际验证"
                else "不可用 · 系统未提供红外发射器", !available) },
            onFailure = { Feature("红外", "检查失败 · ${it.message}", true) },
        )
        val notifications = context.getSystemService(NotificationManager::class.java)
        val notificationAllowed = notifications.areNotificationsEnabled() &&
            notifications.getNotificationChannel(EngineService.CHANNEL)?.importance != NotificationManager.IMPORTANCE_NONE
        val power = context.getSystemService(PowerManager::class.java)
        val restricted = Build.VERSION.SDK_INT >= 28 &&
            context.getSystemService(ActivityManager::class.java).isBackgroundRestricted
        val optimized = !power.isIgnoringBatteryOptimizations(context.packageName)
        val executables = listOf("libcamera_hub_exec.so", "libffmpeg_exec.so")
            .all { File(native, it).canExecute() }
        val ai = File(native, "libonnxruntime.so").isFile &&
            (File(config.root, "models/yolox_nano.onnx").isFile ||
                context.assets.list("")?.contains("yolox_nano.onnx") == true)
        val network = context.getSystemService(ConnectivityManager::class.java)
        val connected = network.activeNetwork
        val validated = network.getNetworkCapabilities(connected)
            ?.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) == true
        val addresses = connected?.let { network.getLinkProperties(it) }?.linkAddresses
            ?.joinToString("、") { it.address.hostAddress.orEmpty() }.orEmpty()
        val disk = context.filesDir
        val writable = disk.canWrite()
        return listOf(
            Feature("本机服务 / 录像", if (executables)
                "主服务与 FFmpeg 已打包 · 高位端口运行，无需 Root"
                else "原生程序缺失或不可执行 · 请重新安装完整 APK", !executables),
            microphone,
            infrared,
            Feature("系统中文播报", bridge?.ttsDescription ?: "启动服务后检查系统 TTS 引擎"),
            Feature("AI 推理", if (ai) "运行库与内置模型已打包 · 在控制台启用，使用 CPU"
                else "模型或运行库缺失 · 请重新安装完整 APK", !ai),
            model("语音唤醒 KWS", "kws", listOf("tokens.txt") +
                listOf("encoder", "decoder", "joiner").map { "$it-epoch-12-avg-2-chunk-16-left-64.int8.onnx" }),
            model("语音识别 ASR", "asr", listOf("tokens.txt") +
                listOf("encoder", "decoder", "joiner").map { "$it-epoch-99-avg-1.int8.onnx" }),
            model("本地音色合成", "tts", listOf("tokens.txt", "encoder.int8.onnx", "decoder.int8.onnx", "lexicon.txt"),
                File(config.root, "models/tts/espeak-ng-data").isDirectory &&
                    File(config.root, "models/vocos_24khz.onnx").isFile),
            Feature("通知", if (notificationAllowed) "通知已允许"
                else "通知被关闭 · 请允许通知及「本机 camera-hub 服务」渠道", !notificationAllowed),
            Feature("后台与电池", when {
                restricted -> "系统限制后台运行 · 请在 App 设置中允许后台活动"
                optimized -> "仍受电池优化影响 · 可在电池设置中取消优化"
                else -> "已取消电池优化 · 厂商自启动与后台限制仍需在系统中设置"
            }, restricted || optimized),
            Feature("网络 / DDNS", when {
                connected == null -> "无默认网络 · 本机管理仍可访问，公网访问与 DDNS 待网络恢复"
                validated -> "网络已连接 · $addresses"
                else -> "网络已连接，外网未验证 · $addresses"
            }, connected == null),
            Feature("录像与模型存储", if (writable)
                "App 私有目录可写 · 可用 ${Formatter.formatFileSize(context, StatFs(disk.path).availableBytes)}"
                else "App 数据目录不可写", !writable),
            Feature("系统监控 / 加速", "部分系统统计受 Android 限制；GPU / DSP 加速暂未接入"),
        )
    }
}
