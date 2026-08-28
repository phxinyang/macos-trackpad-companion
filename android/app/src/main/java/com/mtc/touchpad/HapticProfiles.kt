package com.mtc.touchpad

/** Device-independent timing data for the click profiles used by [Haptics]. */
internal object HapticProfiles {
    const val XIAOMI_HARDWARE_EFFECT_ID = 163

    val normalClickTimings = longArrayOf(0L, 8L, 3L, 5L)
    val normalClickAmplitudes = intArrayOf(0, 200, 0, 64)
    val deepPressTimings = longArrayOf(0L, 11L, 4L, 8L)

    fun deepPressAmplitudes(peak: Int): IntArray {
        val safePeak = peak.coerceIn(40, 255)
        return intArrayOf(0, safePeak, 0, (safePeak * 0.34f).toInt().coerceAtLeast(1))
    }

    fun isXiaomiOrRedmi(manufacturer: String, brand: String): Boolean =
        manufacturer.equals("Xiaomi", ignoreCase = true) ||
            brand.equals("Redmi", ignoreCase = true) ||
            brand.equals("Xiaomi", ignoreCase = true)
}
