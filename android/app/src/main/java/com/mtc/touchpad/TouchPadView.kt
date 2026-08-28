package com.mtc.touchpad

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RadialGradient
import android.graphics.Shader
import android.view.MotionEvent
import android.view.View

/** Millimeters in one Android physical pixel, using the device's reported DPI. */
internal fun mmPerPixel(xdpi: Float, ydpi: Float, densityDpi: Int): Float {
    val reported = listOf(xdpi, ydpi)
        .filter { it.isFinite() && it in 72f..1000f }
    val dpi = if (reported.isNotEmpty()) {
        reported.average().toFloat()
    } else {
        densityDpi.toFloat().takeIf { it.isFinite() && it > 0f } ?: 160f
    }
    return 25.4f / dpi
}

/**
 * The pad surface. Captures raw multi-touch and streams wire frames.
 * Features live multi-touch visualizer and Taptic Engine haptic responses.
 */
class TouchPadView(context: Context) : View(context) {

    companion object {
        private const val DEFAULT_DPI = 160
        private const val DRAG_ENGAGE_THRESHOLD_MM = 0.35f
        private const val SWIPE_COMMIT_THRESHOLD_MM = 10.0f
        private const val TAP_MAX_TRAVEL_MM = 1.5f
        private const val TAP_MAX_TIME_MS = 240L

        /**
         * Resend interval for resting contacts. 16 ms ≈ 60 Hz: dense
         * enough that the server's 250 ms silence watchdog never sees a
         * gap during a legitimate hold, sparse enough to be negligible
         * next to the frame rate an active gesture already produces.
         */
        private const val HEARTBEAT_MS = 16L
    }

    var sender: UdpSender? = null
    /** >1 = larger virtual surface, so each screen centimeter moves farther. */
    var scale: Float = 1f

    val haptics = Haptics(context)

    private val mmPerPx = mmPerPixel(
        resources.displayMetrics.xdpi,
        resources.displayMetrics.ydpi,
        resources.displayMetrics.densityDpi.takeIf { it > 0 } ?: DEFAULT_DPI,
    )

    private val cidByPointer = HashMap<Int, Int>()
    private val freeIds = ArrayDeque<Int>()
    private var nextCid = 1
    private val liftEchoRunnables = ArrayList<Runnable>()

    /** Last frame's contacts, resent by the heartbeat while they rest. */
    private var heldContacts: List<FrameEncoder.Contact> = emptyList()
    private var heartbeatRunnable: Runnable? = null

    // Gesture & Haptic tracking state
    private var touchDownTimeMs = 0L
    private var maxFingersInSession = 0
    private var dragEngageHapticFired = false
    private var swipeCommitHapticFired = false
    private var initialCentroidX = 0f
    private var initialCentroidY = 0f

