package site.gwghome.camerahub

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.hardware.ConsumerIrManager
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaPlayer
import android.media.MediaRecorder
import android.net.LocalServerSocket
import android.net.LocalSocket
import android.net.LocalSocketAddress
import android.net.ConnectivityManager
import android.os.Build
import android.os.Bundle
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import org.json.JSONObject
import java.io.Closeable
import java.io.File
import java.util.Collections
import java.util.Locale
import java.net.Inet6Address
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/** One JSON request per connection. Capture replies with a JSON header then PCM16LE.
 * The socket lives in private App storage and additionally checks the caller UID.
 */
class HardwareBridge(
    private val context: Context,
    private val root: File,
    private val captureAllowed: () -> Boolean,
) : Closeable {
    val socketFile = File(root, "hardware.sock")
    private val listener = LocalSocket()
    private val server: LocalServerSocket
    private val clients = Collections.synchronizedSet(mutableSetOf<LocalSocket>())
    private val executor = Executors.newFixedThreadPool(4)
    private val playbackLock = Any()
    private val captureBusy = AtomicBoolean(false)
    private val captureLock = Any()
    private var activeRecorder: AudioRecord? = null // Guarded by captureLock.
    @Volatile var capturing = false
        private set
    @Volatile var captureSilenced = false
        private set
    @Volatile var captureError: String? = null
        private set
    @Volatile var ttsDescription = "正在检查系统中文语音引擎"
        private set
    @Volatile private var closed = false
    private val ttsReady = CountDownLatch(1)
    @Volatile private var ttsStatus = TextToSpeech.ERROR
    private val tts = TextToSpeech(context) { status ->
        ttsStatus = status
        ttsReady.countDown()
    }

    init {
        socketFile.delete()
        listener.bind(LocalSocketAddress(socketFile.path, LocalSocketAddress.Namespace.FILESYSTEM))
        server = LocalServerSocket(listener.fileDescriptor)
        executor.execute {
            runCatching {
                ttsDescription = if (!ttsReady.await(10, TimeUnit.SECONDS) || ttsStatus != TextToSpeech.SUCCESS) {
                    "不可用 · 请安装中文 TTS 引擎，或使用本地音色模型"
                } else if (tts.isLanguageAvailable(Locale.SIMPLIFIED_CHINESE) < TextToSpeech.LANG_AVAILABLE) {
                    "缺少中文语音 · 请在系统 TTS 设置中安装"
                } else {
                    "中文引擎已就绪 · 播报可能依赖引擎的网络设置"
                }
            }.onFailure { if (!closed) ttsDescription = "系统语音引擎检查失败" }
        }
        Thread({
            while (!closed) {
                val client = try { server.accept() } catch (_: Exception) { break }
                if (client.peerCredentials.uid != android.os.Process.myUid() || clients.size >= 4) {
                    client.close()
                    continue
                }
                clients.add(client)
                try {
                    executor.execute {
                        client.use {
                            var reply: BridgeReply? = null
                            try {
                                reply = BridgeReply(client.outputStream)
                                client.soTimeout = 10_000
                                val bytes = ArrayList<Byte>()
                                while (true) {
                                    val byte = client.inputStream.read()
                                    check(byte >= 0) { "请求已关闭" }
                                    if (byte == 10) break
                                    check(bytes.size < 65_536) { "请求过长" }
                                    bytes.add(byte.toByte())
                                }
                                val request = JSONObject(bytes.toByteArray().toString(Charsets.UTF_8))
                                check(!closed) { "硬件服务已停止" }
                                when (request.getString("op")) {
                                    "capture" -> capture(client, reply, request.getInt("rate"))
                                    "play" -> synchronized(playbackLock) {
                                        check(!closed) { "硬件服务已停止" }
                                        play(request.getString("path"))
                                        reply.send()
                                    }
                                    "speak" -> synchronized(playbackLock) {
                                        check(!closed) { "硬件服务已停止" }
                                        speak(request.getString("text"), request.optInt("volume", 60))
                                        reply.send()
                                    }
                                    "ir_status" -> { infrared(); reply.send() }
                                    "network" -> {
                                        val value = JSONObject().put("ok", true).put("if_inet6", ipv6Interfaces())
                                        reply.send(value)
                                    }
                                    "ir" -> {
                                        val pulses = request.getJSONArray("durations_us")
                                        val pattern = IntArray(pulses.length()) { pulses.getInt(it) }
                                        check(pattern.isNotEmpty() && pattern.all { it > 0 } &&
                                            pattern.sumOf { it.toLong() } <= 2_000_000) { "红外波形无效" }
                                        infrared().transmit(request.getInt("carrier_hz"), pattern)
                                        reply.send()
                                    }
                                    else -> error("不支持的硬件操作")
                                }
                            } catch (error: Exception) {
                                runCatching { reply?.fail(error.message ?: "硬件操作失败") }
                            } finally {
                                clients.remove(client)
                            }
                        }
                    }
                } catch (_: java.util.concurrent.RejectedExecutionException) {
                    clients.remove(client)
                    client.close()
                }
            }
        }, "camera-hub-hardware").start()
    }

    private fun capture(client: LocalSocket, reply: BridgeReply, rate: Int) {
        check(captureAllowed()) { "后台恢复后需在 App 中点击「恢复语音」" }
        check(context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) ==
            PackageManager.PERMISSION_GRANTED) { "请在 App 中授予麦克风权限" }
        check(rate in 8_000..192_000) { "采样率无效" }
        check(captureBusy.compareAndSet(false, true)) { "麦克风已被另一采集请求占用" }
        var recorder: AudioRecord? = null
        try {
            val audio = try {
                FeatureStatus.microphoneBlock(context)?.let { error(it) }
                synchronized(captureLock) {
                    check(!closed) { "硬件服务已停止" }
                    createRecorder(rate).also {
                        recorder = it
                        activeRecorder = it
                    }.apply {
                        check(state == AudioRecord.STATE_INITIALIZED) { "麦克风初始化失败" }
                        startRecording()
                        check(recordingState == AudioRecord.RECORDSTATE_RECORDING) { "麦克风启动失败" }
                    }
                }
            } catch (error: Exception) {
                captureError = error.message ?: "麦克风启动失败"
                throw error
            }
            capturing = true
            captureError = null
            reply.send()
            val buffer = ByteArray(4096)
            while (!closed && captureAllowed()) {
                val size = try {
                    FeatureStatus.microphoneBlock(context)?.let { error(it) }
                    audio.read(buffer, 0, buffer.size, AudioRecord.READ_BLOCKING).also {
                        check(it > 0) { "麦克风读取失败：$it" }
                    }
                } catch (error: Exception) {
                    if (!closed && captureAllowed()) captureError = error.message ?: "麦克风读取失败"
                    throw error
                }
                if (Build.VERSION.SDK_INT >= 29) {
                    captureSilenced = audio.activeRecordingConfiguration?.isClientSilenced == true
                }
                // A worker closing its connection is normal and is not a microphone failure.
                client.outputStream.write(buffer, 0, size)
            }
        } finally {
            capturing = false
            captureSilenced = false
            synchronized(captureLock) {
                activeRecorder = null
                runCatching { recorder?.stop() }
                runCatching { recorder?.release() }
            }
            captureBusy.set(false)
        }
    }

    private fun createRecorder(rate: Int): AudioRecord {
        check(context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) ==
            PackageManager.PERMISSION_GRANTED) { "请在 App 中授予麦克风权限" }
        val minimum = AudioRecord.getMinBufferSize(
            rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT,
        )
        check(minimum > 0) { "设备不支持此采样率" }
        return AudioRecord.Builder()
            .setAudioSource(MediaRecorder.AudioSource.VOICE_RECOGNITION)
            .setAudioFormat(AudioFormat.Builder().setSampleRate(rate)
                .setChannelMask(AudioFormat.CHANNEL_IN_MONO)
                .setEncoding(AudioFormat.ENCODING_PCM_16BIT).build())
            .setBufferSizeInBytes(maxOf(minimum * 2, 8192))
            .build()
    }

    private fun play(path: String) {
        val file = File(path).canonicalFile
        check(file.path.startsWith(root.canonicalPath + File.separator) && file.isFile) {
            "音频文件不在 App 数据目录"
        }
        val completed = CountDownLatch(1)
        val failed = AtomicBoolean(false)
        val player = MediaPlayer()
        try {
            player.setAudioAttributes(AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_MEDIA)
                .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
            player.setOnCompletionListener { completed.countDown() }
            player.setOnErrorListener { _, _, _ ->
                failed.set(true)
                completed.countDown()
                true
            }
            player.setDataSource(file.path)
            player.prepare()
            check(!closed) { "硬件服务已停止" }
            player.start()
            check(completed.await(55, TimeUnit.SECONDS) && !failed.get()) { "音频播放失败或超时" }
        } finally {
            player.release()
        }
    }

    private fun speak(text: String, volume: Int) {
        check(text.length in 1..4000) { "播报文本长度无效" }
        check(ttsReady.await(10, TimeUnit.SECONDS) && ttsStatus == TextToSpeech.SUCCESS) {
            "系统语音引擎不可用，请安装中文 TTS 或启用音色模型"
        }
        check(!closed) { "硬件服务已停止" }
        check(tts.setLanguage(Locale.SIMPLIFIED_CHINESE) >= TextToSpeech.LANG_AVAILABLE) {
            "系统语音引擎未安装中文语音"
        }
        val completed = CountDownLatch(1)
        val failed = AtomicBoolean(false)
        tts.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
            override fun onStart(utteranceId: String?) {}
            override fun onDone(utteranceId: String?) { completed.countDown() }
            @Deprecated("Required by platform")
            override fun onError(utteranceId: String?) {
                failed.set(true)
                completed.countDown()
            }
        })
        val params = Bundle().apply {
            putFloat(TextToSpeech.Engine.KEY_PARAM_VOLUME, volume.coerceIn(0, 100) / 100f)
        }
        try {
            check(!closed) { "硬件服务已停止" }
            check(tts.speak(text, TextToSpeech.QUEUE_FLUSH, params, "camera-hub") == TextToSpeech.SUCCESS &&
                completed.await(45, TimeUnit.SECONDS) && !failed.get()) { "系统播报失败或超时" }
        } finally {
            tts.stop()
        }
    }

    private fun infrared(): ConsumerIrManager {
        val ir = context.getSystemService(ConsumerIrManager::class.java)
        check(ir != null && ir.hasIrEmitter()) { "设备没有可用的 Android 红外发射器" }
        return ir
    }

    @Suppress("DEPRECATION")
    private fun ipv6Interfaces(): String {
        val manager = context.getSystemService(ConnectivityManager::class.java)
        return manager.allNetworks.flatMap { network ->
            val properties = manager.getLinkProperties(network) ?: return@flatMap emptyList()
            properties.linkAddresses.filter { it.address is Inet6Address }.map { address ->
                // Retain prefix/scope/temporary flags for the existing DDNS selection rules.
                val bytes = address.address.address.joinToString("") { "%02x".format(it) }
                "%s 00 %02x %02x %02x %s".format(
                    bytes, address.prefixLength, address.scope, address.flags, properties.interfaceName,
                )
            }
        }.joinToString("\n")
    }

    override fun close() {
        closed = true
        runCatching { server.close() }
        runCatching { listener.close() }
        synchronized(clients) { clients.toList() }.forEach { runCatching { it.close() } }
        // AudioRecord's blocking native read does not respond to Thread.interrupt().
        // Stop here to unblock it; the capture worker owns the final release.
        synchronized(captureLock) { runCatching { activeRecorder?.stop() } }
        executor.shutdownNow()
        tts.stop()
        tts.shutdown()
        socketFile.delete()
    }
}
