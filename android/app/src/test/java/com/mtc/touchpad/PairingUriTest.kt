package com.mtc.touchpad

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PairingUriTest {
    @Test
    fun parsesPairingLink() {
        assertEquals(
            PairingTarget("macbook.local", 4242, "secret"),
            PairingUri.parse("mtc://pair?host=macbook.local&port=4242&token=secret"),
        )
    }

    @Test
    fun rejectsNonPairingLinksAndBadPorts() {
        assertNull(PairingUri.parse("https://example.com/?host=macbook.local&port=4242"))
        assertNull(PairingUri.parse("mtc://pair?host=macbook.local&port=70000"))
        assertNull(PairingUri.parse("mtc://pair?host=mac%20book&port=4242"))
    }
}
