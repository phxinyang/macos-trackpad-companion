package com.mtc.touchpad

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
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
        strokeWidth = 1.5f * resources.displayMetrics.density
        color = 0x66FFFFFF
    }
    private val textPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.WHITE
        textSize = 14f * resources.displayMetrics.density
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
        val radius = height * 0.32f
        val bounds = RectF(0.5f, 0.5f, width - 0.5f, height - 0.5f)
        canvas.drawRoundRect(bounds, radius, radius, backgroundPaint)
        if (pressed) {
            val progress = if (deepPressed) {
                1f
            } else {
                ((SystemClock.uptimeMillis() - downAt).toFloat() / holdDurationMs)
                    .coerceIn(0f, 1f)
            }
            val progressBounds = RectF(0f, 0f, width.toFloat() * progress, height.toFloat())
            canvas.save()
            canvas.clipRect(bounds)
            canvas.drawRoundRect(progressBounds, radius, radius, progressPaint)
            canvas.restore()
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
        removeCallbacks(tick)
        if (deepPressed) onDeepPress?.invoke(false)
        pressed = false
        deepPressed = false
        invalidate()
    }

    fun cancelPress() {
        if (pressed) finishPress() else removeCallbacks(tick)
    }
}
