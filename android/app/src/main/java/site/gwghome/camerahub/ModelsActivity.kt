package site.gwghome.camerahub

import android.os.Bundle
import android.widget.ProgressBar

class ModelsActivity : NativePage() {
    override val pollMillis = 3_000L
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (isFinishing) return
        page("模型管理")
        ui.button(content, "刷新模型状态") { refresh() }
        refresh()
    }

    override fun refresh() {
        val position = scroll.scrollY
        request(work = { api.request("/api/v1/assets").getJSONObject("assets") }, done = { assets ->
            content.removeAllViews()
            ui.text(content, "模型安装在当前节点。离开页面后，服务端会继续下载和安装。", muted = true)
            for (key in listOf("voice_kws", "voice_asr", "voice_tts")) {
                val asset = assets.optJSONObject(key) ?: continue
                val card = ui.card(content)
                ui.text(card, asset.optString("label"), 20f)
                val active = asset.optBoolean("active")
                ui.text(card, when {
                    active -> "安装中 · ${asset.optString("state")}"
                    asset.optBoolean("installed") -> "已安装"
                    !asset.optBoolean("supported") -> "此节点暂不支持"
                    else -> "待安装"
                }, 16f).apply { contentDescription = "${asset.optString("label")} · $text" }
                ui.text(card, asset.optString("detail"), muted = true)
                if (active) {
                    val percent = asset.optDouble("progress_percent", Double.NaN)
                    card.addView(ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal).apply {
                        isIndeterminate = !percent.isFinite()
                        max = 100
                        progress = if (percent.isFinite()) percent.toInt().coerceIn(0, 100) else 0
                        contentDescription = "模型安装进度"
                    })
                    ui.text(card, "已下载 ${NativeUi.bytes(asset.optDouble("downloaded_bytes", 0.0))} / " +
                        NativeUi.bytes(asset.optDouble("download_bytes", 0.0)), 13f, true)
                }
                val error = asset.optString("last_error").takeUnless { it == "null" }.orEmpty()
                if (error.isNotBlank()) ui.text(card, error)
                if (!asset.optBoolean("installed")) {
                    ui.text(card, "所需可用空间 ${NativeUi.bytes(asset.optDouble("required_available_bytes", 0.0))}", 12f, true)
                    ui.button(card, "安装${asset.optString("label")}") {
                        confirm("安装模型", "将在 ${server.url} 下载并安装此模型。") {
                            request("正在提交安装任务…", content, {
                                api.request("/api/v1/assets/${segment(asset.getString("id"))}/install", "POST")
                            }) {
                                refresh()
                            }
                        }
                    }.isEnabled = asset.optBoolean("installable") && !active && !assets.optBoolean("busy")
                }
            }
            ui.button(content, "刷新模型状态") { refresh() }
            scroll.post { scroll.scrollTo(0, position) }
        })
    }
}
