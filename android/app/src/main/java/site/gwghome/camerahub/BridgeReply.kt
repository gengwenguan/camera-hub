package site.gwghome.camerahub

import org.json.JSONObject
import java.io.OutputStream

/** A connection has one JSON header. After it starts, failure is signalled by EOF. */
internal class BridgeReply(private val output: OutputStream) {
    private var started = false

    fun send(value: JSONObject = JSONObject().put("ok", true)) {
        check(!started) { "硬件响应已发送" }
        // Set before writing: even a partial header must never be followed by another.
        started = true
        output.write("$value\n".toByteArray(Charsets.UTF_8))
        output.flush()
    }

    fun fail(error: String) {
        if (!started) send(JSONObject().put("ok", false).put("error", error.take(512)))
    }
}
