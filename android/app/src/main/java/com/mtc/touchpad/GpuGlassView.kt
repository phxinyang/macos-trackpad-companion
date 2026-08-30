package com.mtc.touchpad

import android.annotation.TargetApi
import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapShader
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.graphics.RuntimeShader
import android.graphics.Shader
import android.os.Build
import android.view.View
import android.widget.FrameLayout
import kotlin.math.max

/**
 * Single-pass glass compositor for API 31+. The background is captured once
 * at half resolution and the shader performs refraction, edge dispersion and
 * touch lighting in one draw. QWEA0 remains the fallback for older devices.
 */
@TargetApi(Build.VERSION_CODES.S)
class GpuGlassView(context: Context) : FrameLayout(context) {

    companion object {
        private const val BACKDROP_SCALE = 0.5f
        private const val SHADER = """
            uniform shader scene;
            uniform float2 canvasSize;
            uniform float2 touch;
            uniform float active;
            uniform float refraction;
            uniform float chroma;
            uniform float saturation;
            uniform float highlight;
            uniform float3 edgeTint;

            half4 sampleScene(float2 p) {
                return scene.eval(p);
            }

            half4 main(float2 p) {
                float2 uv = p / canvasSize;
                float2 q = uv * 2.0 - 1.0;
                float radius = length(q);
                float2 normal = q / max(radius, 0.001);
                float edge = smoothstep(0.28, 0.96, radius);
                float2 bend = normal * refraction * edge;
                float2 fringe = normal * chroma * edge;

                half4 center = sampleScene(p + bend);
                half4 red = sampleScene(p + bend + fringe);
                half4 blue = sampleScene(p + bend - fringe);
                half3 rgb = half3(red.r, center.g, blue.b);
                float luminance = dot(rgb, half3(0.2126, 0.7152, 0.0722));
                rgb = mix(half3(luminance), rgb, saturation);

                float rim = smoothstep(0.36, 0.94, radius);
                float touchGlow = exp(-distance(uv, touch) * 8.0) * active;
                rgb += edgeTint * 0.14 * rim * highlight;
                rgb += edgeTint * 0.24 * touchGlow;
                return half4(clamp(rgb, 0.0, 1.0), 1.0);
            }
        """
    }

    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val fallbackPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val clipPath = Path()
    private val bounds = RectF()
    private val shaderMatrix = Matrix()
    private var glassShader: RuntimeShader? = runCatching { RuntimeShader(SHADER) }.getOrNull()
    private var sceneShader: BitmapShader? = null
    private var backdropBitmap: Bitmap? = null
    private var backdropSource: View? = null
    private var cornerRadiusPx = 30f * resources.displayMetrics.density
    private var touchX = 0f
    private var touchY = 0f
    private var touchActive = false
    private var refractionPx = 24f
    private var chromaPx = 7f
    private var saturationAmount = 1.12f
    private var highlightAmount = 0.9f
    private val captureTask = Runnable { captureBackdrop() }
    private val settleCaptureTask = Runnable { captureBackdrop() }

    init {
        setWillNotDraw(false)
        clipChildren = false
        clipToPadding = false
        if (glassShader == null) setBackgroundColor(Color.TRANSPARENT)
    }

    fun setBackdropSource(source: View) {
        backdropSource = source
        scheduleCapture(80L)
    }

    fun setInteraction(active: Boolean, x: Float = touchX, y: Float = touchY) {
        touchActive = active
        touchX = x.coerceIn(0f, width.toFloat())
        touchY = y.coerceIn(0f, height.toFloat())
        invalidate()
    }

    /** Maps the shared LiquidPreset into the cheaper single-pass shader. */
    fun setOptics(
        refractionHeight: Float,
        dispersionStrength: Float,
        saturation: Float,
        highlightOpacity: Float,
        accentColor: Int = Color.rgb(99, 214, 255),
    ) {
        refractionPx = (refractionHeight / 10f).coerceIn(8f, 42f)
        chromaPx = (dispersionStrength * 35f).coerceIn(2f, 10f)
        // Theme presets describe a perceptual saturation preference, not a
        // literal RGB multiplier. Compress the range so colorful wallpapers
        // retain their hue without becoming neon after the lens pass.
        val requestedSaturation = (saturation / 100f).coerceIn(0.65f, 1.8f)
        saturationAmount = (1f + (requestedSaturation - 1f) * 0.62f).coerceIn(0.78f, 1.5f)
        highlightAmount = (highlightOpacity / 100f).coerceIn(0.2f, 1.4f)
        glassShader?.setFloatUniform(
            "edgeTint",
            Color.red(accentColor) / 255f,
            Color.green(accentColor) / 255f,
            Color.blue(accentColor) / 255f,
        )
        invalidate()
    }

