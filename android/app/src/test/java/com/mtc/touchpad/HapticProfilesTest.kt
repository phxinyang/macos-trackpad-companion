package com.mtc.touchpad

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class HapticProfilesTest {
    @Test
    fun deep_press_is_a_sharp_peak_with_damped_tail() {
        assertArrayEquals(longArrayOf(0L, 11L, 4L, 8L), HapticProfiles.deepPressTimings)
        assertArrayEquals(intArrayOf(0, 255, 0, 86), HapticProfiles.deepPressAmplitudes(255))
        assertEquals(40, HapticProfiles.deepPressAmplitudes(1)[1])
    }

    @Test
    fun vendor_profile_is_limited_to_xiaomi_brands() {
        assertTrue(HapticProfiles.isXiaomiOrRedmi("Xiaomi", "23078RKD5C"))
        assertTrue(HapticProfiles.isXiaomiOrRedmi("unknown", "Redmi"))
        assertFalse(HapticProfiles.isXiaomiOrRedmi("Samsung", "SM-S918B"))
        assertEquals(163, HapticProfiles.XIAOMI_HARDWARE_EFFECT_ID)
    }
}