    // Visualizer Paints
    private val touchPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.FILL
    }
    private val ringPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = 3f * resources.displayMetrics.density
    }
    private val textPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.WHITE
        textSize = 14f * resources.displayMetrics.density
        textAlign = Paint.Align.CENTER
    }

    // Active touch points for visualization
    private class VisualTouch(var x: Float, var y: Float, var id: Int)
    private val activeVisualTouches = ArrayList<VisualTouch>()

    init {
        setWillNotDraw(false)
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val n = activeVisualTouches.size
        if (n == 0) return

        // Color theme based on finger count:
        // 1F: Electric Cyan, 2F: Violet, 3F: Emerald, 4F+: Sunset Amber
        val (baseColor, glowColor) = when (n) {
            1 -> Pair(0xFF00E5FF.toInt(), 0x3300E5FF.toInt())
            2 -> Pair(0xFFA855F7.toInt(), 0x33A855F7.toInt())
            3 -> Pair(0xFF10B981.toInt(), 0x3310B981.toInt())
            else -> Pair(0xFFF59E0B.toInt(), 0x33F59E0B.toInt())
        }

        val radiusPx = 36f * resources.displayMetrics.density
        ringPaint.color = baseColor

        for (touch in activeVisualTouches) {
            // Glowing radial background
            touchPaint.shader = RadialGradient(
                touch.x, touch.y, radiusPx,
                intArrayOf(glowColor, 0x00000000),
                floatArrayOf(0.4f, 1.0f),
                Shader.TileMode.CLAMP
            )
            canvas.drawCircle(touch.x, touch.y, radiusPx, touchPaint)
            touchPaint.shader = null

            // Outer crisp halo ring
            canvas.drawCircle(touch.x, touch.y, radiusPx * 0.55f, ringPaint)

            // Inner touch dot
            touchPaint.color = baseColor
            canvas.drawCircle(touch.x, touch.y, 6f * resources.displayMetrics.density, touchPaint)
        }
    }

    private fun updateVisualTouches(event: MotionEvent) {
        activeVisualTouches.clear()
        for (i in 0 until event.pointerCount) {
            val pid = event.getPointerId(i)
            val cid = cidByPointer[pid] ?: (i + 1)
            activeVisualTouches.add(VisualTouch(event.getX(i), event.getY(i), cid))
        }
        invalidate()
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                parent?.requestDisallowInterceptTouchEvent(true)
                freeAll()
                touchDownTimeMs = System.currentTimeMillis()
                maxFingersInSession = 1
                dragEngageHapticFired = false
                swipeCommitHapticFired = false
                initialCentroidX = event.x * mmPerPx * scale
                initialCentroidY = event.y * mmPerPx * scale
                remember(event)
                updateVisualTouches(event)
                sendCurrent(event)
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                remember(event)
                val count = event.pointerCount
                if (count > maxFingersInSession) {
                    maxFingersInSession = count
                }
                if (count == 3 || count == 4) {
                    val c = getCentroidMm(event)
                    initialCentroidX = c.first
                    initialCentroidY = c.second
                }
                updateVisualTouches(event)
                sendCurrent(event)
            }
            MotionEvent.ACTION_MOVE -> {
                remember(event)
                val count = event.pointerCount
                if (count > maxFingersInSession) {
                    maxFingersInSession = count
                }

                // Haptic trigger for 3-finger drag start
                if (count == 3 && !dragEngageHapticFired) {
                    val (cx, cy) = getCentroidMm(event)
                    val dist = Math.hypot((cx - initialCentroidX).toDouble(), (cy - initialCentroidY).toDouble()).toFloat()
                    if (dist >= DRAG_ENGAGE_THRESHOLD_MM) {
                        dragEngageHapticFired = true
                        haptics.dragEngage(this)
                    }
                }
                // Haptic trigger for 4-finger swipe milestone commit
                if (count == 4 && !swipeCommitHapticFired) {
                    val (cx, cy) = getCentroidMm(event)
                    val dist = Math.hypot((cx - initialCentroidX).toDouble(), (cy - initialCentroidY).toDouble()).toFloat()
                    if (dist >= SWIPE_COMMIT_THRESHOLD_MM) {
                        swipeCommitHapticFired = true
                        haptics.swipeCommit(this)
                    }
                }

                updateVisualTouches(event)
                sendHistoricalAndCurrent(event)
            }

            MotionEvent.ACTION_POINTER_UP -> {
                val pid = event.getPointerId(event.actionIndex)
                freeId(pid)
                updateVisualTouches(event)
                if (cidByPointer.isEmpty()) echoLift() else sendCurrentSurvivors(event)
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                val duration = System.currentTimeMillis() - touchDownTimeMs
                // Only vibrate on actual tap-to-click (short press with minimal movement)
                if (duration <= TAP_MAX_TIME_MS) {
                    val (cx, cy) = getCentroidMm(event)
                    val dx = Math.abs(cx - initialCentroidX)
                    val dy = Math.abs(cy - initialCentroidY)
                    if (dx < TAP_MAX_TRAVEL_MM && dy < TAP_MAX_TRAVEL_MM) {
                        if (maxFingersInSession == 2) {
                            haptics.heavyClick(this) // 2-finger right click press
                        } else if (maxFingersInSession == 1) {
                            haptics.click(this) // 1-finger click press
                        }
                    }
                }
                freeAll()
                activeVisualTouches.clear()
                invalidate()
                echoLift()
            }
        }
        return true
    }

    private fun getCentroidMm(event: MotionEvent): Pair<Float, Float> {
        var sumX = 0f
        var sumY = 0f
        val n = event.pointerCount
        for (i in 0 until n) {
            sumX += event.getX(i)
            sumY += event.getY(i)
        }
        return Pair(sumX / n * mmPerPx * scale, sumY / n * mmPerPx * scale)
    }

    private fun remember(event: MotionEvent) {
        for (i in 0 until event.pointerCount) {
            val pid = event.getPointerId(i)
            if (!cidByPointer.containsKey(pid)) {
                val curX = event.getX(i) * mmPerPx * scale
                val curY = event.getY(i) * mmPerPx * scale
                var isSplit = false
                for (j in 0 until event.pointerCount) {
                    if (i != j && cidByPointer.containsKey(event.getPointerId(j))) {
                        val otherX = event.getX(j) * mmPerPx * scale
                        val otherY = event.getY(j) * mmPerPx * scale
                        // Only filter out impossible capacitive hardware ghosting (< 3.0mm).
                        // Real human fingers placed close together can naturally be 5-8mm apart.
                        if (Math.hypot((curX - otherX).toDouble(), (curY - otherY).toDouble()) < 3.0) {
                            isSplit = true
                            break
                        }
                    }
                }
                if (!isSplit) {
                    cidByPointer[pid] = if (freeIds.isNotEmpty()) freeIds.removeFirst() else (nextCid++ % 255 + 1)
                }
            }
        }
    }

    private fun freeId(pid: Int) {
        cidByPointer.remove(pid)?.let { freeIds.addLast(it) }
    }

    private fun freeAll() {
        cidByPointer.values.forEach { freeIds.addLast(it) }
        cidByPointer.clear()
        cancelLiftEcho()
        cancelHeartbeat()
    }

    private fun toMm(x: Float, y: Float): FrameEncoder.Contact {
        return FrameEncoder.Contact(
            id = 0, // set by caller
            x = x * mmPerPx * scale,
            y = y * mmPerPx * scale,
        )
    }

    private fun sendHistoricalAndCurrent(event: MotionEvent) {
        val s = sender ?: return
        val historySize = event.historySize
        // Stream historical frames if present for high sampling rate (120Hz/240Hz)
        for (h in 0 until historySize) {
            val list = ArrayList<FrameEncoder.Contact>(event.pointerCount)
            val histTicks = (event.getHistoricalEventTimeNanos(h) / 100_000L).toInt()
            for (i in 0 until event.pointerCount) {
                val pid = event.getPointerId(i)
                val cid = cidByPointer[pid] ?: continue
                val c = toMm(event.getHistoricalX(i, h), event.getHistoricalY(i, h))
                list.add(FrameEncoder.Contact(cid, c.x, c.y))
            }
            if (list.isNotEmpty()) {
                s.send(FrameEncoder.encode(s.nextSeq(), histTicks, false, list))
            }
        }
        // Then send current
        sendFramesOf(event)
    }

    private fun sendFramesOf(event: MotionEvent) {
        val s = sender ?: return
        val list = ArrayList<FrameEncoder.Contact>(event.pointerCount)
        for (i in 0 until event.pointerCount) {
            val pid = event.getPointerId(i)
            val cid = cidByPointer[pid] ?: continue
            val c = toMm(event.getX(i), event.getY(i))
            list.add(FrameEncoder.Contact(cid, c.x, c.y))
        }
        s.send(FrameEncoder.encode(s.nextSeq(), s.nowTicks(), false, list))
        armHeartbeat(list)
    }

    /**
     * Keep the frame stream alive while contacts rest.
     *
     * Android only delivers `ACTION_MOVE` when a pointer actually moves,
     * so a finger held still stops producing frames entirely. From the
     * far end that is indistinguishable from the client disappearing,
     * and the server's silence watchdog used to resolve it by
     * synthesizing a lift — which manufactured taps the user never made
     * and, at the wrong moment, whole double-clicks. A real trackpad
     * reports at a fixed rate whether or not anything moved; this makes
     * the phone behave the same way.
     *
     * Resent frames carry a fresh sequence number and a current
     * timestamp, so the receiver treats them as genuine "still here,
     * still at this position" samples rather than replays.
     */
    private fun armHeartbeat(list: List<FrameEncoder.Contact>) {
        cancelHeartbeat()
        if (list.isEmpty()) return
        heldContacts = list
        val r = object : Runnable {
            override fun run() {
                val s = sender ?: return
                val held = heldContacts
                if (held.isEmpty()) return
                s.send(FrameEncoder.encode(s.nextSeq(), s.nowTicks(), false, held))
                postDelayed(this, HEARTBEAT_MS)
            }
        }
        heartbeatRunnable = r
        postDelayed(r, HEARTBEAT_MS)
    }

    private fun cancelHeartbeat() {
        heartbeatRunnable?.let { removeCallbacks(it) }
        heartbeatRunnable = null
        heldContacts = emptyList()
    }

    private fun sendCurrent(event: MotionEvent) = sendFramesOf(event)

    /** POINTER_UP batches still carry valid positions of surviving fingers. */
    private fun sendCurrentSurvivors(event: MotionEvent) = sendFramesOf(event)

    /** All-lifted frame ×3 (now/+30ms/+90ms) — the one stateful transition. */
    private fun echoLift() {
        val s = sender ?: return
        cancelHeartbeat()
        s.send(FrameEncoder.encode(s.nextSeq(), s.nowTicks(), false, emptyList()))
        listOf(30L, 90L).forEach { delay ->
            val r = Runnable { sender?.send(FrameEncoder.encode(sender!!.nextSeq(), sender!!.nowTicks(), false, emptyList())) }
            liftEchoRunnables.add(r)
            postDelayed(r, delay)
        }
    }

    private fun cancelLiftEcho() {
        liftEchoRunnables.forEach { removeCallbacks(it) }
        liftEchoRunnables.clear()
    }
}
