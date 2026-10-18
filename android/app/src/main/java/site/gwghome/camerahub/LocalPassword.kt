package site.gwghome.camerahub

import android.app.Activity
import android.app.AlertDialog
import android.content.Intent
import android.view.View
import android.widget.Toast

object LocalPassword {
    fun show(activity: Activity) {
        val config = EngineConfig(activity)
        val ui = NativeUi(activity)
        val form = ui.column(20)
        ui.text(form, "浏览器登录账号：admin。服务运行时，保存会重启服务并使旧会话失效。", muted = true)
        val first = ui.field(form, "新密码（至少 5 个字符）", secret = true)
        val second = ui.field(form, "再次输入新密码", secret = true)
        val error = ui.text(form, "", 13f).apply {
            setTextColor(activity.getColor(R.color.warning))
            visibility = View.GONE
        }
        val dialog = AlertDialog.Builder(activity)
            .setTitle(R.string.engine_change_password)
            .setView(form)
            .setNegativeButton("取消", null)
            .setPositiveButton("保存", null)
            .create()
        dialog.setOnShowListener {
            dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener {
                val wanted = first.text.toString()
                val message = when {
                    wanted.length < 5 -> "密码至少需要 5 个字符"
                    wanted != second.text.toString() -> "两次输入的密码不一致"
                    else -> null
                }
                if (message != null) {
                    error.text = message
                    error.visibility = View.VISIBLE
                    return@setOnClickListener
                }
                config.savePassword(wanted)
                if (EngineService.active) {
                    activity.startService(Intent(activity, EngineService::class.java)
                        .setAction(EngineService.RESTART))
                    Toast.makeText(activity, "密码已保存，正在重新加载本机服务", Toast.LENGTH_LONG).show()
                } else {
                    Toast.makeText(activity, "密码已保存，将在下次启动服务时生效", Toast.LENGTH_LONG).show()
                }
                dialog.dismiss()
            }
        }
        dialog.show()
    }
}
