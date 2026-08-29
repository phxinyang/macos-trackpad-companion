package com.mtc.touchpad

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.os.SystemClock
import android.view.MotionEvent
import android.view.View
/**
 * A software Force Click affordance. It does not depend on MotionEvent.pressure:
 * after the configured hold time it reports a standard mouse-button hold to the
 * Mac, and releases it when the finger leaves the bar.
 */
class DeepPressBarView(context: Context) : View(context) {

    private val density = resources.displayMetrics.density

    init {
        isClickable = true
        isFocusable = true
        contentDescription = "深按，按住发送左键"
    }

    var holdDurationMs: Long = 650L
        set(value) {
            field = value.coerceIn(200L, 2_000L)
            invalidate()
        }

    var onDeepPress: ((Boolean) -> Unit)? = null
    var onHeartbeat: (() -> Unit)? = null
    var haptics: Haptics? = null

    val isDeepPressed: Boolean
        get() = deepPressed

    private var pressed = false
    private var deepPressed = false
    private var downAt = 0L

    private val backgroundPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = 0xDD24242E.toInt()
    }
    private val progressPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = 0xFF007AFF.toInt()
    }
    private val strokePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = 1.5f * density
        color = 0x66FFFFFF
    }
    private val textPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.WHITE
        textSize = 14f * density
        textAlign = Paint.Align.CENTER
        typeface = android.graphics.Typeface.DEFAULT_BOLD
    }

    /** Apply the active product theme without changing the press state machine. */
    fun applyTheme(background: Int, progress: Int, stroke: Int, text: Int) {
        backgroundPaint.color = background
        progressPaint.color = progress
        strokePaint.color = stroke
        textPaint.color = text
        invalidate()
    }
    private val tick = object : Runnable {
        override fun run() {
            if (!pressed) return
            val elapsed = SystemClock.uptimeMillis() - downAt
            if (!deepPressed && elapsed >= holdDurationMs) {
                deepPressed = true
                onDeepPress?.invoke(true)
                haptics?.deepPress(this@DeepPressBarView)
            }
            if (deepPressed) onHeartbeat?.invoke()
            invalidate()
            postDelayed(this, 16L)
        }
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        // Keep the deep-press affordance a control, not a pill. Capping the
        // radius also keeps the shape stable when users change its height.
        val radius = minOf(height * 0.25f, 10f * density)
        val bounds = RectF(0.5f, 0.5f, width - 0.5f, height - 0.5f)
        canvas.drawRoundRect(bounds, radius, radius, backgroundPaint)
        if (pressed) {
            val progress = if (deepPressed) {
                1f
            } else {
                ((SystemClock.uptimeMillis() - downAt).toFloat() / holdDurationMs)
                    .coerceIn(0f, 1f)
            }
            if (progress > 0f) {
                val progressBounds = RectF(0f, 0f, width.toFloat() * progress, height.toFloat())
                val progressRadius = minOf(radius, progressBounds.width() / 2f)
                val clipPath = Path().apply {
                    addRoundRect(bounds, radius, radius, Path.Direction.CW)
                }
                canvas.save()
                canvas.clipPath(clipPath)
                canvas.drawRoundRect(progressBounds, progressRadius, progressRadius, progressPaint)
                canvas.restore()
            }
        }
        canvas.drawRoundRect(bounds, radius, radius, strokePaint)
        val label = if (deepPressed) "按下" else "深按"
        val baseline = height / 2f - (textPaint.ascent() + textPaint.descent()) / 2f
        canvas.drawText(label, width / 2f, baseline, textPaint)
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                pressed = true
                deepPressed = false
                downAt = SystemClock.uptimeMillis()
                animatePressIn()
                // Force Click is a two-stage interaction: the first click is
                // felt immediately, and the deeper confirmation follows only
                // after the hold threshold is crossed.
                haptics?.click(this@DeepPressBarView)
                removeCallbacks(tick)
                post(tick)
                invalidate()
                return true
            }
            MotionEvent.ACTION_MOVE -> {
                // Leaving the bar cancels the pending/active button, matching
                // a real control whose press target is no longer held.
                if (event.x < 0f || event.x > width || event.y < 0f || event.y > height) {
                    finishPress()
                }
                return true
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                finishPress()
                return true
            }
        }
        return true
    }

    private fun finishPress() {
        if (!pressed && !deepPressed) return
        removeCallbacks(tick)
        if (deepPressed) onDeepPress?.invoke(false)
        pressed = false
        deepPressed = false
        animatePressOut()
        invalidate()
    }

    private fun animatePressIn() {
        animate().cancel()
        animate()
            .scaleX(InteractionMetrics.PRESS_SCALE)
            .scaleY(InteractionMetrics.PRESS_SCALE)
            .translationY(0.5f)
            .alpha(InteractionMetrics.PRESS_ALPHA)
            .setDuration(InteractionMetrics.PRESS_DOWN_MS)
            .setInterpolator(InteractionMetrics.PRESS_DOWN_INTERPOLATOR)
            .start()
    }

    private fun animatePressOut() {
        animate().cancel()
        animate()
            .scaleX(1f)
            .scaleY(1f)
            .translationY(0f)
            .alpha(1f)
            .setDuration(InteractionMetrics.PRESS_UP_MS)
            .setInterpolator(InteractionMetrics.PRESS_UP_INTERPOLATOR)
            .start()
    }

    fun cancelPress() {
        if (pressed) finishPress() else removeCallbacks(tick)
    }
}
