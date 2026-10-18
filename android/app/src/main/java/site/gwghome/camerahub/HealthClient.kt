package site.gwghome.camerahub

import android.os.SystemClock
import org.json.JSONObject
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

data class HealthStatus(val uptimeSeconds: Long, val latencyMs: Long)

class HealthException(val textId: Int, val statusCode: Int = 0) : IOException()

object HealthClient {
    /** Runs on the caller's background thread; /health needs no login cookie. */
    fun check(server: ServerAddress, timeoutMs: Int = 8_000): HealthStatus {
        val started = SystemClock.elapsedRealtime()
        val connection = URL("${server.url}/health").openConnection() as HttpURLConnection
        try {
            connection.connectTimeout = timeoutMs
            connection.readTimeout = timeoutMs
            connection.instanceFollowRedirects = false
            connection.useCaches = false
            connection.setRequestProperty("Accept", "application/json")
            val status = connection.responseCode
            if (status in 300..399) throw HealthException(R.string.health_redirect)
            if (status != 200) throw HealthException(R.string.http_error, status)
            val body = connection.inputStream.bufferedReader().use { reader ->
                val result = StringBuilder()
                val buffer = CharArray(2048)
                while (true) {
                    val count = reader.read(buffer)
                    if (count < 0) break
                    if (result.length + count > 16_384) {
                        throw HealthException(R.string.health_invalid)
                    }
                    result.append(buffer, 0, count)
                }
                result.toString()
            }
            val json = JSONObject(body)
            if (!json.optBoolean("ok") || json.optString("service") != "camera-hub") {
                throw HealthException(R.string.health_invalid)
            }
            val uptime = json.getLong("uptime_seconds")
            if (uptime < 0) throw HealthException(R.string.health_invalid)
            return HealthStatus(uptime, SystemClock.elapsedRealtime() - started)
        } finally {
            connection.disconnect()
        }
    }
}
