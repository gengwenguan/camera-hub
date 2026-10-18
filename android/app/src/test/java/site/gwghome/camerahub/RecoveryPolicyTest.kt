package site.gwghome.camerahub

import org.junit.Assert.*
import org.junit.Test

class RecoveryPolicyTest {
    @Test fun `fresh install and manual stop never restore`() {
        val fresh = RecoveryPolicy()
        assertFalse(fresh.canRestore())
        val running = fresh.manualStart().copy(bootEnabled = true)
        assertTrue(running.canRestore(afterBoot = true))
        val stopped = running.reserveRetry()!!.manualStop()
        assertFalse(stopped.canRestore())
        assertFalse(stopped.canRestore(afterBoot = true))
        assertNull(stopped.reserveRetry())
    }

    @Test fun `boot needs opt in and a previously running service`() {
        val running = RecoveryPolicy().manualStart()
        assertTrue(running.canRestore())
        assertFalse(running.canRestore(afterBoot = true))
        assertFalse(RecoveryPolicy(bootEnabled = true).canRestore(afterBoot = true))
        assertTrue(running.copy(bootEnabled = true).canRestore(afterBoot = true))
    }

    @Test fun `five automatic attempts exhaust budget including after reconstruction`() {
        var current = RecoveryPolicy().manualStart()
        val delays = mutableListOf<Long>()
        repeat(RecoveryPolicy.MAX_ATTEMPTS) {
            current = current.reserveRetry()!!
            delays += current.retryDelayMs
            // Recreated supervisors receive the saved counter, not a fresh budget.
            current = RecoveryPolicy(current.desiredRunning, current.enabled, current.bootEnabled, current.attempts)
        }
        assertEquals(listOf(2_000L, 4_000L, 8_000L, 16_000L, 30_000L), delays)
        assertFalse(current.canRestore())
        assertFalse(current.copy(bootEnabled = true).canRestore(afterBoot = true))
        assertNull(current.reserveRetry())
    }

    @Test fun `disabling recovery cancels retries without losing running intent`() {
        val disabled = RecoveryPolicy().manualStart().reserveRetry()!!.copy(enabled = false, bootEnabled = true)
        assertTrue(disabled.desiredRunning)
        assertFalse(disabled.canRestore())
        assertFalse(disabled.canRestore(afterBoot = true))
        assertNull(disabled.reserveRetry())
        assertEquals(2, disabled.copy(enabled = true).reserveRetry()!!.attempts)
    }

    @Test fun `stable operation and an explicit start reset exhausted retries`() {
        val exhausted = RecoveryPolicy(desiredRunning = true, attempts = RecoveryPolicy.MAX_ATTEMPTS)
        assertEquals(1, exhausted.stable().reserveRetry()!!.attempts)
        val restarted = exhausted.manualStop().manualStart()
        assertTrue(restarted.desiredRunning)
        assertEquals(0, restarted.attempts)
        assertEquals(2_000L, restarted.reserveRetry()!!.retryDelayMs)
    }
}
