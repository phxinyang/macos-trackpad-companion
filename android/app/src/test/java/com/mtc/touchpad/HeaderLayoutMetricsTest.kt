package com.mtc.touchpad

import org.junit.Assert.assertTrue
import org.junit.Assert.assertEquals
import org.junit.Test

class HeaderLayoutMetricsTest {
    @Test
    fun compact_capsule_has_room_for_all_persistent_actions() {
        assertTrue(
            "compact header width must not clip the fullscreen action",
            HeaderLayoutMetrics.COMPACT_WIDTH_DP >= HeaderLayoutMetrics.CONTENT_MIN_WIDTH_DP,
        )
    }

    @Test
    fun fullscreen_keeps_the_centered_pad_inset() {
        assertEquals(
            "fullscreen must keep the compact top inset instead of becoming edge-to-edge",
            PadLayoutMetrics.COMPACT_TOP_MARGIN_DP,
            PadLayoutMetrics.topMargin(fullscreen = true, connected = false, headerExpanded = true),
        )
        assertEquals(PadLayoutMetrics.SIDE_MARGIN_DP, PadLayoutMetrics.BOTTOM_MARGIN_DP)
    }
}
