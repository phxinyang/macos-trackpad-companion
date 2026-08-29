package com.mtc.touchpad

import org.junit.Assert.assertTrue
import org.junit.Test

class HeaderLayoutMetricsTest {
    @Test
    fun compact_capsule_has_room_for_all_persistent_actions() {
        assertTrue(
            "compact header width must not clip the fullscreen action",
            HeaderLayoutMetrics.COMPACT_WIDTH_DP >= HeaderLayoutMetrics.CONTENT_MIN_WIDTH_DP,
        )
    }
}
