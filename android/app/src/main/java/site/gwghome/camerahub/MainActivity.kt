package site.gwghome.camerahub

import android.app.KeyguardManager
import android.content.Intent
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.widget.Button
import android.widget.TextView

class MainActivity : HubActivity() {
    private lateinit var localManage: Button
    private lateinit var localStatusTitle: TextView
    private lateinit var localStatusDetail: TextView
    private val main = Handler(Looper.getMainLooper())
    private var localRouteConsumed = false
    private val localRoute = object : Runnable {
        override fun run() {
            if (isFinishing || isDestroyed || localRouteConsumed) return
            refreshLocalNode()
            val locked = getSystemService(KeyguardManager::class.java).isKeyguardLocked
            if (!EngineService.active || locked) {
                main.postDelayed(this, 500)
                return
            }
            localRouteConsumed = true
            openLocalNode()
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        fitSystemBars()
        localRouteConsumed = savedInstanceState?.getBoolean("local_route_consumed") ?: false
        localManage = findViewById(R.id.local_manage)
        localStatusTitle = findViewById(R.id.local_status_title)
        localStatusDetail = findViewById(R.id.local_status_detail)
        localManage.setOnClickListener { openLocalNode() }
        findViewById<Button>(R.id.local_engine).setOnClickListener {
            startActivity(Intent(this, EngineActivity::class.java))
        }
    }

    override fun onResume() {
        super.onResume()
        refreshLocalNode()
        if (!localRouteConsumed) main.post(localRoute)
    }

    override fun onPause() {
        main.removeCallbacks(localRoute)
        super.onPause()
    }

    private fun refreshLocalNode() {
        val running = EngineService.active
        localStatusTitle.text = if (running) "服务运行中" else "服务未启动"
        localStatusTitle.setTextColor(getColor(if (running) R.color.accent else R.color.warning))
        localStatusDetail.text = if (running) {
            "${EngineConfig(this).url} · 打开 App 自动进入管理"
        } else {
            "启动后可直接管理，无需再次登录"
        }
        localManage.text = if (running) "进入本机管理" else "启动本机服务"
    }

    private fun openLocalNode() {
        if (!EngineService.active) {
            startActivity(Intent(this, EngineActivity::class.java))
            return
        }
        runCatching {
            startForegroundService(Intent(this, EngineService::class.java)
                .setAction(EngineService.RESUME_VOICE))
        }
        startActivity(Intent(this, DashboardActivity::class.java)
            .putExtra("server", EngineConfig(this).url))
    }

    override fun onSaveInstanceState(outState: Bundle) {
        outState.putBoolean("local_route_consumed", localRouteConsumed)
        super.onSaveInstanceState(outState)
    }

    override fun onDestroy() {
        main.removeCallbacks(localRoute)
        super.onDestroy()
    }
}
