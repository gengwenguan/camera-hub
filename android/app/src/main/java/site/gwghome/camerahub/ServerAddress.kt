package site.gwghome.camerahub

import java.net.URI
import java.util.Locale

/** A server origin: camera-hub serves its console and API at the root. */
class ServerAddress private constructor(val url: String) {
    fun contains(candidate: String): Boolean = runCatching {
        val target = URI(candidate)
        val origin = URI(url)
        target.rawUserInfo == null &&
            target.scheme.equals(origin.scheme, ignoreCase = true) &&
            target.host.equals(origin.host, ignoreCase = true) &&
            effectivePort(target) == effectivePort(origin)
    }.getOrDefault(false)

    companion object {
        const val DEFAULT = "https://mi6.gwghome.site:8443"

        fun parse(input: String): ServerAddress {
            val uri = URI(input.trim())
            val scheme = uri.scheme?.lowercase(Locale.ROOT)
            require(scheme == "https" || scheme == "http")
            require(!uri.host.isNullOrBlank() && uri.rawUserInfo == null)
            require(uri.port == -1 || uri.port in 1..65535)
            require(uri.rawPath.isNullOrEmpty() || uri.rawPath == "/")
            require(uri.rawQuery == null && uri.rawFragment == null)
            val port = uri.port.takeUnless {
                it == -1 || (scheme == "https" && it == 443) || (scheme == "http" && it == 80)
            }
            return ServerAddress(
                "$scheme://${uri.host.lowercase(Locale.ROOT)}${port?.let { ":$it" }.orEmpty()}",
            )
        }

        private fun effectivePort(uri: URI): Int =
            if (uri.port != -1) uri.port else if (uri.scheme.equals("https", true)) 443 else 80
    }
}
