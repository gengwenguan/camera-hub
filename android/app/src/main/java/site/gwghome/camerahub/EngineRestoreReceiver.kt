package site.gwghome.camerahub

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

class EngineRestoreReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val boot = intent.action == Intent.ACTION_BOOT_COMPLETED
        if (!boot && intent.action != Intent.ACTION_MY_PACKAGE_REPLACED) return
        val state = EngineState(context)
        if (!state.policy.canRestore(afterBoot = boot) || EngineService.active) return
        try {
            context.startForegroundService(Intent(context, EngineService::class.java)
                .setAction(EngineService.RESTORE))
        } catch (error: RuntimeException) {
            val failure = "系统未允许后台恢复，请打开 App 启动：${error.message}"
            state.recordFailure(failure)
            state.recordStatus(failure)
        }
    }
}
