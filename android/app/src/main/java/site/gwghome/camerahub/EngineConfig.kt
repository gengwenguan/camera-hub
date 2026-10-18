package site.gwghome.camerahub

import android.content.Context
import android.hardware.ConsumerIrManager
import java.io.File
import java.security.SecureRandom
import java.util.UUID

class EngineConfig(private val context: Context) {
    val root = File(context.filesDir, "engine")
    val log = File(root, "logs/engine.log")
    // Separate stop/start generations: an old supervisor must never kill a new engine.
    val pidFile = File(root, "engine-${UUID.randomUUID()}.pid")
    private val preferences = context.getSharedPreferences("engine", Context.MODE_PRIVATE)
    val password: String
        get() = preferences.getString("password", null) ?: run {
            val bytes = ByteArray(12).also { SecureRandom().nextBytes(it) }
            val value = bytes.joinToString("") { "%02x".format(it) }
            preferences.edit().putString("password", value).apply()
            value
        }
    val port: Int get() = preferences.getInt("port", 8080)
    val url: String get() = "http://127.0.0.1:$port"

    fun savePort(port: Int) {
        require(port in 1024..65535) { "端口应为 1024–65535" }
        require(port != 8443) { "8443 已用于 HTTPS / MoQ，请选择其他 HTTP 端口" }
        require(port != 49081 && port != 49182) { "端口与内部语音/红外组件冲突" }
        preferences.edit().putInt("port", port).apply()
    }

    fun savePassword(password: String) {
        require(password.length >= 5) { "密码至少需要 5 个字符" }
        preferences.edit().putString("password", password).apply()
    }

    fun prepare(): ProcessBuilder {
        listOf(root, log.parentFile!!, File(root, "tmp"), File(root, "models")).forEach {
            check(it.isDirectory || it.mkdirs()) { "无法创建 App 数据目录" }
        }
        pidFile.delete()
        if (log.length() > 4 * 1024 * 1024) {
            val old = File(log.parentFile, "engine.previous.log")
            old.delete()
            log.renameTo(old)
        }
        val model = File(root, "models/yolox_nano.onnx")
        if (!model.isFile) {
            val temporary = File.createTempFile("yolox_nano-", ".part", File(root, "models"))
            try {
                context.assets.open("yolox_nano.onnx").use { source ->
                    temporary.outputStream().use { source.copyTo(it) }
                }
                check(temporary.renameTo(model)) { "无法安装 AI 模型" }
            } finally {
                temporary.delete()
            }
        }
        val nativeDir = File(context.applicationInfo.nativeLibraryDir)
        val executable = File(nativeDir, "libcamera_hub_exec.so")
        check(executable.canExecute()) { "APK 未包含可执行的 ARM64 服务，请重新构建原生依赖" }
        return ProcessBuilder(executable.path, "server").apply {
            directory(root)
            redirectErrorStream(true)
            redirectOutput(ProcessBuilder.Redirect.appendTo(log))
            environment().putAll(mapOf(
                "LD_LIBRARY_PATH" to nativeDir.path,
                "TMPDIR" to File(root, "tmp").path,
                "CAMERA_HUB_PID_FILE" to pidFile.path,
                "CAMERA_HUB_ANDROID_SOCKET" to File(root, "hardware.sock").path,
                "CAMERA_HUB_ANDROID_IR" to if (
                    context.getSystemService(ConsumerIrManager::class.java)?.hasIrEmitter() == true
                ) "1" else "0",
                "CAMERA_HUB_FFMPEG" to File(nativeDir, "libffmpeg_exec.so").path,
                "CAMERA_HUB_WEB_USERNAME" to "admin",
                "CAMERA_HUB_WEB_PASSWORD" to password,
                "CAMERA_HUB_BIND" to "[::]:$port",
                "CAMERA_HUB_TLS_BIND" to "[::]:8443",
                "CAMERA_HUB_MOQ_BIND" to "[::]:8443",
                "CAMERA_HUB_COMPONENT_MANAGER_ENABLED" to "true",
                "CAMERA_HUB_LOG_DIR" to log.parentFile!!.path,
                "CAMERA_HUB_IR_BIND" to "127.0.0.1:49182",
                "CAMERA_HUB_IR_URL" to "http://127.0.0.1:49182",
                "CAMERA_HUB_IR_DEVICE" to "Android ConsumerIrManager",
                "CAMERA_HUB_TTS_BIND" to "127.0.0.1:49081",
                "CAMERA_HUB_TTS_URL" to "http://127.0.0.1:49081",
                "CAMERA_HUB_TTS_TOKEN" to password,
                "CAMERA_HUB_VOICE_LIB_DIR" to nativeDir.path,
                "CAMERA_HUB_VOICE_MODEL_DIR" to File(root, "models/kws").path,
                "CAMERA_HUB_ASR_MODEL_DIR" to File(root, "models/asr").path,
                "CAMERA_HUB_TTS_MODEL_DIR" to File(root, "models/tts").path,
                "CAMERA_HUB_TTS_VOCODER" to File(root, "models/vocos_24khz.onnx").path,
                "CAMERA_HUB_AI_RUNTIME" to File(nativeDir, "libonnxruntime.so").path,
                "CAMERA_HUB_AI_MODEL" to model.path,
            ))
        }
    }
}
