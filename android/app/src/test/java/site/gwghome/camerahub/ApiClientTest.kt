package site.gwghome.camerahub

import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import java.net.InetAddress
import java.net.ServerSocket
import java.io.ByteArrayInputStream
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicInteger

class ApiClientTest {
    private lateinit var http: ServerSocket
    private lateinit var thread: Thread
    private data class Exchange(val requestHeaders: Map<String, String>, val requestBody: ByteArrayInputStream)
    private val routes = ConcurrentHashMap<String, (Exchange) -> ByteArray>()
    private lateinit var server: ServerAddress
    private lateinit var api: ApiClient

    @Before fun start() {
        http = ServerSocket(0, 10, InetAddress.getByName("127.0.0.1"))
        thread = Thread {
            while (!http.isClosed) {
                val socket = runCatching { http.accept() }.getOrNull() ?: break
                socket.use {
                    runCatching {
                        val input = socket.getInputStream().buffered()
                        fun line(): String {
                            val bytes = java.io.ByteArrayOutputStream()
                            while (true) {
                                val byte = input.read()
                                if (byte < 0 || byte == 10) return bytes.toString("UTF-8").trimEnd('\r')
                                bytes.write(byte)
                            }
                        }
                        val path = line().split(" ")[1]
                        val headers = mutableMapOf<String, String>()
                        while (true) {
                            val line = line()
                            if (line.isEmpty()) break
                            headers[line.substringBefore(':').lowercase()] = line.substringAfter(':').trim()
                        }
                        val bytes = ByteArray(headers["content-length"]?.toInt() ?: 0)
                        java.io.DataInputStream(input).readFully(bytes)
                        val response = routes.getValue(path)(Exchange(headers, ByteArrayInputStream(bytes)))
                        socket.getOutputStream().write(response)
                    }
                }
            }
        }.apply { isDaemon = true; start() }
        server = ServerAddress.parse("http://127.0.0.1:${http.localPort}")
        api = ApiClient(server)
    }
    @After fun stop() {
        api.close()
        HubSessions.clear(server)
        http.close()
        thread.join(1000)
    }
    private fun endpoint(path: String, status: Int = 200, body: String = """{"ok":true}""",
                         header: Pair<String, String>? = null, inspect: (Exchange) -> Unit = {}) {
        routes[path] = {
            inspect(it)
            val bytes = body.toByteArray()
            ("HTTP/1.1 $status Response\r\nConnection: close\r\nContent-Length: ${bytes.size}\r\n" +
                (header?.let { (name, value) -> "$name: $value\r\n" } ?: "") + "\r\n").toByteArray() + bytes
        }
    }
    @Test fun nativeLoginSendsJsonAndReusesHttpOnlySession() {
        var posted = ""
        var cookie: String? = null
        endpoint("/api/v1/auth/login", header = "Set-Cookie" to
            "camera_hub_session=token; Path=/; HttpOnly; SameSite=Strict") {
            posted = it.requestBody.bufferedReader().readText()
        }
        endpoint("/api/v1/info") { cookie = it.requestHeaders["cookie"] }
        api.request("/api/v1/auth/login", "POST", JSONObject().put("username", "用户").put("password", "a\"b"))
        api.request("/api/v1/info")
        assertEquals("用户", JSONObject(posted).getString("username"))
        assertEquals("a\"b", JSONObject(posted).getString("password"))
        assertEquals("camera_hub_session=token", cookie)
    }
    @Test fun redirectsNeverForwardCredentialsOrRepeatMutation() {
        val hits = AtomicInteger()
        endpoint("/redirect", 307, header = "Location" to "${server.url}/target")
        endpoint("/target") { hits.incrementAndGet() }
        val error = runCatching { api.request("/redirect", "POST", JSONObject().put("password", "private")) }.exceptionOrNull()
        assertEquals(307, (error as ApiException).status)
        assertEquals(0, hits.get())
    }
    @Test fun unauthorizedClearsOnlyMatchingOrigin() {
        val other = ServerAddress.parse("http://127.0.0.1:1")
        HubSessions.accept(server, listOf("camera_hub_session=a"))
        HubSessions.accept(other, listOf("camera_hub_session=b"))
        endpoint("/private", 401)
        assertEquals(401, (runCatching { api.request("/private") }.exceptionOrNull() as ApiException).status)
        assertNull(HubSessions.cookie(server))
        assertEquals("camera_hub_session=b", HubSessions.cookie(other))
        HubSessions.clear(other)
    }
    @Test fun serverValidationMessageIsPreserved() {
        endpoint("/settings", 400, """{"ok":false,"error":"配置已被其他会话修改，请重新加载"}""")
        val error = runCatching { api.request("/settings", "PUT", JSONObject()) }.exceptionOrNull()
        assertTrue(error!!.message!!.contains("其他会话"))
    }
    @Test fun secureCookieIsNotAcceptedOverHttpAndDeletionIsHonored() {
        HubSessions.accept(server, listOf("camera_hub_session=a; Secure"))
        assertNull(HubSessions.cookie(server))
        HubSessions.accept(server, listOf("camera_hub_session=a"))
        HubSessions.accept(server, listOf("camera_hub_session=; Max-Age=0"))
        assertNull(HubSessions.cookie(server))
    }
    @Test fun invalidOrOversizedResponsesFailWithoutCrashingUi() {
        endpoint("/html", body = "<html>login</html>")
        endpoint("/large", body = "a".repeat(2 * 1024 * 1024 + 1))
        assertTrue(runCatching { api.request("/html") }.exceptionOrNull() is java.io.IOException)
        assertTrue(runCatching { api.request("/large") }.exceptionOrNull() is java.io.IOException)
    }
}
