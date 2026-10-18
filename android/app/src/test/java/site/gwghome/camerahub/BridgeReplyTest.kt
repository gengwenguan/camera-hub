package site.gwghome.camerahub

import org.json.JSONObject
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.OutputStream

class BridgeReplyTest {
    @Test fun failureBeforeCaptureReturnsOneJsonHeader() {
        val output = ByteArrayOutputStream()
        val reply = BridgeReply(output)
        reply.fail("麦克风\n未授权")
        reply.fail("不应发送第二次")

        val wire = output.toString("UTF-8")
        val value = JSONObject(wire)
        assertFalse(value.getBoolean("ok"))
        assertEquals("麦克风\n未授权", value.getString("error"))
        assertEquals(1, wire.count { it == '\n' })
    }

    @Test fun captureFailureNeverAppendsJsonToPcm() {
        val output = ByteArrayOutputStream()
        val reply = BridgeReply(output)
        reply.send()
        val header = output.toByteArray()
        val pcm = byteArrayOf(0, 1, 10, 0, -1, 127)
        output.write(pcm)
        reply.fail("麦克风读取失败")

        assertTrue(JSONObject(header.toString(Charsets.UTF_8)).getBoolean("ok"))
        assertArrayEquals(header + pcm, output.toByteArray())
    }

    @Test fun partialHeaderFailureDoesNotStartAnotherResponse() {
        val bytes = ByteArrayOutputStream()
        val output = object : OutputStream() {
            override fun write(value: Int) {
                if (bytes.size() == 3) throw IOException("socket closed")
                bytes.write(value)
            }
        }
        val reply = BridgeReply(output)
        assertTrue(runCatching { reply.send() }.exceptionOrNull() is IOException)
        reply.fail("socket closed")
        assertEquals(3, bytes.size())
    }
}
