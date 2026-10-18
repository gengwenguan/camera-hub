package site.gwghome.camerahub

import android.annotation.SuppressLint
import android.content.Context

class EngineState(context: Context) {
    private val preferences = context.getSharedPreferences("engine-recovery", Context.MODE_PRIVATE)

    val policy: RecoveryPolicy
        get() = synchronized(lock) {
            RecoveryPolicy(
                preferences.getBoolean("desired", false),
                preferences.getBoolean("enabled", true),
                preferences.getBoolean("boot", false),
                preferences.getInt("attempts", 0),
            )
        }
    val status: String get() = preferences.getString("status", "本机服务未启动").orEmpty()
    val lastFailure: String get() = preferences.getString("failure", "").orEmpty()
    val lastFailureAt: Long get() = preferences.getLong("failureAt", 0)

    // Small synchronous writes: manual stop and reserved attempts must survive process death.
    @SuppressLint("ApplySharedPref")
    fun update(transform: (RecoveryPolicy) -> RecoveryPolicy) = synchronized(lock) {
        val next = transform(policy)
        preferences.edit().putBoolean("desired", next.desiredRunning)
            .putBoolean("enabled", next.enabled).putBoolean("boot", next.bootEnabled)
            .putInt("attempts", next.attempts).commit()
        Unit
    }

    fun reserveRetry(): Long? = synchronized(lock) {
        val next = policy.reserveRetry() ?: return null
        update { next }
        next.retryDelayMs
    }

    fun recordStatus(value: String) {
        preferences.edit().putString("status", value).apply()
    }

    fun recordFailure(value: String) {
        preferences.edit().putString("failure", value.take(512))
            .putLong("failureAt", System.currentTimeMillis()).apply()
    }

    companion object {
        private val lock = Any()
    }
}
