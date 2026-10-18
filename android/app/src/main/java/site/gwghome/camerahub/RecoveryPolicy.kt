package site.gwghome.camerahub

/** Persisted across service/process recreation so crashes cannot reset the retry budget. */
data class RecoveryPolicy(
    val desiredRunning: Boolean = false,
    val enabled: Boolean = true,
    val bootEnabled: Boolean = false,
    val attempts: Int = 0,
) {
    fun canRestore(afterBoot: Boolean = false): Boolean =
        desiredRunning && enabled && (!afterBoot || bootEnabled) && attempts < MAX_ATTEMPTS

    fun manualStart() = copy(desiredRunning = true, attempts = 0)
    fun manualStop() = copy(desiredRunning = false, attempts = 0)
    fun reserveRetry(): RecoveryPolicy? = if (canRestore()) copy(attempts = attempts + 1) else null
    fun stable() = copy(attempts = 0)

    val retryDelayMs: Long
        get() = if (attempts == 0) 0 else minOf(2_000L shl (attempts.coerceIn(1, MAX_ATTEMPTS) - 1), 30_000L)

    companion object {
        const val MAX_ATTEMPTS = 5
        const val STABLE_MS = 5 * 60_000L
    }
}