    fun setFullscreen(fullscreen: Boolean) {
        cornerRadiusPx = if (fullscreen) 0f else 30f * resources.displayMetrics.density
        updateClipPath()
        invalidate()
    }

    private fun captureBackdrop() {
        val source = backdropSource ?: return
        if (width <= 0 || height <= 0 || source.width <= 0 || source.height <= 0) {
            scheduleCapture(32L)
            return
        }
        val targetLocation = IntArray(2)
        val sourceLocation = IntArray(2)
        getLocationOnScreen(targetLocation)
        source.getLocationOnScreen(sourceLocation)
        val bitmapWidth = max(1, (width * BACKDROP_SCALE).toInt())
        val bitmapHeight = max(1, (height * BACKDROP_SCALE).toInt())
        val next = Bitmap.createBitmap(bitmapWidth, bitmapHeight, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(next)
        canvas.scale(BACKDROP_SCALE, BACKDROP_SCALE)
        canvas.translate(
            -(targetLocation[0] - sourceLocation[0]).toFloat(),
            -(targetLocation[1] - sourceLocation[1]).toFloat(),
        )
        source.draw(canvas)
        backdropBitmap?.let { if (!it.isRecycled) it.recycle() }
        backdropBitmap = next
        sceneShader = BitmapShader(next, Shader.TileMode.CLAMP, Shader.TileMode.CLAMP).also { shader ->
            shader.setFilterMode(BitmapShader.FILTER_MODE_LINEAR)
            shaderMatrix.reset()
            // BitmapShader's local matrix maps the bitmap into view space.
            // The captured texture is half-resolution, so it must be scaled
            // up to the compositor bounds (using the inverse ratio would
            // leave the scene confined to the top-left quarter in fullscreen).
            shaderMatrix.setScale(width / next.width.toFloat(), height / next.height.toFloat())
            shader.setLocalMatrix(shaderMatrix)
        }
        sceneShader?.let { glassShader?.setInputShader("scene", it) }
        invalidate()
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        updateClipPath()
        if (backdropSource != null) scheduleCapture(24L)
        scheduleSettledCapture()
    }

    private fun updateClipPath() {
        bounds.set(0f, 0f, width.toFloat(), height.toFloat())
        clipPath.reset()
        clipPath.addRoundRect(bounds, cornerRadiusPx, cornerRadiusPx, Path.Direction.CW)
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        ensureClipPath()
        val shader = glassShader
        val source = sceneShader
        if (width <= 0 || height <= 0) return
        if (shader == null || source == null) {
            // The backdrop capture is asynchronous and RuntimeShader can be
            // unavailable on a few vendor builds. Keep a stable material while
            // either lower layer is warming up instead of exposing a transparent
            // hole that looks like the glass surface disappeared.
            fallbackPaint.shader = null
            fallbackPaint.color = 0x24FFFFFF
            canvas.save()
            canvas.clipPath(clipPath)
            canvas.drawRect(bounds, fallbackPaint)
            canvas.restore()
            return
        }
        shader.setFloatUniform("canvasSize", width.toFloat(), height.toFloat())
        shader.setFloatUniform(
            "touch",
            if (width > 0) touchX / width else 0.5f,
            if (height > 0) touchY / height else 0.5f,
        )
        shader.setFloatUniform("active", if (touchActive) 1f else 0f)
        shader.setFloatUniform("refraction", refractionPx)
        shader.setFloatUniform("chroma", chromaPx)
        shader.setFloatUniform("saturation", saturationAmount)
        shader.setFloatUniform("highlight", highlightAmount)
        paint.shader = shader
        canvas.save()
        canvas.clipPath(clipPath)
        canvas.drawRect(bounds, paint)
        canvas.restore()
        paint.shader = null
    }

    override fun dispatchDraw(canvas: Canvas) {
        ensureClipPath()
        canvas.save()
        canvas.clipPath(clipPath)
        super.dispatchDraw(canvas)
        canvas.restore()
    }

    private fun ensureClipPath() {
        if (bounds.width() != width.toFloat() || bounds.height() != height.toFloat()) {
            updateClipPath()
        }
    }

    override fun onDetachedFromWindow() {
        removeCallbacks(captureTask)
        removeCallbacks(settleCaptureTask)
        backdropSource = null
        sceneShader = null
        backdropBitmap?.let { if (!it.isRecycled) it.recycle() }
        backdropBitmap = null
        super.onDetachedFromWindow()
    }

    private fun scheduleCapture(delayMs: Long) {
        removeCallbacks(captureTask)
        postDelayed(captureTask, delayMs)
    }

    private fun scheduleSettledCapture() {
        removeCallbacks(settleCaptureTask)
        if (backdropSource != null) postDelayed(settleCaptureTask, 380L)
    }
}
