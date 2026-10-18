package site.gwghome.camerahub

import android.content.Intent
import android.os.Bundle
import android.widget.Button
import android.widget.TextView

class MainActivity : HubActivity() {
    private lateinit var localManage: Button
    private lateinit var localStatusTitle: TextView
    private lateinit var localStatusDetail: TextView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        fitSystemBars()
        localManage = findViewById(R.id.local_manage)
        localStatusTitle = findViewById(R.id.local_status_title)
        localStatusDetail = findViewById(R.id.local_status_detail)
        localManage.setOnClickListener { openWebManagement() }
        findViewById<Button>(R.id.local_password).setOnClickListener {
            LocalPassword.show(this)
        }
        findViewById<Button>(R.id.local_engine).setOnClickListener {
            startActivity(Intent(this, EngineActivity::class.java))
        }
    }

    override fun onResume() {
        super.onResume()
        refreshLocalNode()
    }

    private fun refreshLocalNode() {
        val running = EngineService.active
        localStatusTitle.text = if (running) "服务运行中" else "服务未启动"
        localStatusTitle.setTextColor(getColor(if (running) R.color.accent else R.color.warning))
        localStatusDetail.text = if (running) {
            "${EngineConfig(this).url}\n浏览器登录账号：admin；密码可在下方设置。"
        } else {
            "请先进入本机服务设置并启动服务"
        }
    }

    private fun openWebManagement() {
        if (!EngineService.active) {
            startActivity(Intent(this, EngineActivity::class.java))
            return
        }
        runCatching {
            startForegroundService(Intent(this, EngineService::class.java)
                .setAction(EngineService.RESUME_VOICE))
        }
        openBrowser(EngineConfig(this).url)
    }
}
