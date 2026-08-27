package com.mtc.touchpad

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Byte-parity lock against the Rust implementation's canonical vector
 * (docs/wire-protocol.md, crates/touchpad-proto tests).
 */
class FrameEncoderTest {

    @Test
    fun canonical_vector() {
        // button=true; seq=42; scan=987654;
        // c5 (-13.5, 77.25) tip+conf; c9 (4.0, -0.5) tip only
        val golden =
            "41 54 50 31 01 01 02 2a 00 00 00 06 12 0f 00 " +
            "05 03 00 00 58 c1 00 80 9a 42 " +
            "09 01 00 00 80 40 00 00 00 bf"

        val bytes = FrameEncoder.encode(
            seq = 42,
            scanTimeTicks = 987_654,
            button = true,
            contacts = listOf(
                FrameEncoder.Contact(5, -13.5f, 77.25f),
                FrameEncoder.Contact(9, 4.0f, -0.5f, confidence = false),
            ),
        )

        assertEquals(
            golden,
            bytes.joinToString(" ") { "%02x".format(it.toInt() and 0xff) },
        )
    }

    @Test
    fun empty_frame_is_header_only() {
        val bytes = FrameEncoder.encode(seq = 1, scanTimeTicks = 0, button = false, contacts = emptyList())
        assertEquals(15, bytes.size)
    }

    @Test
    fun physical_pixel_scale_is_isotropic() {
        val mm = mmPerPixel(xdpi = 445.6f, ydpi = 445.6f, densityDpi = 520)
        assertEquals(25.4f / 445.6f, mm, 0.00001f)
        assertEquals(25.4f / 520f, mmPerPixel(0f, Float.NaN, densityDpi = 520), 0.00001f)
        assertTrue(mm > 0f)
    }
}
