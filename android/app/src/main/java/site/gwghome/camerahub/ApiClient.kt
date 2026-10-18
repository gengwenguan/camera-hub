package site.gwghome.camerahub

import org.json.JSONObject
import java.io.IOException
import java.io.OutputStream
import java.net.HttpCookie
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.ConcurrentHashMap

class ApiException(val status: Int, message: String) : IOException(message)

/** Passwords are never saved. Sessions live in memory and are isolated by exact origin. */
object HubSessions {
    private val cookies = ConcurrentHashMap<String, String>()
    fun cookie(server: ServerAddress): String? = cookies[server.url]
    fun clear(server: ServerAddress) { cookies.remove(server.url) }
    fun accept(server: ServerAddress, headers: List<String>) {
        headers.flatMap { HttpCookie.parse(it) }.firstOrNull { it.name == "camera_hub_session" }?.let {
            if (it.maxAge == 0L || it.value.isEmpty()) clear(server)
            else if (!it.secure || server.url.startsWith("https:")) {
                cookies[server.url] = "${it.name}=${it.value}"
            }
        }
    }
}

/** Synchronous API transport; callers use a background executor. Never follows redirects. */
class ApiClient(val server: ServerAddress) {
    @Volatile private var closed = false
    @Volatile private var active: HttpURLConnection? = null

    private fun <T> exchange(path: String, method: String, body: JSONObject?,
                             accept: String, read: (HttpURLConnection) -> T): T {
        check(path.startsWith("/") && !path.startsWith("//") && server.contains(server.url + path))
        if (closed) throw IOException("页面已关闭")
        val connection = URL(server.url + path).openConnection() as HttpURLConnection
        active = connection
        try {
            if (closed) throw IOException("页面已关闭")
            connection.connectTimeout = 8_000
            connection.readTimeout = 15_000
            connection.instanceFollowRedirects = false
            connection.useCaches = false
            connection.requestMethod = method
            connection.setRequestProperty("Accept", accept)
            HubSessions.cookie(server)?.let { connection.setRequestProperty("Cookie", it) }
            if (body != null) {
                connection.doOutput = true
                connection.setRequestProperty("Content-Type", "application/json; charset=utf-8")
                connection.outputStream.use { it.write(body.toString().toByteArray(Charsets.UTF_8)) }
            }
            val status = connection.responseCode
            if (status in 300..399) {
                throw ApiException(status, "服务要求跳转，请返回连接页填写最终 HTTPS 地址（包括端口）。")
            }
            if (status == 401) {
                HubSessions.clear(server)
                throw ApiException(status, if (path.endsWith("/auth/login"))
                    "本机登录密码不一致，请在服务设置中重新设置"
                else "本机会话已过期")
            }
            if (status !in 200..299) {
                val text = connection.errorStream?.use { it.readNBytesLimited(2 * 1024 * 1024) }
                    ?.toString(Charsets.UTF_8).orEmpty()
                val json = runCatching { JSONObject(text) }.getOrNull()
                throw ApiException(status,
                    json?.optString("error")?.takeIf { it.isNotBlank() } ?: "服务返回 HTTP $status")
            }
            val headers = connection.headerFields.entries.filter { it.key.equals("Set-Cookie", true) }
                .flatMap { it.value }
            HubSessions.accept(server, headers)
            return read(connection)
        } finally {
            connection.disconnect()
            if (active === connection) active = null
        }
    }

    fun request(path: String, method: String = "GET", body: JSONObject? = null,
                allowPartial: Boolean = false): JSONObject =
        exchange(path, method, body, "application/json") { connection ->
            if (connection.responseCode == 204) JSONObject() else {
                val text = connection.inputStream.use { it.readNBytesLimited(2 * 1024 * 1024) }
                    .toString(Charsets.UTF_8)
                val json = runCatching { JSONObject(text) }.getOrNull()
                    ?: throw IOException("服务未返回有效的 JSON 数据")
                if (!allowPartial && json.has("ok") && !json.optBoolean("ok"))
                    throw IOException(json.optString("error", "操作未成功"))
                json
            }
        }

    /** Streams authenticated files without keeping an entire recording in memory. */
    fun download(path: String, output: OutputStream, limit: Long = Long.MAX_VALUE,
                 progress: (Long, Long) -> Unit = { _, _ -> }): Long =
        exchange(path, "GET", null, "*/*") { connection ->
            val expected = connection.contentLengthLong
            if (expected > limit) throw IOException("文件过大")
            if (connection.contentType.orEmpty().let { it.startsWith("text/html") || it.startsWith("application/json") })
                throw IOException("服务未返回媒体文件")
            var count = 0L
            connection.inputStream.use { input ->
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    if (closed || Thread.currentThread().isInterrupted) throw IOException("下载已取消")
                    val size = input.read(buffer)
                    if (size < 0) break
                    if (count > limit - size) throw IOException("文件过大")
                    output.write(buffer, 0, size)
                    count += size
                    progress(count, expected)
                }
            }
            if (expected >= 0 && count != expected) throw IOException("文件传输不完整")
            count
        }

    fun close() {
        closed = true
        active?.disconnect()
    }
}

private fun java.io.InputStream.readNBytesLimited(limit: Int): ByteArray {
    val output = java.io.ByteArrayOutputStream()
    val buffer = ByteArray(8192)
    while (true) {
        val size = read(buffer)
        if (size < 0) return output.toByteArray()
        if (output.size() + size > limit) throw IOException("服务响应过大")
        output.write(buffer, 0, size)
    }
}
