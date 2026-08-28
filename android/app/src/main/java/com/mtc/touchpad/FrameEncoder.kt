package com.mtc.touchpad

/**
 * Wire-format encoder — mirrors crates/touchpad-proto exactly.
 * Layout contract lives in docs/wire-protocol.md; the unit test locks
 * byte parity with the Rust implementation's canonical vector.
 */
object FrameEncoder {

    private val AUTH_MAGIC = byteArrayOf('A'.code.toByte(), 'T'.code.toByte(), 'K'.code.toByte(), '1'.code.toByte())

    /** Contacts are present/tip-down; confidence can be disabled when the
     * platform reports a palm or otherwise uncertain contact. */
    data class Contact(
        val id: Int,
        val x: Float,
        val y: Float,
        val confidence: Boolean = true,
    )

    const val TIP: Int = 0b01
    const val CONFIDENCE: Int = 0b10
    const val MAX_CONTACTS: Int = 10

    fun encode(seq: Int, scanTimeTicks: Int, button: Boolean, contacts: List<Contact>): ByteArray {
        require(contacts.size <= MAX_CONTACTS) { "too many contacts" }
        val buf = ByteArray(15 + 10 * contacts.size)
        buf[0] = 'A'.code.toByte(); buf[1] = 'T'.code.toByte()
        buf[2] = 'P'.code.toByte(); buf[3] = '1'.code.toByte()
        buf[4] = 1 // version
        buf[5] = if (button) 1 else 0
        buf[6] = contacts.size.toByte()

        putU32(buf, 7, seq)
        putU32(buf, 11, scanTimeTicks)

        contacts.forEachIndexed { i, c ->
            val o = 15 + 10 * i
            buf[o] = c.id.toByte()
            buf[o + 1] = (TIP or if (c.confidence) CONFIDENCE else 0).toByte()
            putF32(buf, o + 2, c.x)
            putF32(buf, o + 6, c.y)
        }
        return buf
    }

    /** Wrap an ATP1 frame in the optional authenticated UDP envelope. */
    fun authenticate(token: String, frame: ByteArray): ByteArray {
        val tokenBytes = token.toByteArray(Charsets.UTF_8)
        require(tokenBytes.isNotEmpty() && tokenBytes.size <= 256) { "token must be 1..256 UTF-8 bytes" }
        val out = ByteArray(6 + tokenBytes.size + frame.size)
        AUTH_MAGIC.copyInto(out, 0)
        out[4] = tokenBytes.size.toByte()
        out[5] = (tokenBytes.size ushr 8).toByte()
        tokenBytes.copyInto(out, 6)
        frame.copyInto(out, 6 + tokenBytes.size)
        return out
    }

    private fun putU32(b: ByteArray, off: Int, v: Int) {
        b[off] = v.toByte(); b[off + 1] = (v ushr 8).toByte()
        b[off + 2] = (v ushr 16).toByte(); b[off + 3] = (v ushr 24).toByte()
    }

    private fun putF32(b: ByteArray, off: Int, v: Float) {
        val bits = java.lang.Float.floatToIntBits(v)
        putU32(b, off, bits)
    }
}
