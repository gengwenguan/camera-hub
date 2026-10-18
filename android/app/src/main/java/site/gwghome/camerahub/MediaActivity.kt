package site.gwghome.camerahub

import android.app.AlertDialog
import android.content.Intent
import android.graphics.BitmapFactory
import android.os.Bundle
import android.provider.DocumentsContract
import android.view.View
import android.widget.CheckBox
import android.widget.ImageView
import android.widget.LinearLayout
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.text.DateFormat
import java.util.Date

/** Native media catalogue, authenticated preview and SAF export. */
class MediaActivity : NativePage() {
    private var photos = true
    private var device = ""
    private var day = ""
    private var items = listOf<JSONObject>()
    private var pageIndex = 0
    private val selected = linkedSetOf<String>()
    private var pendingPath: String? = null
    private var pendingName: String? = null
    private var resultMessage = ""
    private var downloading = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (isFinishing) return
        photos = intent.getStringExtra("kind") != "records"
        device = savedInstanceState?.getString("device").orEmpty()
        day = savedInstanceState?.getString("day").orEmpty()
        pageIndex = savedInstanceState?.getInt("page") ?: 0
        pendingPath = savedInstanceState?.getString("pendingPath")
        pendingName = savedInstanceState?.getString("pendingName")
        selected.addAll(savedInstanceState?.getStringArrayList("selected").orEmpty())
        page(if (photos) "AI 相册" else "录像文件")
        render()
        if (device.isNotBlank() && pendingPath == null) refresh()
    }

    override fun refresh() {
        if (device.isBlank()) return
        val path = if (photos) "/api/v1/devices/${segment(device)}/photos"
            else "/api/v1/devices/${segment(device)}/records/${if (day.isBlank()) "days" else segment(day)}"
        request("正在读取文件目录…", content, { api.request(path) }) { body ->
            items = body.getJSONArray(if (photos) "photos" else if (day.isBlank()) "days" else "records").objects()
            selected.retainAll(items.map { it.optString("name") }.toSet())
            render()
            notice.text = resultMessage
            resultMessage = ""
        }
    }

    private fun render() {
        content.removeAllViews()
        val source = ui.card(content)
        val input = ui.field(source, "设备 ID", device).apply { contentDescription = "媒体设备ID" }
        ui.text(source, "可手动填写离线设备 ID，查看已经保存的文件。", 12f, true)
        val row = ui.row(source)
        ui.button(row, "选择设备") {
            request(freeze = content, work = { api.request("/api/v1/devices").getJSONArray("devices").objects() }, done = { devices ->
                val ids = devices.map { it.getString("device_id") }
                if (ids.isEmpty()) notice.setText(R.string.media_no_devices)
                else AlertDialog.Builder(this).setTitle("选择设备").setItems(ids.toTypedArray()) { _, index ->
                    chooseDevice(ids[index])
                }.setNegativeButton("取消", null).show()
            })
        }.also(ui::compact)
        ui.button(row, "加载文件") {
            val value = input.text.toString().trim()
            if (value.isBlank()) notice.setText(R.string.media_device_required) else chooseDevice(value)
        }.also(ui::compact)
        if (device.isBlank()) return
        if (!photos) {
            ui.button(content, "时间轴回放（Web）") { openWeb("camera/playback") }
            if (day.isNotBlank()) {
                ui.text(content, day, 20f)
                ui.button(content, "返回日期列表") {
                    day = ""; items = emptyList(); pageIndex = 0; render(); refresh()
                }
            }
        }
        ui.button(content, "刷新文件列表") { refresh() }
        val pageCount = ((items.size + 29) / 30).coerceAtLeast(1)
        pageIndex = pageIndex.coerceIn(0, pageCount - 1)
        ui.text(content, "共 ${items.size} 项 · 第 ${pageIndex + 1} / $pageCount 页", muted = true)
        if (photos && items.isNotEmpty()) {
            val selection = ui.text(content, "已选择 ${selected.size} 张")
            ui.button(content, "选择本页") {
                selected.addAll(items.drop(pageIndex * 30).take(30).map { it.getString("name") })
                render()
            }
            ui.button(content, "取消选择") { selected.clear(); render() }
            ui.button(content, "删除所选照片") {
                if (selected.isEmpty()) notice.text = "请先选择照片"
                else if (selected.size > 1000) notice.setText(R.string.media_delete_limit)
                else confirm("删除照片", "永久删除所选 ${selected.size} 张照片？") {
                    val names = JSONArray(selected.toList())
                    request("正在删除照片…", content, {
                        api.request("/api/v1/devices/${segment(device)}/photos/delete", "POST",
                            JSONObject().put("names", names), allowPartial = true)
                    }) { response ->
                        val deleted = response.getJSONObject("deleted")
                        resultMessage = "已删除 ${deleted.optInt("deleted")} 张，已不存在 ${deleted.optInt("missing")} 张"
                        val errors = deleted.optJSONArray("errors")
                        if (errors != null && errors.length() > 0) resultMessage += "\n部分删除失败：$errors"
                        refresh()
                    }
                }
            }
            items.drop(pageIndex * 30).take(30).forEach { item ->
                val name = item.getString("name")
                val card = ui.card(content)
                card.addView(CheckBox(this).apply {
                    text = name
                    setTextColor(getColor(R.color.ink))
                    minHeight = ui.dp(48)
                    contentDescription = "选择照片 $name"
                    isChecked = name in selected
                    setOnCheckedChangeListener { _, checked ->
                        if (checked) selected.add(name) else selected.remove(name)
                        selection.text = getString(R.string.media_selected, selected.size)
                    }
                })
                metadata(card, item)
                ui.button(card, "预览照片") { preview(name) }.contentDescription = "预览照片 $name"
                ui.button(card, "保存照片") { export("/photos/${segment(device)}/${segment(name)}", name, "image/jpeg") }
                    .contentDescription = "保存照片 $name"
            }
        } else if (!photos) {
            items.drop(pageIndex * 30).take(30).forEach { item ->
                val card = ui.card(content)
                if (day.isBlank()) {
                    val date = item.getString("date")
                    ui.text(card, "$date · ${item.optInt("files")} 个文件", 18f)
                    ui.text(card, NativeUi.bytes(item.optDouble("bytes", 0.0)), muted = true)
                    ui.button(card, "查看 $date") {
                        day = date; items = emptyList(); pageIndex = 0; render(); refresh()
                    }
                } else {
                    val name = item.getString("name")
                    ui.text(card, name, 18f)
                    metadata(card, item)
                    val active = item.optBoolean("active")
                    if (active) ui.text(card, "正在写入，分片结束后可保存。", muted = true)
                    ui.button(card, "保存录像") {
                        export("/records/${segment(device)}/${segment(day)}/${segment(name)}", name,
                            if (name.endsWith(".mp4", true)) "video/mp4" else "video/mp2t")
                    }.apply { isEnabled = !active; contentDescription = "保存录像 $name" }
                }
            }
        }
        if (items.isEmpty()) ui.text(content, "当前目录没有文件。", muted = true)
        if (pageCount > 1) {
            val pages = ui.row(content)
            ui.button(pages, "上一页") { pageIndex--; render(); scroll.scrollTo(0, 0) }
                .also(ui::compact).isEnabled = pageIndex > 0
            ui.button(pages, "下一页") { pageIndex++; render(); scroll.scrollTo(0, 0) }
                .also(ui::compact).isEnabled = pageIndex < pageCount - 1
        }
    }

    private fun chooseDevice(value: String) {
        if (value != device) { day = ""; selected.clear(); pageIndex = 0 }
        device = value
        items = emptyList()
        render()
        refresh()
    }

    private fun metadata(parent: LinearLayout, item: JSONObject) {
        val epoch = item.optLong("modified_epoch")
        ui.text(parent, NativeUi.bytes(item.optDouble("size", 0.0)) +
            if (epoch > 0) " · ${DateFormat.getDateTimeInstance().format(Date(epoch * 1000))}" else "", 12f, true)
    }

    private fun preview(name: String) {
        request("正在加载照片…", content, {
            val bytes = ByteArrayOutputStream().use {
                api.download("/photos/${segment(device)}/${segment(name)}", it, 16L * 1024 * 1024)
                it.toByteArray()
            }
            val options = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
            if (options.outWidth <= 0 || options.outHeight <= 0) throw IOException("无法解码照片")
            var sample = 1
            while (options.outWidth / sample > 1600 || options.outHeight / sample > 1600) sample *= 2
            options.inSampleSize = sample
            options.inJustDecodeBounds = false
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options) ?: throw IOException("无法解码照片")
        }) { bitmap ->
            val image = ImageView(this).apply {
                adjustViewBounds = true
                setImageBitmap(bitmap)
                contentDescription = name
            }
            AlertDialog.Builder(this).setTitle(name).setView(image)
                .setPositiveButton("关闭", null)
                .setNeutralButton("保存照片") { _, _ -> export("/photos/${segment(device)}/${segment(name)}", name, "image/jpeg") }
                .setNegativeButton("删除") { _, _ ->
                    confirm("删除照片", "永久删除 $name？") {
                        request("正在删除照片…", content, {
                            api.request("/api/v1/devices/${segment(device)}/photos/${segment(name)}", "DELETE")
                        }) { resultMessage = "照片已删除"; refresh() }
                    }
                }.show()
        }
    }

    @Suppress("DEPRECATION")
    private fun export(path: String, name: String, mime: String) {
        if (busy) { notice.text = "请等待当前操作完成"; return }
        pendingPath = path
        pendingName = name
        runCatching {
            startActivityForResult(Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                type = mime
                putExtra(Intent.EXTRA_TITLE, name)
            }, 40)
        }.onFailure { pendingPath = null; pendingName = null; notice.text = errorText(it) }
    }

    @Deprecated("Platform activity result API supports API 27")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != 40) return
        val path = pendingPath
        val name = pendingName
        pendingPath = null
        pendingName = null
        val uri = data?.data
        if (resultCode != RESULT_OK || uri == null || path == null) return
        downloading = true
        request("正在保存 $name…", content, {
            try {
                var updated = 0L
                contentResolver.openOutputStream(uri, "w")?.use { output ->
                    api.download(path, output) { count, total ->
                        val now = android.os.SystemClock.elapsedRealtime()
                        if (now - updated >= 500) {
                            updated = now
                            runOnUiThread {
                                if (!isDestroyed && !isFinishing) notice.text = getString(R.string.media_progress,
                                    NativeUi.bytes(count.toDouble()),
                                    if (total > 0) NativeUi.bytes(total.toDouble()) else "未知大小")
                            }
                        }
                    }
                } ?: throw IOException("无法打开保存位置")
            } catch (error: Exception) {
                runCatching { DocumentsContract.deleteDocument(contentResolver, uri) }
                throw error
            } finally {
                runOnUiThread { downloading = false }
            }
        }) { notice.text = getString(R.string.media_saved, name) }
    }

    override fun handleBack() {
        if (downloading) confirm("取消保存", "返回将中止当前下载并清理未完成文件。") { finish() }
        else finish()
    }
    override fun onSaveInstanceState(outState: Bundle) {
        outState.putString("device", device)
        outState.putString("day", day)
        outState.putInt("page", pageIndex)
        outState.putStringArrayList("selected", ArrayList(selected))
        outState.putString("pendingPath", pendingPath)
        outState.putString("pendingName", pendingName)
        super.onSaveInstanceState(outState)
    }
}
