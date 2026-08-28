package com.mtc.touchpad

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.RadialGradient
import android.graphics.Shader
import android.os.SystemClock
import android.view.MotionEvent
import android.view.View
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin

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
        private const val TRAIL_POINTS = 8
        private const val RELEASE_FADE_MS = 280L

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

    // Visualizer Paints. The touch markers intentionally use restrained
    // spectral colors so the glass rim remains the primary material cue.
    private val touchPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.FILL
    }
    private val ringPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = 1.5f * resources.displayMetrics.density
    }
    private val trailPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeCap = Paint.Cap.ROUND
    }
    private val density = resources.displayMetrics.density
    private val visualColors = intArrayOf(
        0xFF7DD8FF.toInt(),
        0xFFC1A4FF.toInt(),
        0xFF83E7B2.toInt(),
        0xFFFFC979.toInt(),
    )

    // Visual state is keyed by Android pointer id, not contact id. This keeps
    // each marker continuous when another finger joins or lifts mid-gesture.
    private data class TrailPoint(val x: Float, val y: Float, val timeMs: Long)
    private class VisualTouch(
        val pointerId: Int,
        var x: Float,
        var y: Float,
        var velocityX: Float,
        var velocityY: Float,
        var lastTimeMs: Long,
    ) {
        val trail = ArrayDeque<TrailPoint>()
        var releasedAtMs = 0L
    }

    private val activeVisualTouches = LinkedHashMap<Int, VisualTouch>()
    private val releasedVisualTouches = ArrayList<VisualTouch>()
    private var visualFrameTimeMs = 0L

    init {
        setWillNotDraw(false)
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        visualFrameTimeMs = if (visualFrameTimeMs == 0L) {
            SystemClock.uptimeMillis()
        } else {
            max(visualFrameTimeMs, SystemClock.uptimeMillis())
        }

        activeVisualTouches.values.forEach { drawVisualTouch(canvas, it, 1f, visualFrameTimeMs) }
        val iterator = releasedVisualTouches.iterator()
        while (iterator.hasNext()) {
            val touch = iterator.next()
            val elapsed = visualFrameTimeMs - touch.releasedAtMs
            val progress = (elapsed / RELEASE_FADE_MS.toFloat()).coerceIn(0f, 1f)
            if (progress >= 1f) {
                iterator.remove()
            } else {
                // A quick ease-out leaves a soft optical afterimage without
                // making the pad feel like it is emitting particles.
                val alpha = 1f - progress * progress
                drawVisualTouch(canvas, touch, alpha, visualFrameTimeMs)
            }
        }

        if (activeVisualTouches.isNotEmpty() || releasedVisualTouches.isNotEmpty()) {
            postInvalidateOnAnimation()
        }
    }

    private fun drawVisualTouch(canvas: Canvas, touch: VisualTouch, alpha: Float, nowMs: Long) {
        val color = visualColors[Math.floorMod(touch.pointerId, visualColors.size)]
        val speed = hypot(touch.velocityX, touch.velocityY)
        val direction = if (speed > 0.5f) atan2(touch.velocityY, touch.velocityX) else -Math.PI.toFloat() / 2f
        val pulse = (sin(nowMs * 0.010f + touch.pointerId) * 0.5f + 0.5f) * density
        val coreRadius = (5.5f + min(speed * 0.018f, 2.5f)) * density
        val haloRadius = (22f + min(speed * 0.045f, 9f) + pulse * 2f) * density
        val glowRadius = haloRadius * 2.25f

        // Broad glow: the alpha is low enough to preserve the sampled glass
        // colors beneath it, while movement visibly changes the light field.
        touchPaint.shader = RadialGradient(
            touch.x, touch.y, glowRadius,
            intArrayOf(withAlpha(color, (0x38 * alpha).toInt()), withAlpha(color, (0x12 * alpha).toInt()), 0x00000000),
            floatArrayOf(0f, 0.42f, 1f),
            Shader.TileMode.CLAMP,
        )
        canvas.drawCircle(touch.x, touch.y, glowRadius, touchPaint)
        touchPaint.shader = null

        // Velocity ribbon. It is drawn behind the core and fades toward the
        // previous positions, producing a liquid smear instead of a cursor dot.
        if (touch.trail.isNotEmpty() && speed > 0.8f) {
            trailPaint.color = withAlpha(color, (0x34 * alpha).toInt())
            trailPaint.strokeWidth = (4f + min(speed * 0.04f, 7f)) * density
            var previousX = touch.x
            var previousY = touch.y
            touch.trail.reversed().forEachIndexed { index, point ->
                val age = (index + 1) / (touch.trail.size + 1f)
                trailPaint.alpha = (0x50 * alpha * (1f - age)).toInt().coerceIn(0, 255)
                canvas.drawLine(previousX, previousY, point.x, point.y, trailPaint)
                previousX = point.x
                previousY = point.y
            }
            trailPaint.alpha = 255
        }

        // Two optical rings: a thin colored rim and a softer inner ring. The
        // changing radius makes a resting finger feel seated in the glass.
        ringPaint.color = withAlpha(color, (0xB0 * alpha).toInt())
        ringPaint.strokeWidth = 1.35f * density
        canvas.drawCircle(touch.x, touch.y, haloRadius, ringPaint)
        ringPaint.color = withAlpha(Color.WHITE, (0x55 * alpha).toInt())
        ringPaint.strokeWidth = 1f * density
        canvas.drawCircle(touch.x, touch.y, haloRadius * 0.72f, ringPaint)

        // Directional specular streak, aligned to the current motion vector.
        val streakLength = (18f + min(speed * 0.12f, 26f)) * density
        val streakWidth = 3f * density
        val cx = touch.x - cos(direction) * haloRadius * 0.18f
        val cy = touch.y - sin(direction) * haloRadius * 0.18f
        canvas.save()
        canvas.rotate(Math.toDegrees(direction.toDouble()).toFloat(), cx, cy)
        touchPaint.shader = LinearGradient(
            cx - streakLength * 0.5f, cy,
            cx + streakLength * 0.5f, cy,
            intArrayOf(0x00FFFFFF, withAlpha(Color.WHITE, (0x7A * alpha).toInt()), 0x00FFFFFF),
            null,
            Shader.TileMode.CLAMP,
        )
        canvas.drawRoundRect(
            cx - streakLength * 0.5f,
            cy - streakWidth * 0.5f,
            cx + streakLength * 0.5f,
            cy + streakWidth * 0.5f,
            streakWidth,
            streakWidth,
            touchPaint,
        )
        touchPaint.shader = null
        canvas.restore()

        // Crisp contact point and a tiny white specular dot.
        touchPaint.color = withAlpha(color, (0xD8 * alpha).toInt())
        canvas.drawCircle(touch.x, touch.y, coreRadius, touchPaint)
        touchPaint.color = withAlpha(Color.WHITE, (0xDC * alpha).toInt())
        canvas.drawCircle(touch.x - coreRadius * 0.30f, touch.y - coreRadius * 0.30f, coreRadius * 0.34f, touchPaint)
    }

    private fun withAlpha(color: Int, alpha: Int): Int = Color.argb(alpha.coerceIn(0, 255), Color.red(color), Color.green(color), Color.blue(color))

    private fun updateVisualTouches(event: MotionEvent) {
        val seen = HashSet<Int>(event.pointerCount)
        val eventTime = event.eventTime
        val skipIndex = if (event.actionMasked == MotionEvent.ACTION_POINTER_UP) event.actionIndex else -1
        for (i in 0 until event.pointerCount) {
            if (i == skipIndex) continue
            val pid = event.getPointerId(i)
            seen += pid
            val x = event.getX(i)
            val y = event.getY(i)
            val touch = activeVisualTouches[pid]
            if (touch == null) {
                activeVisualTouches[pid] = VisualTouch(pid, x, y, 0f, 0f, eventTime)
            } else {
                val dt = (eventTime - touch.lastTimeMs).coerceIn(1L, 64L).toFloat()
                val dx = x - touch.x
                val dy = y - touch.y
                touch.trail.addLast(TrailPoint(touch.x, touch.y, eventTime))
                while (touch.trail.size > TRAIL_POINTS) touch.trail.removeFirst()
                // Smooth the instantaneous velocity so the ribbon does not
                // jitter on high-rate Android MotionEvent batches.
                touch.velocityX = touch.velocityX * 0.60f + (dx / dt * 16f) * 0.40f
                touch.velocityY = touch.velocityY * 0.60f + (dy / dt * 16f) * 0.40f
                touch.x = x
                touch.y = y
                touch.lastTimeMs = eventTime
            }
        }
        if (event.actionMasked == MotionEvent.ACTION_POINTER_UP) {
            val index = event.actionIndex
            releaseVisualTouch(event.getPointerId(index), event.getX(index), event.getY(index), eventTime)
        }
        activeVisualTouches.keys.toList().forEach { pid ->
            if (pid !in seen) releaseVisualTouch(pid, activeVisualTouches[pid]?.x ?: 0f, activeVisualTouches[pid]?.y ?: 0f, eventTime)
        }
        invalidate()
    }

    private fun releaseVisualTouch(pointerId: Int, x: Float, y: Float, eventTime: Long) {
        val touch = activeVisualTouches.remove(pointerId) ?: return
        touch.x = x
        touch.y = y
        touch.releasedAtMs = eventTime
        releasedVisualTouches += touch
    }

    private fun releaseAllVisualTouches(eventTime: Long) {
        activeVisualTouches.values.toList().forEach { touch ->
            touch.releasedAtMs = eventTime
            releasedVisualTouches += touch
        }
        activeVisualTouches.clear()
        invalidate()
    }

    private fun clearVisualTouches() {
        activeVisualTouches.clear()
        releasedVisualTouches.clear()
        invalidate()
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                parent?.requestDisallowInterceptTouchEvent(true)
                freeAll()
                clearVisualTouches()
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
                // ACTION_CANCEL is a transport/lifecycle abort, not a user
                // tap. Never emit click feedback on cancellation; still send
                // the lift frame so the Mac cannot retain stale contacts.
                if (event.actionMasked == MotionEvent.ACTION_UP) {
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
                }
                freeAll()
                if (event.actionMasked == MotionEvent.ACTION_UP) {
                    releaseAllVisualTouches(event.eventTime)
                } else {
                    clearVisualTouches()
                }
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
