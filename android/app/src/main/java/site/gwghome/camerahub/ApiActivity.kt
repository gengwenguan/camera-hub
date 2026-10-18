package site.gwghome.camerahub

import android.os.Bundle
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import java.net.ConnectException
import java.net.SocketTimeoutException
import java.net.UnknownHostException
import java.util.concurrent.Executors
import javax.net.ssl.SSLException

/** One request at a time per screen; results are never delivered to a destroyed Activity. */
abstract class ApiActivity : HubActivity() {
    protected lateinit var server: ServerAddress
    protected lateinit var api: ApiClient
    protected lateinit var ui: NativeUi
    protected lateinit var notice: TextView
    protected var busy = false
        private set
    protected var pollingRequest = false
    private val executor = Executors.newSingleThreadExecutor()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ui = NativeUi(this)
        server = runCatching { ServerAddress.parse(intent.getStringExtra("server").orEmpty()) }
            .getOrElse { finish(); return }
        api = ApiClient(server)
    }

    protected fun <T> request(
        message: String = "正在加载…",
        freeze: View? = null,
        work: () -> T,
        failed: (Throwable) -> Unit = {},
        done: (T) -> Unit,
    ) {
        if (isFinishing || isDestroyed) return
        if (busy) {
            android.widget.Toast.makeText(this, "正在处理上一项操作，请稍候", android.widget.Toast.LENGTH_SHORT).show()
            return
        }
        busy = true
        val quiet = pollingRequest
        if (!quiet) {
            notice.text = message
            notice.setTextColor(getColor(R.color.muted))
        }
        val enabled = mutableListOf<View>()
        fun disable(view: View) {
            if (view.isEnabled) { enabled.add(view); view.isEnabled = false }
            if (view is ViewGroup) for (index in 0 until view.childCount) disable(view.getChildAt(index))
        }
        freeze?.let(::disable)
        executor.execute {
            val result = runCatching(work)
            runOnUiThread {
                if (isFinishing || isDestroyed) return@runOnUiThread
                busy = false
                enabled.forEach { it.isEnabled = true }
                result.fold(onSuccess = {
                    if (!quiet) notice.text = ""
                    runCatching { done(it) }.onFailure { error ->
                        notice.text = errorText(error)
                        notice.setTextColor(getColor(R.color.warning))
                    }
                }, onFailure = {
                    notice.text = errorText(it)
                    notice.setTextColor(getColor(R.color.warning))
                    failed(it)
                    if (it is ApiException && it.status == 401) unauthorized()
                })
            }
        }
    }

    protected open fun unauthorized() {}

    protected fun errorText(error: Throwable): String = when (error) {
        is SSLException -> getString(R.string.tls_error)
        is UnknownHostException -> getString(R.string.health_dns)
        is SocketTimeoutException -> "请求超时。如刚执行了操作，请刷新状态确认结果后再操作。"
        is ConnectException -> getString(R.string.health_connect)
        else -> error.message ?: "请求失败，请稍后重试"
    }

    override fun onDestroy() {
        if (::api.isInitialized) api.close()
        executor.shutdownNow()
        super.onDestroy()
    }
}
