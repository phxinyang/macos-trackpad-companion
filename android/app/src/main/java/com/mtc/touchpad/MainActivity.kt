package com.mtc.touchpad

import android.app.Activity
import android.content.Intent
import android.content.SharedPreferences
import android.graphics.BitmapFactory
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.ColorMatrix
import android.graphics.ColorMatrixColorFilter
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.RadialGradient
import android.graphics.Rect
import android.graphics.Shader
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.view.animation.DecelerateInterpolator
import android.widget.Button
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import android.view.animation.PathInterpolator
import com.example.liquidglass.GlassAccessibilityMode
import com.example.liquidglass.BlurMethod
import com.example.liquidglass.GlassMaterial
import com.example.liquidglass.LiquidGlassView
import java.io.InputStream
import kotlin.math.max

private val CURATED_THEME_KEYS = setOf(
    "light-glass", "dark-glass", "ocean-glass", "sunset-glass", "aurora-glass", "graphite-glass",
    "custom-glass",
    "tokyo-night", "nord", "dracula", "solarized-dark", "catppuccin-mocha", "monokai",
    "classic-light", "classic-dark", "high-contrast",
)

private enum class ThemeMode(val key: String, val title: String, val detail: String) {
    LIGHT_GLASS("light-glass", "晨曦玻璃", "明亮、通透，默认外观"),
    DARK_GLASS("dark-glass", "夜幕玻璃", "深色背景与半透明控制层"),
    OCEAN_GLASS("ocean-glass", "海洋玻璃", "蓝青色场与更强折射"),
    SUNSET_GLASS("sunset-glass", "日落玻璃", "暖色场与柔和彩边"),
    AURORA_GLASS("aurora-glass", "极光玻璃", "青绿与紫色动态光场"),
    GRAPHITE_GLASS("graphite-glass", "石墨玻璃", "低亮度深色液态玻璃"),
    CUSTOM_GLASS("custom-glass", "自定义液态玻璃", "可替换壁纸与可调折射"),
    DROPLET_GLASS("droplet-glass", "凝露水滴", "局部凸面折射与边缘高光"),
    RIPPLE_WATER("ripple-water", "触控水波", "触摸时才出现的局部波纹"),
    RAIN_GLASS("rain-glass", "雨痕玻璃", "低密度滑落水滴与冷色玻璃"),
    PRISM_CRYSTAL("prism-crystal", "棱镜晶体", "彩色边缘、晶体折射与闪光"),
    GEL_SURFACE("gel-surface", "软胶表面", "轻微弹性形变与柔和高光"),
    LIQUID_METAL("liquid-metal", "液态金属", "镜面金属、流动高光与深色底"),
    PAPER_TEXTURE("paper-texture", "纸张纹理", "温暖纸面与细颗粒纤维"),
    HOLOGRAPHIC("holographic", "全息彩膜", "随角度变化的虹彩薄膜"),
    RETRO_LCD("retro-lcd", "复古 LCD", "扫描线、像素网格与荧光绿"),
    CERAMIC("ceramic", "陶瓷白", "不透明陶瓷、柔和阴影与清晰边界"),
    TOKYO_NIGHT("tokyo-night", "Tokyo Night", "蓝紫色编辑器主题"),
    NORD("nord", "Nord", "冷色北欧编辑器主题"),
    DRACULA("dracula", "Dracula", "紫色高对比编辑器主题"),
    SOLARIZED_DARK("solarized-dark", "Solarized Dark", "青色低对比编辑器主题"),
    CATPPUCCIN_MOCHA("catppuccin-mocha", "Catppuccin Mocha", "柔和深色编辑器主题"),
    MONOKAI("monokai", "Monokai", "经典代码编辑器主题"),
    CLASSIC_LIGHT("classic-light", "经典浅色", "纯色表面，关闭玻璃层"),
    CLASSIC_DARK("classic-dark", "经典深色", "纯色深色，低干扰"),
    HIGH_CONTRAST("high-contrast", "高对比", "黑白边界，优先可读性");

    companion object {
        fun from(key: String?): ThemeMode = values().firstOrNull { it.key == key && it.key in CURATED_THEME_KEYS } ?: LIGHT_GLASS
    }
}

private val CURATED_THEME_MODES = listOf(
    ThemeMode.LIGHT_GLASS,
    ThemeMode.DARK_GLASS,
    ThemeMode.OCEAN_GLASS,
    ThemeMode.SUNSET_GLASS,
    ThemeMode.AURORA_GLASS,
    ThemeMode.GRAPHITE_GLASS,
    ThemeMode.CUSTOM_GLASS,
    ThemeMode.TOKYO_NIGHT,
    ThemeMode.NORD,
    ThemeMode.DRACULA,
    ThemeMode.SOLARIZED_DARK,
    ThemeMode.CATPPUCCIN_MOCHA,
    ThemeMode.MONOKAI,
    ThemeMode.CLASSIC_LIGHT,
    ThemeMode.CLASSIC_DARK,
    ThemeMode.HIGH_CONTRAST,
)

private data class LiquidPreset(
    val bevelWidth: Float,
    val refractionHeight: Float,
    val dispersionStrength: Float,
    val blurAmount: Float,
    val saturation: Float,
    val highlightOpacity: Float,
    val adaptiveTint: Boolean,
)

private enum class MaterialKind {
    LIQUID_GLASS,
    DROPLET_GLASS,
    RIPPLE_WATER,
    RAIN_GLASS,
    PRISM_CRYSTAL,
    GEL_SURFACE,
    LIQUID_METAL,
    PAPER_TEXTURE,
    HOLOGRAPHIC,
    RETRO_LCD,
    CERAMIC,
}

private val LIQUID_DEFAULT = LiquidPreset(46f, 240f, .20f, .030f, 150f, 86f, false)

// Keep one predictable rendering profile across devices. The lens remains
// optically rich, while backdrop and blur buffers stay bounded on high-DPI
// phones and tablets.
private const val WALLPAPER_MAX_EDGE_PX = 1600
private const val GLASS_GLOBAL_DOWNSAMPLE = 0.5f
private const val GLASS_DOWNSAMPLE_SCALE = 3

internal object HeaderLayoutMetrics {
    const val COMPACT_WIDTH_DP = 236
    const val CONTENT_MIN_WIDTH_DP = 223
}

internal object PadLayoutMetrics {
    const val SIDE_MARGIN_DP = 18
    const val COMPACT_TOP_MARGIN_DP = 18
    const val EXPANDED_TOP_MARGIN_DP = 68
    const val BOTTOM_MARGIN_DP = 18

    fun topMargin(fullscreen: Boolean, connected: Boolean, headerExpanded: Boolean): Int =
        if (fullscreen || (connected && !headerExpanded)) COMPACT_TOP_MARGIN_DP else EXPANDED_TOP_MARGIN_DP
}

internal object InteractionMetrics {
    const val BUTTON_RADIUS_DP = 8
    const val PRESS_SCALE = 0.992f
    const val PRESS_ALPHA = 0.96f
    const val PRESS_DOWN_MS = 95L
    const val PRESS_UP_MS = 175L
    const val FULLSCREEN_ENTER_MS = 260L
    const val FULLSCREEN_EXIT_MS = 220L
    const val FULLSCREEN_ENTER_SCALE = 0.985f
    const val FULLSCREEN_EXIT_SCALE = 0.995f
    val PRESS_DOWN_INTERPOLATOR = PathInterpolator(0.2f, 0f, 0f, 1f)
    val PRESS_UP_INTERPOLATOR = DecelerateInterpolator(1.35f)
    // A short ease-out with a soft tail matches the way macOS hides chrome:
    // the surface stays anchored while the controls leave its visual field.
    val FULLSCREEN_ENTER_INTERPOLATOR = PathInterpolator(0.16f, 1f, 0.3f, 1f)
    val FULLSCREEN_EXIT_INTERPOLATOR = PathInterpolator(0.2f, 0f, 0f, 1f)
}

private data class ThemePalette(
    val canvas: Int,
    val chrome: Int,
    val chromeStroke: Int,
    val button: Int,
    val buttonStroke: Int,
    val input: Int,
    val pad: Int,
    val padStroke: Int,
    val label: Int,
    val secondary: Int,
    val accent: Int,
    val success: Int,
    val warning: Int,
    val danger: Int,
    val deep: Int,
    val deepProgress: Int,
    val deepStroke: Int,
    val deepText: Int,
    val usesLiquidGlass: Boolean,
    val sceneStart: Int = canvas,
    val sceneMid: Int = canvas,
    val sceneEnd: Int = canvas,
    val sceneGlowA: Int = 0,
    val sceneGlowB: Int = 0,
    val sceneGlowC: Int = 0,
    val sceneDark: Boolean = false,
    val liquid: LiquidPreset? = null,
    val material: MaterialKind = if (usesLiquidGlass) MaterialKind.LIQUID_GLASS else MaterialKind.CERAMIC,
)

/**
 * Lightweight material layer used inside the touch surface. It is deliberately
 * a software layer: the input view stays the only touch target, while this
 * view supplies the visual identity for non-glass themes and the water family.
 */
private class MaterialSurfaceView(context: android.content.Context) : View(context) {
    var palette: ThemePalette? = null
        set(value) {
            field = value
            invalidate()
        }

    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val density = resources.displayMetrics.density
    private var phase = 0f
    private var animate = false
    var visualEffectsEnabled: Boolean = true
        set(value) {
            field = value
            if (!value) {
                animate = false
                removeCallbacks(frame)
            }
            invalidate()
        }
    private var rippleX = -1f
    private var rippleY = -1f
    private var rippleStartedAt = 0L
    private val frame = object : Runnable {
        override fun run() {
            if (!animate || !isAttachedToWindow) return
            phase = (phase + 0.012f) % 1f
            if (rippleStartedAt > 0L &&
                android.os.SystemClock.uptimeMillis() - rippleStartedAt > 760L
            ) {
                animate = false
                invalidate()
                return
            }
            invalidate()
            postDelayed(this, 50L)
        }
    }

    init {
        isClickable = false
        isFocusable = false
        setWillNotDraw(false)
    }

    fun startMaterialMotion(enabled: Boolean) {
        animate = enabled && visualEffectsEnabled
        removeCallbacks(frame)
        if (animate) post(frame)
    }

    fun pulse(x: Float, y: Float) {
        if (!visualEffectsEnabled) return
        val material = palette?.material ?: return
        if (material !in setOf(
                MaterialKind.RIPPLE_WATER,
                MaterialKind.LIQUID_METAL,
                MaterialKind.HOLOGRAPHIC,
                MaterialKind.PRISM_CRYSTAL,
                MaterialKind.GEL_SURFACE,
            )
        ) return
        rippleX = x
        rippleY = y
        rippleStartedAt = android.os.SystemClock.uptimeMillis()
        phase = 0f
        startMaterialMotion(true)
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        if (animate) post(frame)
    }

    override fun onDetachedFromWindow() {
        removeCallbacks(frame)
        super.onDetachedFromWindow()
    }

    override fun onDraw(canvas: android.graphics.Canvas) {
        val scene = palette ?: return
        val w = width.toFloat()
        val h = height.toFloat()
        // API 31+ GpuGlassView (and the QWEA0 fallback) owns the optical
        // surface. An opaque child fill here would paint over that lens and
        // leave only a static gradient visible while the pad is idle.
        if (scene.material == MaterialKind.LIQUID_GLASS) return
        paint.shader = null
        paint.style = Paint.Style.FILL
        canvas.drawColor(scene.pad)
        when (scene.material) {
            MaterialKind.LIQUID_METAL -> drawLiquidMetal(canvas, scene, w, h)
            MaterialKind.PAPER_TEXTURE -> drawPaper(canvas, scene, w, h)
            MaterialKind.HOLOGRAPHIC -> drawHolographic(canvas, scene, w, h)
            MaterialKind.RETRO_LCD -> drawLcd(canvas, scene, w, h)
            MaterialKind.CERAMIC -> drawCeramic(canvas, scene, w, h)
            MaterialKind.DROPLET_GLASS -> drawDroplets(canvas, scene, w, h, 15, true)
            MaterialKind.RAIN_GLASS -> drawDroplets(canvas, scene, w, h, 10, false)
            MaterialKind.RIPPLE_WATER -> drawWater(canvas, scene, w, h)
            MaterialKind.PRISM_CRYSTAL -> drawPrism(canvas, scene, w, h)
            MaterialKind.GEL_SURFACE -> drawGel(canvas, scene, w, h)
            MaterialKind.LIQUID_GLASS -> drawQuietGlass(canvas, scene, w, h)
        }
        paint.shader = null
    }

    private fun drawQuietGlass(canvas: android.graphics.Canvas, scene: ThemePalette, w: Float, h: Float) {
        // Keep the resting surface continuous. Discrete circles and cards
        // compete with the input area and make the material look illustrated
        // instead of like one sheet of glass.
        paint.shader = LinearGradient(
            0f, 0f, w, h,
            intArrayOf(withAlpha(scene.sceneStart, 0xB0), withAlpha(scene.sceneMid, 0x9C), withAlpha(scene.sceneEnd, 0xB0)),
            floatArrayOf(0f, .48f, 1f),
            Shader.TileMode.CLAMP,
        )
        canvas.drawRect(0f, 0f, w, h, paint)
        paint.shader = LinearGradient(
            w * .02f, h * .92f, w * .98f, h * .10f,
            intArrayOf(0x00000000, withAlpha(scene.sceneGlowA, 0x42), 0x18FFFFFF, withAlpha(scene.sceneGlowB, 0x38)),
            floatArrayOf(0f, .32f, .66f, 1f),
            Shader.TileMode.CLAMP,
        )
        canvas.drawRect(0f, 0f, w, h, paint)
        paint.shader = LinearGradient(
            0f, 0f, w, h,
            intArrayOf(0x42FFFFFF, 0x00000000, 0x22FFFFFF),
            floatArrayOf(0f, .42f, 1f),
            Shader.TileMode.CLAMP,
        )
        canvas.drawRect(0f, 0f, w, h, paint)
        paint.shader = null
    }

    private fun withAlpha(color: Int, alpha: Int): Int = Color.argb(
        alpha.coerceIn(0, 255), Color.red(color), Color.green(color), Color.blue(color),
    )

    private fun drawDroplets(canvas: android.graphics.Canvas, scene: ThemePalette, w: Float, h: Float, count: Int, shine: Boolean) {
        val seed = intArrayOf(11, 47, 83, 19, 61, 97, 31, 73, 5, 53, 89, 23, 67, 41, 101)
        for (i in 0 until count) {
            val x = w * (.08f + (seed[i] % 87) / 100f)
            val y = h * (.10f + ((seed[i] * 7) % 78) / 100f)
            val radius = dp(16 + seed[i] % 26).toFloat()
            paint.shader = RadialGradient(x - radius * .28f, y - radius * .32f, radius * 1.18f,
                intArrayOf(0xB8FFFFFF.toInt(), 0x4EFFFFFF, 0x1C8EDFD2, 0x081D6B7A), floatArrayOf(0f, .20f, .68f, 1f), Shader.TileMode.CLAMP)
            canvas.drawCircle(x, y, radius, paint)
            paint.shader = null
            paint.style = Paint.Style.STROKE
            paint.strokeWidth = dp(1.2f)
            paint.color = if (shine) 0xA8FFFFFF.toInt() else 0x6BA1D7E0
            canvas.drawCircle(x, y, radius * .86f, paint)
            paint.style = Paint.Style.FILL
            paint.color = 0xCFFFFFFF.toInt()
            canvas.drawCircle(x - radius * .30f, y - radius * .34f, radius * .12f, paint)
        }
    }

    private fun drawWater(canvas: android.graphics.Canvas, scene: ThemePalette, w: Float, h: Float) {
        paint.shader = LinearGradient(0f, 0f, w, h, intArrayOf(0x3C6ADDEB, 0x0896D5FF, 0x3047A7B8), null, Shader.TileMode.CLAMP)
        canvas.drawRect(0f, 0f, w, h, paint)
        paint.shader = null
        val age = if (rippleStartedAt == 0L) 1f else ((android.os.SystemClock.uptimeMillis() - rippleStartedAt) / 760f).coerceIn(0f, 1f)
        val cx = if (rippleX >= 0f) rippleX else w * .50f
        val cy = if (rippleY >= 0f) rippleY else h * .45f
        paint.style = Paint.Style.STROKE
        for (i in 0..3) {
            val r = dp(24f + i * 24f) + age * minOf(w, h) * (.18f + i * .045f)
            paint.strokeWidth = dp(1f + (3 - i) * .35f)
            paint.color = Color.argb(((86 - i * 15) * (1f - age * .72f)).toInt().coerceAtLeast(8), 110, 226, 239)
            canvas.drawOval(android.graphics.RectF(cx - r * 1.45f, cy - r * .55f, cx + r * 1.45f, cy + r * .55f), paint)
        }
        paint.style = Paint.Style.FILL
        paint.shader = RadialGradient(w * .72f, h * .24f, w * .32f, intArrayOf(0x4CFFFFFF, 0x00000000), null, Shader.TileMode.CLAMP)
        canvas.drawCircle(w * .72f, h * .24f, w * .32f, paint)
    }

    private fun drawPrism(canvas: android.graphics.Canvas, scene: ThemePalette, w: Float, h: Float) {
        paint.shader = LinearGradient(0f, h, w, 0f, intArrayOf(0x583DDBFF, 0x4CFB7CEB, 0x463B9DFF, 0x3CC6FFBD), null, Shader.TileMode.MIRROR)
        canvas.drawRect(0f, 0f, w, h, paint)
        paint.shader = null
        paint.style = Paint.Style.STROKE
        paint.strokeWidth = dp(2f)
        paint.color = 0x66FFFFFF
        for (i in 0..3) {
            val inset = dp(24f + i * 18f)
            canvas.drawRoundRect(android.graphics.RectF(inset, inset, w - inset, h - inset), dp(26f), dp(26f), paint)
        }
        paint.style = Paint.Style.FILL
    }

    private fun drawGel(canvas: android.graphics.Canvas, scene: ThemePalette, w: Float, h: Float) {
        paint.shader = RadialGradient(w * .38f, h * .30f, w * .62f, intArrayOf(0x66FFFFFF, 0x1CFFB58A, 0x081A6A72), null, Shader.TileMode.CLAMP)
        canvas.drawRect(0f, 0f, w, h, paint)
        paint.shader = null
        paint.color = 0x26FFFFFF
        canvas.drawOval(android.graphics.RectF(w * .16f, h * .17f, w * .86f, h * .85f), paint)
        paint.style = Paint.Style.STROKE
        paint.strokeWidth = dp(2f)
        paint.color = 0x72FFFFFF
        canvas.drawOval(android.graphics.RectF(w * .18f, h * .18f, w * .84f, h * .82f), paint)
        paint.style = Paint.Style.FILL
    }

    private fun drawLiquidMetal(canvas: android.graphics.Canvas, scene: ThemePalette, w: Float, h: Float) {
        val shift = (phase - .5f) * w * .35f
        paint.shader = LinearGradient(shift, 0f, shift + w * .72f, h,
            intArrayOf(0xFF10171D.toInt(), 0xFF7A8C94.toInt(), 0xFFE4F1F3.toInt(), 0xFF3A4B54.toInt(), 0xFF11181E.toInt()),
            floatArrayOf(0f, .27f, .47f, .62f, 1f), Shader.TileMode.MIRROR)
        canvas.drawRect(0f, 0f, w, h, paint)
        paint.shader = null
        paint.color = 0x38FFFFFF
        canvas.drawOval(android.graphics.RectF(w * .08f, h * .08f, w * .64f, h * .44f), paint)
        paint.color = 0x3A000000
        canvas.drawOval(android.graphics.RectF(w * .40f, h * .56f, w * 1.05f, h * 1.08f), paint)
    }

    private fun drawPaper(canvas: android.graphics.Canvas, scene: ThemePalette, w: Float, h: Float) {
        paint.shader = LinearGradient(0f, 0f, 0f, h, intArrayOf(0xFFFDF8EF.toInt(), 0xFFEDE0CC.toInt()), null, Shader.TileMode.CLAMP)
        canvas.drawRect(0f, 0f, w, h, paint)
        paint.shader = null
        paint.strokeWidth = dp(.7f)
        for (i in 0..170) {
            val x = ((i * 73) % 997) / 997f * w
            val y = ((i * 47) % 991) / 991f * h
            paint.color = if (i % 3 == 0) 0x1F8D694B else 0x171F1711
            canvas.drawPoint(x, y, paint)
        }
        paint.color = 0x2DFFFFFF
        paint.strokeWidth = dp(2f)
        canvas.drawLine(w * .08f, h * .18f, w * .46f, h * .14f, paint)
    }

    private fun drawHolographic(canvas: android.graphics.Canvas, scene: ThemePalette, w: Float, h: Float) {
        val shift = phase * w * 1.4f - w * .4f
        paint.shader = LinearGradient(shift, 0f, shift + w * .65f, h, intArrayOf(0xFF4B3C99.toInt(), 0xFF1EAFB1.toInt(), 0xFFFF6DB4.toInt(), 0xFFEDC15F.toInt(), 0xFF4B3C99.toInt()), null, Shader.TileMode.MIRROR)
        canvas.drawRect(0f, 0f, w, h, paint)
        paint.shader = null
        paint.color = 0x3EFFFFFF
        for (i in 0..10) {
            val x = ((i * 0.13f + phase * .26f) % 1.2f - .1f) * w
            canvas.save()
            canvas.rotate(-18f, x, h * .5f)
            canvas.drawRect(x, 0f, x + dp(4f), h, paint)
            canvas.restore()
        }
    }

    private fun drawLcd(canvas: android.graphics.Canvas, scene: ThemePalette, w: Float, h: Float) {
        paint.color = 0xFF06150D.toInt()
        canvas.drawRect(0f, 0f, w, h, paint)
        paint.color = 0x2136BC75
        paint.strokeWidth = dp(1f)
        var y = dp(5f)
        while (y < h) {
            canvas.drawLine(0f, y, w, y, paint)
            y += dp(5f)
        }
        paint.color = 0x1834A766
        var x = dp(5f)
        while (x < w) {
            canvas.drawLine(x, 0f, x, h, paint)
            x += dp(5f)
        }
        paint.shader = RadialGradient(w * .5f, h * .25f, w * .6f, intArrayOf(0x3256FF9D, 0x00000000), null, Shader.TileMode.CLAMP)
        canvas.drawRect(0f, 0f, w, h, paint)
    }

    private fun drawCeramic(canvas: android.graphics.Canvas, scene: ThemePalette, w: Float, h: Float) {
        // Ceramic is the opaque fallback for classic/editor themes. Use the
        // palette itself instead of a white baked-in gradient, otherwise dark
        // editor themes (Tokyo/Nord/Dracula/etc.) render as pale ceramic.
        val top = if (scene.sceneDark) scene.pad else Color.WHITE
        val bottom = if (scene.sceneDark) scene.canvas else scene.pad
        paint.shader = LinearGradient(0f, 0f, 0f, h, intArrayOf(top, bottom), null, Shader.TileMode.CLAMP)
        canvas.drawRect(0f, 0f, w, h, paint)
        paint.shader = null
        // Editor and classic themes use a quiet, theme-tinted grid instead of
        // the old ellipse highlights/shadows so the touch plane reads as a
        // surface, not a decorative shadow card.
        paint.color = withAlpha(scene.accent, if (scene.sceneDark) 0x16 else 0x12)
        paint.strokeWidth = dp(1f)
        var x = dp(36f)
        while (x < w) {
            canvas.drawLine(x, 0f, x, h, paint)
            x += dp(72f)
        }
        var y = dp(36f)
        while (y < h) {
            canvas.drawLine(0f, y, w, y, paint)
            y += dp(72f)
        }
    }

    private fun dp(value: Float): Float = value * density
    private fun dp(value: Int): Float = value * density
}

private fun paletteFor(mode: ThemeMode): ThemePalette = when (mode) {
    ThemeMode.LIGHT_GLASS -> ThemePalette(
        canvas = 0xFFF4F5F7.toInt(), chrome = 0xE8FFFFFF.toInt(), chromeStroke = 0x66FFFFFF,
        button = 0xBFFFFFFF.toInt(), buttonStroke = 0x3D8C929B, input = 0xF0FFFFFF.toInt(),
        pad = 0xFFFFFFFF.toInt(), padStroke = 0xFFDDE2EA.toInt(), label = 0xFF1D1D1F.toInt(),
        secondary = 0xFF6E6E73.toInt(), accent = 0xFF007AFF.toInt(), success = 0xFF1E9E5A.toInt(),
        warning = 0xFFB86A00.toInt(), danger = 0xFFC9342F.toInt(), deep = 0xFFE2ECFA.toInt(),
        deepProgress = 0xFF007AFF.toInt(), deepStroke = 0xFF6D9FE8.toInt(), deepText = 0xFF12345C.toInt(),
        usesLiquidGlass = true, sceneStart = 0xFFE7F7FF.toInt(), sceneMid = 0xFFF7FBFF.toInt(), sceneEnd = 0xFFFFE8D7.toInt(),
        sceneGlowA = 0xA04A9FFF.toInt(), sceneGlowB = 0xA0FF9A61.toInt(), sceneGlowC = 0x607BC7D8, liquid = LIQUID_DEFAULT,
    )
    ThemeMode.DARK_GLASS -> ThemePalette(
        canvas = 0xFF0B0D12.toInt(), chrome = 0xC51B1F29.toInt(), chromeStroke = 0x35FFFFFF,
        button = 0xB5222833.toInt(), buttonStroke = 0x2CFFFFFF, input = 0xFF252A34.toInt(),
        pad = 0xFF151923.toInt(), padStroke = 0x3DFFFFFF, label = 0xFFF5F7FB.toInt(),
        secondary = 0xFFAEB6C5.toInt(), accent = 0xFF0A84FF.toInt(), success = 0xFF30D158.toInt(),
        warning = 0xFFFF9F0A.toInt(), danger = 0xFFFF453A.toInt(), deep = 0xFF24242E.toInt(),
        deepProgress = 0xFF0A84FF.toInt(), deepStroke = 0x66FFFFFF, deepText = 0xFFFFFFFF.toInt(),
        usesLiquidGlass = true, sceneStart = 0xFF2A5774.toInt(), sceneMid = 0xFF101821.toInt(), sceneEnd = 0xFF243B59.toInt(),
        sceneGlowA = 0xB04FB6E8.toInt(), sceneGlowB = 0xA86D7CFF.toInt(), sceneGlowC = 0x625B8CFF, sceneDark = true,
        liquid = LIQUID_DEFAULT.copy(bevelWidth = 48f, refractionHeight = 260f, dispersionStrength = .24f, saturation = 155f, highlightOpacity = 92f, adaptiveTint = true),
    )
    ThemeMode.OCEAN_GLASS -> paletteFor(ThemeMode.LIGHT_GLASS).copy(
        canvas = 0xFFEAF7FF.toInt(), chrome = 0xE6F4FCFF.toInt(), chromeStroke = 0x668ACFEF,
        button = 0xBFE8FAFF.toInt(), buttonStroke = 0x3D54B7D9, input = 0xF0F4FCFF.toInt(), pad = 0xFFF4FCFF.toInt(),
        padStroke = 0xFFB5E0F2.toInt(), accent = 0xFF007AFF.toInt(), deep = 0xFFD9F3FF.toInt(), deepProgress = 0xFF007AFF.toInt(),
        deepStroke = 0xFF4FA9D6.toInt(), deepText = 0xFF073B5A.toInt(), sceneStart = 0xFFD9F4FF.toInt(), sceneMid = 0xFFEFFBFF.toInt(), sceneEnd = 0xFFD8FFF5.toInt(),
        sceneGlowA = 0xA0009DFF.toInt(), sceneGlowB = 0xA000E1CA.toInt(), sceneGlowC = 0x6085BEFF, liquid = LIQUID_DEFAULT.copy(bevelWidth = 48f, refractionHeight = 270f, dispersionStrength = .28f, saturation = 164f, highlightOpacity = 96f),
    )
    ThemeMode.SUNSET_GLASS -> paletteFor(ThemeMode.LIGHT_GLASS).copy(
        canvas = 0xFFFFF2EB.toInt(), chrome = 0xE6FFF9F5.toInt(), chromeStroke = 0x66F2B79C,
        button = 0xBFFFF5ED.toInt(), buttonStroke = 0x3DDC9D7B, input = 0xF0FFF9F5.toInt(), pad = 0xFFFFF8F3.toInt(),
        padStroke = 0xFFF1C3A8.toInt(), accent = 0xFFFF6B35.toInt(), deep = 0xFFFFE6D8.toInt(), deepProgress = 0xFFFF6B35.toInt(),
        deepStroke = 0xFFE29A75.toInt(), deepText = 0xFF63230F.toInt(), sceneStart = 0xFFFFE0D7.toInt(), sceneMid = 0xFFFFF7ED.toInt(), sceneEnd = 0xFFFFE5B8.toInt(),
        sceneGlowA = 0xA0FF766B.toInt(), sceneGlowB = 0xA0FFB74C.toInt(), sceneGlowC = 0x60FFD592, liquid = LIQUID_DEFAULT.copy(bevelWidth = 44f, refractionHeight = 220f, dispersionStrength = .24f, saturation = 160f, highlightOpacity = 90f),
    )
    ThemeMode.AURORA_GLASS -> paletteFor(ThemeMode.LIGHT_GLASS).copy(
        canvas = 0xFFEEF9F8.toInt(), chrome = 0xE6F2FDFC.toInt(), chromeStroke = 0x6689D9C8,
        button = 0xBFF0FFFA.toInt(), buttonStroke = 0x3D75CDBA, input = 0xF0F2FDFC.toInt(), pad = 0xFFF4FFFC.toInt(),
        padStroke = 0xFFB1E5D8.toInt(), accent = 0xFF00A98F.toInt(), deep = 0xFFDDFBF3.toInt(), deepProgress = 0xFF00A98F.toInt(),
        deepStroke = 0xFF56BCA9.toInt(), deepText = 0xFF06483D.toInt(), sceneStart = 0xFFDDF5FF.toInt(), sceneMid = 0xFFF2FFF9.toInt(), sceneEnd = 0xFFE9DFFF.toInt(),
        sceneGlowA = 0xA048B2FF.toInt(), sceneGlowB = 0xA000CAA4.toInt(), sceneGlowC = 0x60B87AFF, liquid = LIQUID_DEFAULT.copy(bevelWidth = 52f, refractionHeight = 300f, dispersionStrength = .32f, saturation = 170f, highlightOpacity = 102f),
    )
    ThemeMode.GRAPHITE_GLASS -> paletteFor(ThemeMode.DARK_GLASS).copy(
        canvas = 0xFF0F1218.toInt(), chrome = 0xD91C222E.toInt(), chromeStroke = 0x42FFFFFF, button = 0xB5222C3A.toInt(),
        buttonStroke = 0x35FFFFFF, input = 0xFF202936.toInt(), pad = 0xFF151B24.toInt(), padStroke = 0x4DFFFFFF, label = 0xFFF4F7FB.toInt(),
        secondary = 0xFFB3BDCB.toInt(), accent = 0xFF64D2FF.toInt(), deep = 0xFF202B38.toInt(), deepProgress = 0xFF64D2FF.toInt(), deepStroke = 0x6689DFFF,
        deepText = 0xFFE7F9FF.toInt(), sceneStart = 0xFF2C77B4.toInt(), sceneMid = 0xFF111923.toInt(), sceneEnd = 0xFFAF4758.toInt(), sceneGlowA = 0xA02C77B4.toInt(),
        sceneGlowB = 0xA0AF4758.toInt(), sceneGlowC = 0x6243AEA7, sceneDark = true, liquid = LIQUID_DEFAULT.copy(bevelWidth = 50f, refractionHeight = 250f, dispersionStrength = .30f, blurAmount = .045f, saturation = 148f, highlightOpacity = 94f, adaptiveTint = true),
    )
    ThemeMode.CUSTOM_GLASS -> paletteFor(ThemeMode.LIGHT_GLASS).copy(
        canvas = 0xFF171A22.toInt(), chrome = 0xD91D2330.toInt(), chromeStroke = 0x50FFFFFF,
        button = 0xB52B3342.toInt(), buttonStroke = 0x3FFFFFFF, input = 0xFF252C39.toInt(),
        pad = 0xFF151B25.toInt(), padStroke = 0x66FFFFFF, label = 0xFFF7F8FC.toInt(),
        secondary = 0xFFB7C0D1.toInt(), accent = 0xFF8ED8FF.toInt(), deep = 0xFF243448.toInt(),
        deepProgress = 0xFF8ED8FF.toInt(), deepStroke = 0x889EDBFF.toInt(), deepText = 0xFFEAF8FF.toInt(),
        sceneStart = 0xFF243E58.toInt(), sceneMid = 0xFF141A26.toInt(), sceneEnd = 0xFF3E2943.toInt(),
        sceneGlowA = 0xA04A9FFF.toInt(), sceneGlowB = 0xA0FF6E9D.toInt(), sceneGlowC = 0x625CC6FF,
        sceneDark = true, liquid = LIQUID_DEFAULT.copy(
            bevelWidth = 58f, refractionHeight = 340f, dispersionStrength = .34f,
            blurAmount = .025f, saturation = 170f, highlightOpacity = 108f, adaptiveTint = true,
        ),
    )
    ThemeMode.DROPLET_GLASS -> paletteFor(ThemeMode.LIGHT_GLASS).copy(
        canvas = 0xFFEAF6F5.toInt(), chrome = 0xDDF5FFFF.toInt(), chromeStroke = 0x6686BFC0,
        button = 0xBFEAFAF8.toInt(), buttonStroke = 0x3D5AABA8, input = 0xF0F7FFFE.toInt(), pad = 0xFFEFFFFD.toInt(),
        padStroke = 0xFFB1D9D4.toInt(), accent = 0xFF168C87.toInt(), deep = 0xFFD8F3EF.toInt(), deepProgress = 0xFF168C87.toInt(),
        deepStroke = 0xFF56AAA2.toInt(), deepText = 0xFF0A4846.toInt(), sceneStart = 0xFFCFEDE8.toInt(), sceneMid = 0xFFF7FFFE.toInt(), sceneEnd = 0xFFD9EFFF.toInt(),
        sceneGlowA = 0xA067D7C8.toInt(), sceneGlowB = 0xA06AAFEA.toInt(), sceneGlowC = 0x608EE7DA, material = MaterialKind.DROPLET_GLASS,
        liquid = LIQUID_DEFAULT.copy(bevelWidth = 56f, refractionHeight = 300f, dispersionStrength = .22f, blurAmount = .04f, saturation = 156f, highlightOpacity = 108f),
    )
    ThemeMode.RIPPLE_WATER -> paletteFor(ThemeMode.OCEAN_GLASS).copy(
        canvas = 0xFFE8F4F7.toInt(), chrome = 0xDDEEF9FA.toInt(), accent = 0xFF007C91.toInt(), deep = 0xFFD5F0F4.toInt(),
        deepProgress = 0xFF007C91.toInt(), deepStroke = 0xFF4DA6B4.toInt(), deepText = 0xFF0A3E48.toInt(),
        sceneStart = 0xFFC8ECF5.toInt(), sceneMid = 0xFFF4FCFC.toInt(), sceneEnd = 0xFFCDE8F4.toInt(), sceneGlowA = 0xA03FC1D2.toInt(),
        sceneGlowB = 0xA04E9EEA.toInt(), sceneGlowC = 0x6079D9DD, material = MaterialKind.RIPPLE_WATER,
        liquid = LIQUID_DEFAULT.copy(bevelWidth = 48f, refractionHeight = 280f, dispersionStrength = .16f, blurAmount = .05f, saturation = 150f, highlightOpacity = 98f),
    )
    ThemeMode.RAIN_GLASS -> paletteFor(ThemeMode.DARK_GLASS).copy(
        canvas = 0xFF111B24.toInt(), chrome = 0xD91C2C38.toInt(), chromeStroke = 0x4594C4D6, button = 0xB5263A48.toInt(),
        buttonStroke = 0x3D7EACBF, input = 0xFF22313D.toInt(), pad = 0xFF13232D.toInt(), padStroke = 0x668DB8C4,
        label = 0xFFEAF8FA.toInt(), secondary = 0xFFA8C9D0.toInt(), accent = 0xFF64D2FF.toInt(), deep = 0xFF213D48.toInt(),
        deepProgress = 0xFF64D2FF.toInt(), deepStroke = 0x669DDDEB, deepText = 0xFFEAF8FA.toInt(), sceneStart = 0xFF1E5069.toInt(),
        sceneMid = 0xFF0B151E.toInt(), sceneEnd = 0xFF243D59.toInt(), sceneGlowA = 0xA030A6C5.toInt(), sceneGlowB = 0xA04D7198.toInt(),
        sceneGlowC = 0x625BB6C5, sceneDark = true, material = MaterialKind.RAIN_GLASS,
        liquid = LIQUID_DEFAULT.copy(bevelWidth = 50f, refractionHeight = 270f, dispersionStrength = .20f, blurAmount = .045f, saturation = 145f, highlightOpacity = 98f, adaptiveTint = true),
    )
    ThemeMode.PRISM_CRYSTAL -> paletteFor(ThemeMode.AURORA_GLASS).copy(
        canvas = 0xFFF3F0FC.toInt(), chrome = 0xDDF8F5FF.toInt(), chromeStroke = 0x668E7DC3, button = 0xBFF4F0FF.toInt(),
        buttonStroke = 0x3D957BCB, input = 0xF0FBF9FF.toInt(), pad = 0xFFF8F5FF.toInt(), padStroke = 0xFFC7B4EA.toInt(),
        label = 0xFF262338.toInt(), secondary = 0xFF6F6885.toInt(), accent = 0xFF7A50D6.toInt(), deep = 0xFFE9DEFF.toInt(),
        deepProgress = 0xFF7A50D6.toInt(), deepStroke = 0xFF9C7BE3.toInt(), deepText = 0xFF36215F.toInt(), sceneStart = 0xFFE4D9FF.toInt(),
        sceneMid = 0xFFFFF8FC.toInt(), sceneEnd = 0xFFD9F8F1.toInt(), sceneGlowA = 0xA0FF7AD9.toInt(), sceneGlowB = 0xA06F9AFF.toInt(),
        sceneGlowC = 0x604BD8C4, material = MaterialKind.PRISM_CRYSTAL,
        liquid = LIQUID_DEFAULT.copy(bevelWidth = 58f, refractionHeight = 340f, dispersionStrength = .40f, blurAmount = .035f, saturation = 176f, highlightOpacity = 118f),
    )
    ThemeMode.GEL_SURFACE -> paletteFor(ThemeMode.SUNSET_GLASS).copy(
        canvas = 0xFFFFF5F0.toInt(), chrome = 0xDDFEF6F3.toInt(), chromeStroke = 0x66E7AA92, button = 0xBFFFF3EC.toInt(),
        buttonStroke = 0x3DCF9073, input = 0xF0FFF9F5.toInt(), pad = 0xFFFFF8F3.toInt(), padStroke = 0xFFF0B999.toInt(),
        accent = 0xFFE75F36.toInt(), deep = 0xFFFFE5D9.toInt(), deepProgress = 0xFFE75F36.toInt(), deepStroke = 0xFFE39A79.toInt(),
        deepText = 0xFF642710.toInt(), sceneStart = 0xFFFFD8C7.toInt(), sceneMid = 0xFFFFF8EF.toInt(), sceneEnd = 0xFFFFE6B8.toInt(),
        sceneGlowA = 0xA0FF8B77.toInt(), sceneGlowB = 0xA0FFBD67.toInt(), sceneGlowC = 0x60F7A2D6, material = MaterialKind.GEL_SURFACE,
        liquid = LIQUID_DEFAULT.copy(bevelWidth = 62f, refractionHeight = 210f, dispersionStrength = .12f, blurAmount = .07f, saturation = 162f, highlightOpacity = 104f),
    )
    ThemeMode.LIQUID_METAL -> paletteFor(ThemeMode.CLASSIC_DARK).copy(
        canvas = 0xFF0D1015.toInt(), chrome = 0xFF1E252D.toInt(), chromeStroke = 0xFF4A5964.toInt(), button = 0xFF27313A.toInt(),
        buttonStroke = 0xFF556773.toInt(), input = 0xFF222B33.toInt(), pad = 0xFF12181F.toInt(), padStroke = 0xFF6B7C86.toInt(),
        label = 0xFFF2F6F8.toInt(), secondary = 0xFFA8B6BE.toInt(), accent = 0xFFB9D4DE.toInt(), deep = 0xFF2E3A43.toInt(),
        deepProgress = 0xFFB9D4DE.toInt(), deepStroke = 0xFF7D969F.toInt(), deepText = 0xFF10161B.toInt(), sceneStart = 0xFF40505A.toInt(),
        sceneMid = 0xFF11171D.toInt(), sceneEnd = 0xFF2A3943.toInt(), sceneGlowA = 0xA0B6C9D0.toInt(), sceneGlowB = 0xA05B6E79.toInt(),
        sceneGlowC = 0x605F737F, sceneDark = true, usesLiquidGlass = false, liquid = null, material = MaterialKind.LIQUID_METAL,
    )
    ThemeMode.PAPER_TEXTURE -> paletteFor(ThemeMode.CLASSIC_LIGHT).copy(
        canvas = 0xFFF3EEE5.toInt(), chrome = 0xFFFDF9F1.toInt(), chromeStroke = 0xFFD9CDBD.toInt(), button = 0xFFF2E8D9.toInt(),
        buttonStroke = 0xFFD4C1AA.toInt(), input = 0xFFFFFBF4.toInt(), pad = 0xFFF9F1E5.toInt(), padStroke = 0xFFD8C7B0.toInt(),
        label = 0xFF302820.toInt(), secondary = 0xFF766759.toInt(), accent = 0xFFB85C38.toInt(), deep = 0xFFEADACA.toInt(),
        deepProgress = 0xFFB85C38.toInt(), deepStroke = 0xFFC68B70.toInt(), deepText = 0xFF4B2415.toInt(), sceneStart = 0xFFE8D9C4.toInt(),
        sceneMid = 0xFFF8F0E2.toInt(), sceneEnd = 0xFFE4D3BD.toInt(), sceneGlowA = 0x687EAD91, sceneGlowB = 0x686CA0BE, sceneGlowC = 0x605D8A6B,
        material = MaterialKind.PAPER_TEXTURE,
    )
    ThemeMode.HOLOGRAPHIC -> paletteFor(ThemeMode.PRISM_CRYSTAL).copy(
        canvas = 0xFF181B2B.toInt(), chrome = 0xE1252A45.toInt(), chromeStroke = 0x667E8EEA, button = 0xB52D3154.toInt(),
        buttonStroke = 0x4D8C9CFF, input = 0xFF242A46.toInt(), pad = 0xFF171B2F.toInt(), padStroke = 0x6693A8FF, label = 0xFFF5F2FF.toInt(),
        secondary = 0xFFB2B8D6.toInt(), accent = 0xFFFF6BD6.toInt(), deep = 0xFF392553.toInt(), deepProgress = 0xFFFF6BD6.toInt(),
        deepStroke = 0xFFB986E7.toInt(), deepText = 0xFFFFF1FB.toInt(), sceneStart = 0xFF273B72.toInt(), sceneMid = 0xFF151A2E.toInt(),
        sceneEnd = 0xFF542C60.toInt(), sceneGlowA = 0xA0FF6BD6.toInt(), sceneGlowB = 0xA06B8CFF.toInt(), sceneGlowC = 0x60A6F5B5,
        sceneDark = true, usesLiquidGlass = false, liquid = null, material = MaterialKind.HOLOGRAPHIC,
    )
    ThemeMode.RETRO_LCD -> paletteFor(ThemeMode.CLASSIC_DARK).copy(
        canvas = 0xFF07100D.toInt(), chrome = 0xFF0D1D17.toInt(), chromeStroke = 0xFF285B47.toInt(), button = 0xFF123025.toInt(),
        buttonStroke = 0xFF347B5E.toInt(), input = 0xFF0E251B.toInt(), pad = 0xFF07170F.toInt(), padStroke = 0xFF347B5E.toInt(),
        label = 0xFFB7FFD7.toInt(), secondary = 0xFF72B493.toInt(), accent = 0xFF65E69A.toInt(), deep = 0xFF143B29.toInt(),
        deepProgress = 0xFF65E69A.toInt(), deepStroke = 0xFF4EAC78.toInt(), deepText = 0xFF07170F.toInt(), sceneStart = 0xFF173B2A.toInt(),
        sceneMid = 0xFF06110B.toInt(), sceneEnd = 0xFF0E2A1C.toInt(), sceneGlowA = 0xA021A764.toInt(), sceneGlowB = 0xA0126D4A.toInt(),
        sceneGlowC = 0x6040A76B, sceneDark = true, usesLiquidGlass = false, liquid = null, material = MaterialKind.RETRO_LCD,
    )
    ThemeMode.CERAMIC -> paletteFor(ThemeMode.CLASSIC_LIGHT).copy(
        canvas = 0xFFE7E8EB.toInt(), chrome = 0xFFF7F8FA.toInt(), chromeStroke = 0xFFC9CDD3.toInt(), button = 0xFFF1F2F4.toInt(),
        buttonStroke = 0xFFC5C9CF.toInt(), input = 0xFFFFFFFF.toInt(), pad = 0xFFF6F6F4.toInt(), padStroke = 0xFFC6C7C3.toInt(),
        label = 0xFF23262B.toInt(), secondary = 0xFF6A7078.toInt(), accent = 0xFF4C6B85.toInt(), deep = 0xFFE0E3E6.toInt(),
        deepProgress = 0xFF4C6B85.toInt(), deepStroke = 0xFF8395A4.toInt(), deepText = 0xFF26303A.toInt(), sceneStart = 0xFFDDE1E5.toInt(),
        sceneMid = 0xFFF5F6F7.toInt(), sceneEnd = 0xFFE1E4E6.toInt(), sceneGlowA = 0x506E8194, sceneGlowB = 0x505A7187, sceneGlowC = 0x405D6873,
        material = MaterialKind.CERAMIC,
    )
    ThemeMode.TOKYO_NIGHT -> paletteFor(ThemeMode.CLASSIC_DARK).copy(canvas = 0xFF16161E.toInt(), chrome = 0xFF1F2335.toInt(), chromeStroke = 0xFF3B4261.toInt(), button = 0xFF292E42.toInt(), buttonStroke = 0xFF3B4261.toInt(), input = 0xFF24283B.toInt(), pad = 0xFF1A1B26.toInt(), padStroke = 0xFF3B4261.toInt(), label = 0xFFC0CAF5.toInt(), secondary = 0xFFA9B1D6.toInt(), accent = 0xFF7AA2F7.toInt(), deep = 0xFF24283B.toInt(), deepProgress = 0xFF7AA2F7.toInt(), deepStroke = 0xFF565F89.toInt(), deepText = 0xFFC0CAF5.toInt())
    ThemeMode.NORD -> paletteFor(ThemeMode.CLASSIC_DARK).copy(canvas = 0xFF2E3440.toInt(), chrome = 0xFF3B4252.toInt(), chromeStroke = 0xFF4C566A.toInt(), button = 0xFF434C5E.toInt(), buttonStroke = 0xFF4C566A.toInt(), input = 0xFF434C5E.toInt(), pad = 0xFF2E3440.toInt(), padStroke = 0xFF4C566A.toInt(), label = 0xFFECEFF4.toInt(), secondary = 0xFFD8DEE9.toInt(), accent = 0xFF88C0D0.toInt(), deep = 0xFF434C5E.toInt(), deepProgress = 0xFF88C0D0.toInt(), deepStroke = 0xFF81A1C1.toInt(), deepText = 0xFF2E3440.toInt())
    ThemeMode.DRACULA -> paletteFor(ThemeMode.CLASSIC_DARK).copy(canvas = 0xFF282A36.toInt(), chrome = 0xFF343746.toInt(), chromeStroke = 0xFF6272A4.toInt(), button = 0xFF44475A.toInt(), buttonStroke = 0xFF6272A4.toInt(), input = 0xFF44475A.toInt(), pad = 0xFF282A36.toInt(), padStroke = 0xFF6272A4.toInt(), label = 0xFFF8F8F2.toInt(), secondary = 0xFFD5D4C8.toInt(), accent = 0xFFBD93F9.toInt(), deep = 0xFF44475A.toInt(), deepProgress = 0xFFBD93F9.toInt(), deepStroke = 0xFF8BE9FD.toInt(), deepText = 0xFF282A36.toInt())
    ThemeMode.SOLARIZED_DARK -> paletteFor(ThemeMode.CLASSIC_DARK).copy(canvas = 0xFF002B36.toInt(), chrome = 0xFF073642.toInt(), chromeStroke = 0xFF586E75.toInt(), button = 0xFF0A4050.toInt(), buttonStroke = 0xFF586E75.toInt(), input = 0xFF0A4050.toInt(), pad = 0xFF002B36.toInt(), padStroke = 0xFF586E75.toInt(), label = 0xFFFDF6E3.toInt(), secondary = 0xFF93A1A1.toInt(), accent = 0xFF2AA198.toInt(), deep = 0xFF0A4050.toInt(), deepProgress = 0xFF2AA198.toInt(), deepStroke = 0xFF2AA198.toInt(), deepText = 0xFFFDF6E3.toInt())
    ThemeMode.CATPPUCCIN_MOCHA -> paletteFor(ThemeMode.CLASSIC_DARK).copy(canvas = 0xFF1E1E2E.toInt(), chrome = 0xFF313244.toInt(), chromeStroke = 0xFF585B70.toInt(), button = 0xFF45475A.toInt(), buttonStroke = 0xFF585B70.toInt(), input = 0xFF45475A.toInt(), pad = 0xFF1E1E2E.toInt(), padStroke = 0xFF585B70.toInt(), label = 0xFFCDD6F4.toInt(), secondary = 0xFFBAC2DE.toInt(), accent = 0xFF89B4FA.toInt(), deep = 0xFF45475A.toInt(), deepProgress = 0xFF89B4FA.toInt(), deepStroke = 0xFF89B4FA.toInt(), deepText = 0xFF1E1E2E.toInt())
    ThemeMode.MONOKAI -> paletteFor(ThemeMode.CLASSIC_DARK).copy(canvas = 0xFF272822.toInt(), chrome = 0xFF3E3D32.toInt(), chromeStroke = 0xFF75715E.toInt(), button = 0xFF49483E.toInt(), buttonStroke = 0xFF75715E.toInt(), input = 0xFF49483E.toInt(), pad = 0xFF272822.toInt(), padStroke = 0xFF75715E.toInt(), label = 0xFFF8F8F2.toInt(), secondary = 0xFFCFCFC2.toInt(), accent = 0xFFA6E22E.toInt(), deep = 0xFF49483E.toInt(), deepProgress = 0xFFA6E22E.toInt(), deepStroke = 0xFFF92672.toInt(), deepText = 0xFF272822.toInt())
    ThemeMode.CLASSIC_LIGHT -> ThemePalette(
        canvas = 0xFFF2F2F7.toInt(), chrome = 0xFFFFFFFF.toInt(), chromeStroke = 0xFFD1D1D6.toInt(),
        button = 0xFFF2F2F7.toInt(), buttonStroke = 0xFFD1D1D6.toInt(), input = 0xFFF2F2F7.toInt(),
        pad = 0xFFFFFFFF.toInt(), padStroke = 0xFFD1D1D6.toInt(), label = 0xFF1D1D1F.toInt(),
        secondary = 0xFF6E6E73.toInt(), accent = 0xFF0071E3.toInt(), success = 0xFF1E9E5A.toInt(),
        warning = 0xFF9A5B00.toInt(), danger = 0xFFC9342F.toInt(), deep = 0xFFE5E5EA.toInt(),
        deepProgress = 0xFF0071E3.toInt(), deepStroke = 0xFF8E8E93.toInt(), deepText = 0xFF1D1D1F.toInt(),
        usesLiquidGlass = false,
    )
    ThemeMode.CLASSIC_DARK -> ThemePalette(
        canvas = 0xFF000000.toInt(), chrome = 0xFF1C1C1E.toInt(), chromeStroke = 0xFF48484A.toInt(),
        button = 0xFF2C2C2E.toInt(), buttonStroke = 0xFF636366.toInt(), input = 0xFF2C2C2E.toInt(),
        pad = 0xFF1C1C1E.toInt(), padStroke = 0xFF48484A.toInt(), label = 0xFFF5F5F7.toInt(),
        secondary = 0xFF98989D.toInt(), accent = 0xFF0A84FF.toInt(), success = 0xFF30D158.toInt(),
        warning = 0xFFFF9F0A.toInt(), danger = 0xFFFF453A.toInt(), deep = 0xFF2C2C2E.toInt(),
        deepProgress = 0xFF0A84FF.toInt(), deepStroke = 0xFF636366.toInt(), deepText = 0xFFF5F5F7.toInt(),
        usesLiquidGlass = false, sceneDark = true,
    )
    ThemeMode.HIGH_CONTRAST -> ThemePalette(
        canvas = Color.WHITE, chrome = Color.WHITE, chromeStroke = Color.BLACK,
        button = Color.WHITE, buttonStroke = Color.BLACK, input = Color.WHITE,
        pad = Color.WHITE, padStroke = Color.BLACK, label = Color.BLACK, secondary = Color.BLACK,
        accent = Color.BLACK, success = Color.BLACK, warning = Color.BLACK, danger = Color.BLACK,
        deep = Color.WHITE, deepProgress = Color.BLACK, deepStroke = Color.BLACK, deepText = Color.BLACK,
        usesLiquidGlass = false,
    )
}

class MainActivity : Activity() {

    private lateinit var prefs: SharedPreferences
    private lateinit var sender: UdpSender
    private lateinit var pad: TouchPadView
    private lateinit var dot: View
    private lateinit var status: TextView
    private lateinit var header: LinearLayout
    private lateinit var controls: LinearLayout
    private lateinit var padFrame: FrameLayout
    private lateinit var padHost: View
    private lateinit var materialSurface: MaterialSurfaceView
    private lateinit var deepPressBar: DeepPressBarView
    private lateinit var fullscreenFloatBtn: Button
    private lateinit var controlsRail: View
    private lateinit var headerInfo: View
    private lateinit var connectButton: Button
    private lateinit var headerToggle: Button
    private var padGlassView: LiquidGlassView? = null
    private var gpuGlassView: GpuGlassView? = null
    private var normalPadFrameBackground: android.graphics.drawable.Drawable? = null
    private lateinit var discovery: MacDiscovery
    private var discoveredEndpoints: List<MacDiscovery.MacEndpoint> = emptyList()
    private var isFullscreenMode = false
    private var isConnected = false
    private var isConnecting = false
    private var connectionAttemptSerial = 0L
    private var headerExpanded = true
    private var modalDepth = 0
    private var wallpaperBitmap: Bitmap? = null

    private val deepButtonHeartbeat = object : Runnable {
        override fun run() {
            if (::deepPressBar.isInitialized && deepPressBar.isDeepPressed) {
                sendVirtualButton(true)
                pad.postDelayed(this, DEEP_BUTTON_HEARTBEAT_MS)
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            window.attributes = window.attributes.apply {
                layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
            }
        }
        prefs = getSharedPreferences("touchpad", MODE_PRIVATE)
        applyPairingIntent(intent)
        wallpaperBitmap = loadWallpaper()
        sender = UdpSender()
        discovery = MacDiscovery(this, object : MacDiscovery.Listener {
            override fun onDiscoveryChanged(endpoints: List<MacDiscovery.MacEndpoint>) {
                runOnUiThread { discoveredEndpoints = endpoints }
            }

            override fun onDiscoveryError(message: String) {
                // Discovery is optional. Keep the manual connection path quiet
                // unless the user explicitly opens the connection sheet.
                runOnUiThread { discoveredEndpoints = discovery.snapshot() }
            }
        })
        discovery.start()
        val palette = themePalette()

        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        window.statusBarColor = Color.TRANSPARENT
        window.navigationBarColor = Color.TRANSPARENT
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            // Transparent bars must not receive the platform's opaque contrast
            // scrim; the app owns the full-window background in immersive mode.
            window.isNavigationBarContrastEnforced = false
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            window.navigationBarDividerColor = Color.TRANSPARENT
        }

        pad = TouchPadView(this).also {
            it.sender = sender
            it.scale = prefs.getFloat(KEY_SCALE, 1f)
            it.haptics.deepPressStrength = prefs.getInt(KEY_DEEP_HAPTIC_STRENGTH, DEFAULT_DEEP_HAPTIC_STRENGTH)
        }

        val initialPalette = palette
        val touchPointsVisible = prefs.getBoolean(
            KEY_TOUCH_POINTS,
            prefs.getBoolean(KEY_VISUAL_EFFECTS, true),
        )
        pad.visualEffectsEnabled = touchPointsVisible
        materialSurface = MaterialSurfaceView(this).apply {
            this.palette = initialPalette
            // Material motion is independent from the touch-point overlay.
            this.visualEffectsEnabled = true
            // The QWEA0 lens already supplies the default Liquid Glass optical
            // layer. Keeping a solid software fill underneath it would flatten
            // the lens to white, so only the alternate material families use
            // this backdrop view.
            visibility = if (initialPalette.material == MaterialKind.LIQUID_GLASS) View.GONE else View.VISIBLE
            // Resting surfaces are static. Only the water theme starts a
            // short-lived animation from an actual touch via pulse().
            startMaterialMotion(false)
            contentDescription = "触控面材质"
        }
        pad.onTouchPulse = { x, y ->
            if (::materialSurface.isInitialized) materialSurface.pulse(x, y)
        }
        pad.onTouchStateChanged = { active ->
            padGlassView?.enableDynamicBackground = active
            gpuGlassView?.setInteraction(active)
        }
        pad.onTouchPositionChanged = { x, y ->
            gpuGlassView?.setInteraction(true, x, y)
        }
        padFrame = FrameLayout(this)
        padFrame.addView(
            materialSurface,
            FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT,
            ),
        )
        padFrame.addView(
            pad,
            FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT,
            ),
        )
        deepPressBar = DeepPressBarView(this).also { bar ->
            bar.haptics = pad.haptics
            bar.applyTheme(palette.deep, palette.deepProgress, palette.deepStroke, palette.deepText)
            bar.holdDurationMs = prefs.getLong(KEY_DEEP_HOLD_MS, DEFAULT_DEEP_HOLD_MS)
            bar.onDeepPress = { down ->
                sendVirtualButton(down)
                if (down) {
                    pad.removeCallbacks(deepButtonHeartbeat)
                    pad.post(deepButtonHeartbeat)
                } else {
                    pad.removeCallbacks(deepButtonHeartbeat)
                }
            }
            bar.visibility = if (prefs.getBoolean(KEY_DEEP_ENABLED, true)) View.VISIBLE else View.GONE
        }
        padFrame.addView(deepPressBar)
        padFrame.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> layoutDeepPressBar() }

        fun surface(color: Int, radius: Int = 16, stroke: Int = palette.buttonStroke): GradientDrawable = GradientDrawable().apply {
            setColor(color)
            cornerRadius = dp(radius).toFloat()
            if (stroke != 0) setStroke(dp(1), stroke)
        }

        fun actionButton(label: String, accent: Boolean = false, onClick: () -> Unit): Button = Button(this).apply {
            text = label
            isAllCaps = false
            val accentLuma = (Color.red(palette.accent) * .2126f + Color.green(palette.accent) * .7152f + Color.blue(palette.accent) * .0722f) / 255f
            setTextColor(if (accent && accentLuma < .62f) Color.WHITE else palette.label)
            textSize = 12f
            minHeight = dp(40)
            minimumHeight = dp(40)
            minWidth = dp(44)
            // Keep the native rail compact enough for a landscape phone while
            // preserving a 44dp hit target.
            setPadding(dp(10), 0, dp(10), 0)
            background = surface(if (accent) palette.accent else palette.button, InteractionMetrics.BUTTON_RADIUS_DP, if (accent) 0x660A84FF else palette.buttonStroke)
            stateListAnimator = null
            elevation = dp(1).toFloat()
            clipToOutline = true
            setOnClickListener { onClick() }
            installPressFeedback(this)
        }

        dot = View(this).apply {
            val d = GradientDrawable().apply {
                shape = GradientDrawable.OVAL
                setColor(palette.secondary)
            }
            background = d
            contentDescription = "连接状态"
            layoutParams = LinearLayout.LayoutParams(dp(9), dp(9)).apply { gravity = Gravity.CENTER_VERTICAL }
        }
        status = TextView(this).apply {
            text = "未连接"
            setTextColor(palette.secondary)
            textSize = 12f
            includeFontPadding = false
            typeface = android.graphics.Typeface.create("sans-serif", android.graphics.Typeface.NORMAL)
            maxLines = 1
        }

        val headerBg = GradientDrawable().apply {
            setColor(palette.chrome)
            cornerRadius = dp(16).toFloat()
            setStroke(dp(1), palette.chromeStroke)
        }
        header = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = headerBg
            elevation = dp(3).toFloat()
            setPadding(dp(18), dp(4), dp(12), dp(4))
            val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                setMargins(dp(16), dp(12), dp(16), dp(8))
            }
            layoutParams = lp
            addView(dot)
            headerInfo = LinearLayout(this@MainActivity).apply {
                orientation = LinearLayout.VERTICAL
                gravity = Gravity.CENTER_VERTICAL
                layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply {
                    marginStart = dp(10)
                    marginEnd = dp(10)
                }
                addView(TextView(this@MainActivity).apply {
                    text = "TRACKPAD COMPANION"
                    setTextColor(palette.secondary)
                    textSize = 8f
                    includeFontPadding = false
                    letterSpacing = 0.12f
                    typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL)
                })
                addView(TextView(this@MainActivity).apply {
                    text = "Mac 触控板"
                    setTextColor(palette.label)
                    textSize = 15f
                    includeFontPadding = false
                    typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL)
                })
                addView(status)
            }
            addView(headerInfo)
            connectButton = actionButton("连接 Mac", true) {
                if (isConnecting) cancelMacConnection() else showConnectionDialog()
            }.apply {
                contentDescription = "配置并连接 Mac"
                layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, dp(40))
            }
            addView(connectButton)
        }

        val sensitivity = TextView(this).apply {
            setTextColor(palette.label)
            textSize = 12f
            gravity = Gravity.CENTER
            minWidth = dp(86)
            text = "灵敏度 ${(pad.scale * 100).toInt()}%"
        }
        fun updateSensitivity() {
            sensitivity.text = "灵敏度 ${(pad.scale * 100).toInt()}%"
        }

        controls = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(14), dp(2), dp(14), dp(4))

            addView(actionButton("−") {
                pad.scale = (pad.scale / 1.15f).coerceIn(0.55f, 1.6f)
                prefs.edit().putFloat(KEY_SCALE, pad.scale).apply()
                updateSensitivity()
            }.apply { contentDescription = "降低灵敏度" })
            addView(sensitivity, LinearLayout.LayoutParams(dp(94), dp(44)))
            addView(actionButton("+") {
                pad.scale = (pad.scale * 1.15f).coerceIn(0.55f, 1.6f)
                prefs.edit().putFloat(KEY_SCALE, pad.scale).apply()
                updateSensitivity()
            }.apply { contentDescription = "提高灵敏度" })

            lateinit var hapticBtn: Button
            val hapticsOn = prefs.getBoolean(KEY_HAPTIC, true)
            pad.haptics.enabled = hapticsOn
            hapticBtn = actionButton(if (hapticsOn) "震动开" else "震动关") {
                val next = !prefs.getBoolean(KEY_HAPTIC, true)
                pad.haptics.enabled = next
                prefs.edit().putBoolean(KEY_HAPTIC, next).apply()
                hapticBtn.text = if (next) "震动开" else "震动关"
                if (next) pad.haptics.click()
            }.apply { contentDescription = "切换触觉反馈" }
            addView(hapticBtn, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, dp(44)).apply { marginStart = dp(8) })

            lateinit var touchPointsBtn: Button
            val touchPointsOn = prefs.getBoolean(
                KEY_TOUCH_POINTS,
                prefs.getBoolean(KEY_VISUAL_EFFECTS, true),
            )
            touchPointsBtn = actionButton(if (touchPointsOn) "触点开" else "触点关") {
                val next = !prefs.getBoolean(
                    KEY_TOUCH_POINTS,
                    prefs.getBoolean(KEY_VISUAL_EFFECTS, true),
                )
                prefs.edit().putBoolean(KEY_TOUCH_POINTS, next).remove(KEY_VISUAL_EFFECTS).apply()
                pad.visualEffectsEnabled = next
                touchPointsBtn.text = if (next) "触点开" else "触点关"
            }.apply { contentDescription = "显示或隐藏触点" }
            addView(touchPointsBtn, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, dp(44)).apply { marginStart = dp(8) })

            addView(actionButton("深按条") { showDeepPressSettingsDialog() }.apply {
                contentDescription = "深按条设置"
                layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, dp(44)).apply { marginStart = dp(8) }
            })
            addView(actionButton("测试") { showGestureTestDialog() }.apply {
                contentDescription = "打开手势测试"
                layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, dp(44)).apply { marginStart = dp(8) }
            })
            addView(actionButton("全屏") { toggleFullscreen(true) }.apply {
                contentDescription = "进入全屏触控模式"
                layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, dp(44)).apply { marginStart = dp(8) }
            })
            addView(actionButton("外观") { showThemeDialog() }.apply {
                contentDescription = "切换界面主题"
                layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, dp(44)).apply { marginStart = dp(8) }
            })
        }

        val padSurface = surface(palette.pad, 28, palette.padStroke)
        padFrame.background = if (palette.usesLiquidGlass) {
            GradientDrawable().apply {
                // QWEA0 owns the sampled lens surface. An opaque tint here
                // flattens the captured scene into a gray slab on Android GPU
                // drivers, so keep the child frame optically transparent.
                // A low-alpha optical tint keeps the captured scene bright on
                // high-density Android GPUs without turning the pad opaque.
                setColor(if (palette.sceneDark) 0x180A1220 else 0x18FFFFFF)
                cornerRadius = dp(28).toFloat()
                setStroke(dp(1), if (palette.sceneDark) 0x52FFFFFF else 0x66FFFFFF.toInt())
            }
        } else {
            GradientDrawable().apply {
                setColor(Color.TRANSPARENT)
                cornerRadius = dp(28).toFloat()
                setStroke(dp(1), palette.padStroke)
            }
        }
        padFrame.clipToOutline = true
        padFrame.setPadding(dp(8), dp(8), dp(8), dp(8))
        normalPadFrameBackground = padFrame.background
        controlsRail = actionButton("控制中心") { showControlCenterDialog() }.apply {
            contentDescription = "打开触控板控制中心"
            minWidth = dp(82)
            layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, dp(40)).apply { marginStart = dp(6) }
        }
        headerToggle = actionButton("⛶") { toggleFullscreen(true) }.apply {
            contentDescription = "进入全屏触控模式"
            minWidth = dp(44)
            layoutParams = LinearLayout.LayoutParams(dp(44), dp(44)).apply { marginStart = dp(6) }
        }
        header.addView(controlsRail, 2, (controlsRail.layoutParams as LinearLayout.LayoutParams))
        header.addView(headerToggle)

        fullscreenFloatBtn = actionButton("×") { toggleFullscreen(false) }.apply {
            alpha = 0.78f
            visibility = View.GONE
            val lp = FrameLayout.LayoutParams(dp(44), dp(44)).apply {
                gravity = Gravity.TOP or Gravity.END
                setMargins(0, dp(12), dp(12), 0)
            }
            layoutParams = lp
            contentDescription = "退出全屏并打开设置"
        }

        val backdropLayer = object : FrameLayout(this) {
            private val paint = Paint(Paint.ANTI_ALIAS_FLAG)

            override fun onDraw(canvas: android.graphics.Canvas) {
                val scene = paletteFor(ThemeMode.from(prefs.getString(KEY_THEME, ThemeMode.LIGHT_GLASS.key)))
                val bitmap = wallpaperBitmap
                if (bitmap != null && width > 0 && height > 0) {
                    val scale = max(width.toFloat() / bitmap.width, height.toFloat() / bitmap.height)
                    val drawWidth = bitmap.width * scale
                    val drawHeight = bitmap.height * scale
                    val left = (width - drawWidth) / 2f
                    val top = (height - drawHeight) / 2f
                    paint.shader = null
                    val wallpaperOpacity = prefs.getInt(KEY_WALLPAPER_OPACITY, DEFAULT_WALLPAPER_OPACITY)
                        .coerceIn(0, 100)
                    val wallpaperSaturation = prefs.getInt(KEY_WALLPAPER_SATURATION, DEFAULT_WALLPAPER_SATURATION)
                        .coerceIn(60, 140) / 100f
                    val wallpaperBrightness = prefs.getInt(KEY_WALLPAPER_BRIGHTNESS, DEFAULT_WALLPAPER_BRIGHTNESS)
                        .coerceIn(70, 130) / 100f
                    val colorMatrix = ColorMatrix().apply { setSaturation(wallpaperSaturation) }
                    colorMatrix.postConcat(ColorMatrix(floatArrayOf(
                        wallpaperBrightness, 0f, 0f, 0f, 0f,
                        0f, wallpaperBrightness, 0f, 0f, 0f,
                        0f, 0f, wallpaperBrightness, 0f, 0f,
                        0f, 0f, 0f, 1f, 0f,
                    )))
                    paint.colorFilter = ColorMatrixColorFilter(colorMatrix)
                    paint.alpha = (wallpaperOpacity * 2.55f).toInt().coerceIn(0, 255)
                    canvas.drawBitmap(bitmap, null, android.graphics.RectF(left, top, left + drawWidth, top + drawHeight), paint)
                    paint.colorFilter = null
                    paint.alpha = 255
                    // A wallpaper is already the visual identity of the scene.
                    // Keep only a restrained readability scrim; theme gradients
                    // belong to the no-wallpaper state and otherwise shift the
                    // user's image toward an unintended hue.
                    paint.color = if (scene.sceneDark) 0x24070B12 else 0x18FFFFFF
                    canvas.drawRect(0f, 0f, width.toFloat(), height.toFloat(), paint)
                } else {
                    val tint = { color: Int, alpha: Int ->
                        Color.argb(alpha, Color.red(color), Color.green(color), Color.blue(color))
                    }
                    paint.shader = LinearGradient(
                        0f, 0f, width.toFloat(), height.toFloat(),
                        intArrayOf(scene.sceneStart, scene.sceneMid, scene.sceneEnd),
                        null,
                        Shader.TileMode.CLAMP,
                    )
                    canvas.drawRect(0f, 0f, width.toFloat(), height.toFloat(), paint)
                    // Continuous fields provide a quiet optical backdrop when
                    // no wallpaper is selected; they are intentionally absent
                    // when a user image is present.
                    paint.shader = LinearGradient(
                        width * -.18f, height * .92f, width * 1.18f, height * .08f,
                        intArrayOf(scene.sceneStart, tint(scene.sceneGlowA, 0x66), scene.sceneMid, tint(scene.sceneGlowB, 0x66), scene.sceneEnd),
                        floatArrayOf(0f, .22f, .48f, .74f, 1f),
                        Shader.TileMode.CLAMP,
                    )
                    canvas.drawRect(0f, 0f, width.toFloat(), height.toFloat(), paint)
                    paint.shader = LinearGradient(
                        0f, height * .04f, width.toFloat(), height * .96f,
                        intArrayOf(0x30FFFFFF, 0x00000000, scene.sceneGlowC, 0x00000000),
                        floatArrayOf(0f, .30f, .68f, 1f),
                        Shader.TileMode.CLAMP,
                    )
                    canvas.drawRect(0f, 0f, width.toFloat(), height.toFloat(), paint)
                }
                paint.shader = null
                paint.colorFilter = null
            }
        }.apply {
            setWillNotDraw(false)
        }
        val contentLayer = FrameLayout(this).apply {
            // Keep the sampled scene full-window. Chrome glass at the top and
            // bottom must see the same continuous backdrop as the pad; if the
            // source stops at the pad bounds, those regions sample transparency
            // and become an opaque-looking black slab on some GPU drivers.
            addView(backdropLayer, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            if (palette.usesLiquidGlass) {
                val liquid = palette.liquid ?: LIQUID_DEFAULT
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    // API 31+ uses one local AGSL pass: a half-resolution scene
                    // texture is sampled for refraction, dispersion and touch
                    // lighting without QWEA0's several intermediate surfaces.
                    val gpu = GpuGlassView(this@MainActivity).apply {
                        setOptics(
                            refractionHeight = liquid.refractionHeight,
                            dispersionStrength = liquid.dispersionStrength,
                            saturation = liquid.saturation,
                            highlightOpacity = liquid.highlightOpacity,
                            accentColor = palette.accent,
                        )
                        setFullscreen(isFullscreenMode)
                        addView(padFrame, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
                    }
                    gpuGlassView = gpu
                    padGlassView = null
                    padHost = gpu
                    addView(gpu, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT).apply {
                        setMargins(dp(18), dp(68), dp(18), dp(66))
                    })
                    gpu.post { gpu.setBackdropSource(backdropLayer) }
                } else {
                    val padGlass = LiquidGlassView(this@MainActivity).apply {
                        cornerRadius = dp(30).toFloat()
                        // CLEAR keeps the scene recognizable through the lens.
                        material = GlassMaterial.CLEAR
                        useShaderPipeline = true
                        // Keep the full dynamic lens during a gesture; the touch
                        // host toggles this off as soon as all fingers lift.
                        enableDynamicBackground = false
                        globalDownsampleFactor = GLASS_GLOBAL_DOWNSAMPLE
                        downsampleScale = GLASS_DOWNSAMPLE_SCALE
                        enableOptimizedCapture = true
                        highQualityBlur = false
                        blurMethod = BlurMethod.DOWNSAMPLE
                        collectFrameStats = false
                        useHardwareBlurWhenPossible = true
                        enableBackdropBlur = true
                        enableChromaticAberration = true
                        enableChromaticDispersion = true
                        aberrationDownsample = 0.35f
                        dispersionDownsample = 0.35f
                        enableEdgeHighlight = true
                        enableSensorHighlight = true
                        enableAdaptiveTint = liquid.adaptiveTint
                        overLight = !palette.sceneDark
                        enableShadow = false
                        bevelWidth = liquid.bevelWidth
                        refractionHeight = liquid.refractionHeight
                        dispersionStrength = liquid.dispersionStrength
                        blurAmount = liquid.blurAmount
                        saturation = liquid.saturation
                        edgeHighlightOpacity = liquid.highlightOpacity
                        enablePressEffect = false
                        addView(padFrame, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
                    }
                    padGlassView = padGlass
                    gpuGlassView = null
                    padHost = padGlass
                    addView(padGlass, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT).apply {
                        setMargins(dp(18), dp(68), dp(18), dp(66))
                    })
                    padGlass.post { padGlass.backdropSource = backdropLayer }
                }
            } else {
                padGlassView = null
                gpuGlassView = null
                padHost = padFrame
                addView(padFrame, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT).apply {
                    setMargins(dp(18), dp(68), dp(18), dp(66))
                })
            }
        }
        val rootFrame = FrameLayout(this).apply {
            setBackgroundColor(palette.canvas)
            addView(contentLayer, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        }

        fun addChromeSurface(content: View, radius: Int, top: Boolean) {
            // Keep chrome native and predictable. QWEA0 is reserved for the
            // touch surface itself; applying a full lens to toolbar bands makes
            // text unreadable on several Android GPU drivers.
            val lp = FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(if (top) 52 else 50)).apply {
                gravity = if (top) Gravity.TOP else Gravity.BOTTOM
                setMargins(dp(12), if (top) dp(6) else 0, dp(12), if (top) 0 else dp(6))
            }
            content.layoutParams = lp
            content.background = GradientDrawable().apply {
                setColor(palette.chrome)
                cornerRadius = dp(radius + 4).toFloat()
                setStroke(dp(1), palette.chromeStroke)
            }
            content.elevation = dp(3).toFloat()
            rootFrame.addView(content, lp)
        }

        addChromeSurface(header, 12, top = true)
        rootFrame.addView(fullscreenFloatBtn)

        padHost.alpha = surfaceOpacityFraction()

        setContentView(rootFrame)
        padFrame.post { layoutDeepPressBar() }

        if (!prefs.getString(KEY_HOST, "").isNullOrEmpty()) {
            connectToMac(
                prefs.getString(KEY_HOST, "") ?: "",
                prefs.getString(KEY_PORT, "4242") ?: "4242",
                prefs.getString(KEY_TOKEN, "") ?: "",
                prefs.getBoolean(KEY_WEB_ENABLED, true),
            )
        }

        immersive()
        excludeSystemGestures(rootFrame)
    }

    private fun connectToMac(host: String, portText: String, tokenText: String, probeWeb: Boolean = true) {
        val port = portText.toIntOrNull()?.coerceIn(1, 65535) ?: 4242
        val attempt = ++connectionAttemptSerial
        prefs.edit()
            .putString(KEY_HOST, host)
            .putString(KEY_PORT, port.toString())
            .putString(KEY_TOKEN, tokenText)
            .putBoolean(KEY_WEB_ENABLED, probeWeb)
            .apply()
        isConnecting = true
        setStatus(false, "连接中…")
        sender.connect(host, port, tokenText.ifEmpty { null }, probeWeb, object : UdpSender.Listener {
            override fun onState(connected: Boolean, message: String) =
                runOnUiThread {
                    if (attempt == connectionAttemptSerial) setStatus(connected, message)
                }
        })
    }

    private fun cancelMacConnection() {
        if (!isConnecting) return
        connectionAttemptSerial += 1
        sender.cancelConnect()
        setStatus(false, "已取消连接")
    }

    private fun disconnectFromMac() {
        connectionAttemptSerial += 1
        isConnecting = false
        sender.cancelConnect()
        setStatus(false, "已断开连接")
    }

    private fun themePalette(): ThemePalette {
        val mode = ThemeMode.from(prefs.getString(KEY_THEME, ThemeMode.LIGHT_GLASS.key))
        val base = paletteFor(mode)
        if (mode != ThemeMode.CUSTOM_GLASS) return base
        val liquid = base.liquid ?: LIQUID_DEFAULT
        return base.copy(liquid = liquid.copy(
            refractionHeight = prefs.getFloat(KEY_GLASS_REFRACTION, liquid.refractionHeight),
            saturation = prefs.getFloat(KEY_GLASS_SATURATION, liquid.saturation),
            blurAmount = prefs.getFloat(KEY_GLASS_BLUR, liquid.blurAmount),
            highlightOpacity = prefs.getFloat(KEY_GLASS_HIGHLIGHT, liquid.highlightOpacity),
        ))
    }

    private fun loadWallpaper(): Bitmap? {
        val custom = prefs.getString(KEY_WALLPAPER_URI, null)
        if (!custom.isNullOrBlank()) {
            val customBitmap = runCatching {
                decodeScaledBitmap { contentResolver.openInputStream(Uri.parse(custom)) }
            }.getOrNull()
            if (customBitmap != null) return customBitmap
        }
        val asset = when (prefs.getString(KEY_WALLPAPER_PRESET, "none")) {
            "mountain" -> "wallpaper_mountain.jpg"
            "night" -> "wallpaper_night.jpg"
            "canyon" -> "wallpaper_canyon.jpg"
            "anime" -> "wallpaper_anime.jpg"
            else -> null
        } ?: return null
        return runCatching { decodeScaledBitmap { assets.open(asset) } }.getOrNull()
    }

    /** Decode only enough pixels for the background lens; never retain a full
     * camera/desktop-sized source image when a smaller texture is sufficient. */
    private fun decodeScaledBitmap(openStream: () -> InputStream?): Bitmap? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        openStream()?.use { BitmapFactory.decodeStream(it, null, bounds) }
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null

        val longestEdge = max(bounds.outWidth, bounds.outHeight)
        var sample = 1
        while (longestEdge / sample > WALLPAPER_MAX_EDGE_PX) sample *= 2

        val options = BitmapFactory.Options().apply {
            inSampleSize = sample
            inPreferredConfig = Bitmap.Config.ARGB_8888
            inScaled = false
        }
        return openStream()?.use { BitmapFactory.decodeStream(it, null, options) }
    }

    private fun setWallpaperPreset(key: String) {
        prefs.edit()
            .putString(KEY_WALLPAPER_PRESET, key)
            .remove(KEY_WALLPAPER_URI)
            .apply()
        recreate()
    }

    private fun pickWallpaper() {
        startActivityForResult(
            Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                type = "image/*"
                putExtra(Intent.EXTRA_MIME_TYPES, arrayOf("image/jpeg", "image/png", "image/webp"))
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
            },
            REQUEST_WALLPAPER,
        )
    }

    private fun showThemeDialog() {
        val selected = ThemeMode.from(prefs.getString(KEY_THEME, ThemeMode.LIGHT_GLASS.key))
        val palette = themePalette()
        val dialog = android.app.Dialog(this).apply {
            requestWindowFeature(android.view.Window.FEATURE_NO_TITLE)
            window?.setBackgroundDrawable(android.graphics.drawable.ColorDrawable(Color.TRANSPARENT))
        }
        val container = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            background = GradientDrawable().apply {
                setColor(palette.chrome)
                cornerRadius = dp(20).toFloat()
                setStroke(dp(1), palette.chromeStroke)
            }
            setPadding(dp(20), dp(18), dp(20), dp(18))
        }
        container.addView(TextView(this).apply {
            text = "外观主题"
            setTextColor(palette.label)
            textSize = 20f
            typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL)
        })
        container.addView(TextView(this).apply {
            text = "玻璃只用于功能层，触控面保持清晰。选择后立即重载界面。"
            setTextColor(palette.secondary)
            textSize = 13f
            setPadding(0, dp(5), 0, dp(12))
        })
        if (selected == ThemeMode.CUSTOM_GLASS) {
            container.addView(actionSheetButton("调整液态玻璃参数", false) {
                dialog.dismiss()
                showCustomGlassDialog()
            }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(46)).apply {
                bottomMargin = dp(8)
            })
        }

        var lastCategory = ""
        CURATED_THEME_MODES.forEach { mode ->
            val modePalette = paletteFor(mode)
            val category = when (modePalette.material) {
                MaterialKind.LIQUID_GLASS -> "Liquid Glass"
                MaterialKind.DROPLET_GLASS, MaterialKind.RIPPLE_WATER, MaterialKind.RAIN_GLASS,
                MaterialKind.PRISM_CRYSTAL, MaterialKind.GEL_SURFACE, MaterialKind.LIQUID_METAL,
                MaterialKind.PAPER_TEXTURE, MaterialKind.HOLOGRAPHIC, MaterialKind.RETRO_LCD -> "材质实验室"
                MaterialKind.CERAMIC -> if (mode.key in setOf("classic-light", "classic-dark", "high-contrast")) "经典与辅助" else "编辑器主题"
            }
            if (category != lastCategory) {
                container.addView(TextView(this).apply {
                    text = category
                    setTextColor(palette.secondary)
                    textSize = 11f
                    letterSpacing = 0.08f
                    typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL)
                    setPadding(dp(4), dp(10), dp(4), dp(6))
                })
                lastCategory = category
            }
            val row = LinearLayout(this).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                isClickable = true
                isFocusable = true
                setPadding(dp(12), dp(10), dp(12), dp(10))
                background = GradientDrawable().apply {
                    setColor(if (mode == selected) Color.argb(37, Color.red(palette.accent), Color.green(palette.accent), Color.blue(palette.accent)) else Color.TRANSPARENT)
                    cornerRadius = dp(12).toFloat()
                    if (mode == selected) setStroke(dp(1), palette.accent)
                }
                setOnClickListener {
                    prefs.edit().putString(KEY_THEME, mode.key).apply()
                    dialog.dismiss()
                    recreate()
                }
            }
            val swatch = View(this).apply {
                val swatchPalette = paletteFor(mode)
                val colors = if (swatchPalette.material != MaterialKind.CERAMIC) {
                    intArrayOf(swatchPalette.sceneGlowA, swatchPalette.chrome, swatchPalette.sceneGlowB)
                } else intArrayOf(swatchPalette.chrome, swatchPalette.button)
                background = GradientDrawable(GradientDrawable.Orientation.TL_BR, colors).apply {
                    cornerRadius = dp(9).toFloat()
                    setStroke(dp(1), swatchPalette.chromeStroke)
                }
                layoutParams = LinearLayout.LayoutParams(dp(38), dp(30))
            }
            row.addView(swatch)
            row.addView(LinearLayout(this@MainActivity).apply {
                orientation = LinearLayout.VERTICAL
                layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply {
                    marginStart = dp(12)
                }
                addView(TextView(this@MainActivity).apply {
                    text = mode.title
                    setTextColor(palette.label)
                    textSize = 14f
                    typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL)
                })
                addView(TextView(this@MainActivity).apply {
                    text = mode.detail
                    setTextColor(palette.secondary)
                    textSize = 11f
                })
            })
            container.addView(row, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(54)).apply {
                bottomMargin = dp(6)
            })
        }
        container.addView(TextView(this).apply {
            text = "背景壁纸"
            setTextColor(palette.secondary)
            textSize = 11f
            letterSpacing = 0.08f
            setPadding(dp(4), dp(12), dp(4), dp(6))
        })
        val wallpaperChoices = listOf(
            "主题背景" to "none",
            "晨光山野" to "mountain",
            "霓虹夜城" to "night",
            "荒野公路" to "canyon",
            "二次元红幕" to "anime",
        )
        wallpaperChoices.chunked(2).forEach { pair ->
            val row = LinearLayout(this).apply {
                orientation = LinearLayout.HORIZONTAL
            }
            pair.forEach { (label, key) ->
                row.addView(actionSheetButton(label, false) { setWallpaperPreset(key) }, LinearLayout.LayoutParams(0, dp(46), 1f).apply {
                    if (row.childCount > 0) marginStart = dp(6)
                    bottomMargin = dp(6)
                })
            }
            if (pair.size == 1) row.addView(View(this), LinearLayout.LayoutParams(0, dp(46), 1f).apply { marginStart = dp(6) })
            container.addView(row)
        }
        container.addView(actionSheetButton("从相册选择自定义壁纸", false) {
            dialog.dismiss()
            pickWallpaper()
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(46)).apply {
            bottomMargin = dp(6)
        })
        container.addView(TextView(this).apply {
            text = "背景外观"
            setTextColor(palette.secondary)
            textSize = 11f
            letterSpacing = 0.08f
            setPadding(dp(4), dp(12), dp(4), dp(6))
        })
        var pendingWallpaperOpacity = prefs.getInt(KEY_WALLPAPER_OPACITY, DEFAULT_WALLPAPER_OPACITY)
        var pendingWallpaperSaturation = prefs.getInt(KEY_WALLPAPER_SATURATION, DEFAULT_WALLPAPER_SATURATION)
        var pendingWallpaperBrightness = prefs.getInt(KEY_WALLPAPER_BRIGHTNESS, DEFAULT_WALLPAPER_BRIGHTNESS)
        var pendingSurfaceOpacity = prefs.getInt(KEY_SURFACE_OPACITY, DEFAULT_SURFACE_OPACITY)
        fun appearanceSlider(title: String, min: Int, max: Int, initial: Int, onChange: (Int) -> Unit) {
            val row = LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(0, dp(3), 0, dp(3))
            }
            val label = TextView(this).apply {
                text = "$title  ${initial}%"
                setTextColor(palette.label)
                textSize = 13f
            }
            val seek = android.widget.SeekBar(this).apply {
                this.max = max - min
                progress = (initial - min).coerceIn(0, max - min)
                contentDescription = title
                setOnSeekBarChangeListener(object : android.widget.SeekBar.OnSeekBarChangeListener {
                    override fun onProgressChanged(seekBar: android.widget.SeekBar?, progress: Int, fromUser: Boolean) {
                        val next = progress + min
                        label.text = "$title  ${next}%"
                        onChange(next)
                    }
                    override fun onStartTrackingTouch(seekBar: android.widget.SeekBar?) = Unit
                    override fun onStopTrackingTouch(seekBar: android.widget.SeekBar?) = Unit
                })
            }
            row.addView(label)
            row.addView(seek, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(38)))
            container.addView(row)
        }
        appearanceSlider("背景可见度", 0, 100, pendingWallpaperOpacity) { pendingWallpaperOpacity = it }
        appearanceSlider("背景饱和度", 60, 140, pendingWallpaperSaturation) { pendingWallpaperSaturation = it }
        appearanceSlider("背景亮度", 70, 130, pendingWallpaperBrightness) { pendingWallpaperBrightness = it }
        appearanceSlider("触控面透明度", 55, 100, pendingSurfaceOpacity) { pendingSurfaceOpacity = it }
        container.addView(actionSheetButton("应用背景外观", true) {
            prefs.edit()
                .putInt(KEY_WALLPAPER_OPACITY, pendingWallpaperOpacity)
                .putInt(KEY_WALLPAPER_SATURATION, pendingWallpaperSaturation)
                .putInt(KEY_WALLPAPER_BRIGHTNESS, pendingWallpaperBrightness)
                .putInt(KEY_SURFACE_OPACITY, pendingSurfaceOpacity)
                .apply()
            dialog.dismiss()
            recreate()
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(46)).apply {
            topMargin = dp(8)
            bottomMargin = dp(6)
        })
        val cancel = actionSheetButton("取消", false) { dialog.dismiss() }
        container.addView(cancel, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(46)).apply {
            topMargin = dp(4)
        })
        val scroll = android.widget.ScrollView(this).apply {
            isFillViewport = true
            addView(container)
        }
        dialog.setContentView(scroll)
        beginModal(dialog)
        dialog.show()
        dialog.window?.setLayout(
            (resources.displayMetrics.widthPixels * 0.90f).toInt(),
            (resources.displayMetrics.heightPixels * 0.82f).toInt(),
        )
    }

    private fun showCustomGlassDialog() {
        val palette = themePalette()
        val base = palette.liquid ?: LIQUID_DEFAULT
        val dialog = android.app.Dialog(this).apply {
            requestWindowFeature(android.view.Window.FEATURE_NO_TITLE)
            window?.setBackgroundDrawable(android.graphics.drawable.ColorDrawable(Color.TRANSPARENT))
        }
        val container = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            background = GradientDrawable().apply {
                setColor(palette.chrome)
                cornerRadius = dp(18).toFloat()
                setStroke(dp(1), palette.chromeStroke)
            }
            setPadding(dp(20), dp(18), dp(20), dp(18))
        }
        container.addView(TextView(this).apply {
            text = "自定义液态玻璃"
            setTextColor(palette.label)
            textSize = 20f
            typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL)
        })
        container.addView(TextView(this).apply {
            text = "调整折射、饱和度、柔化和边缘高光。应用后重载触控面。"
            setTextColor(palette.secondary)
            textSize = 13f
            setPadding(0, dp(5), 0, dp(12))
        })

        fun slider(title: String, min: Int, max: Int, initial: Int, format: (Int) -> String, onChange: (Int) -> Unit) {
            val row = LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(0, dp(7), 0, dp(7))
            }
            val label = TextView(this).apply {
                setTextColor(palette.label)
                textSize = 13f
                text = "$title  ${format(initial)}"
            }
            val seek = android.widget.SeekBar(this).apply {
                this.max = max - min
                progress = (initial - min).coerceIn(0, max - min)
                contentDescription = title
                setOnSeekBarChangeListener(object : android.widget.SeekBar.OnSeekBarChangeListener {
                    override fun onProgressChanged(seekBar: android.widget.SeekBar?, progress: Int, fromUser: Boolean) {
                        val next = progress + min
                        label.text = "$title  ${format(next)}"
                        onChange(next)
                    }
                    override fun onStartTrackingTouch(seekBar: android.widget.SeekBar?) = Unit
                    override fun onStopTrackingTouch(seekBar: android.widget.SeekBar?) = Unit
                })
            }
            row.addView(label)
            row.addView(seek, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(42)))
            container.addView(row)
        }

        var refraction = base.refractionHeight.toInt()
        var saturation = base.saturation.toInt()
        var blur = (base.blurAmount * 1000f).toInt()
        var highlight = base.highlightOpacity.toInt()
        slider("折射高度", 120, 520, refraction, { "$it" }) { refraction = it }
        slider("玻璃饱和度", 90, 210, saturation, { "$it%" }) { saturation = it }
        slider("柔化程度", 0, 100, blur, { "${it / 10f} px" }) { blur = it }
        slider("边缘高光", 35, 140, highlight, { "$it%" }) { highlight = it }

        val actions = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.END
        }
        actions.addView(actionSheetButton("取消", false) { dialog.dismiss() }, LinearLayout.LayoutParams(0, dp(48), 1f).apply { marginEnd = dp(8) })
        actions.addView(actionSheetButton("应用", true) {
            prefs.edit()
                .putFloat(KEY_GLASS_REFRACTION, refraction.toFloat())
                .putFloat(KEY_GLASS_SATURATION, saturation.toFloat())
                .putFloat(KEY_GLASS_BLUR, blur / 1000f)
                .putFloat(KEY_GLASS_HIGHLIGHT, highlight.toFloat())
                .apply()
            dialog.dismiss()
            recreate()
        }, LinearLayout.LayoutParams(0, dp(48), 1f))
        container.addView(actions, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(52)).apply { topMargin = dp(8) })
        val scroll = android.widget.ScrollView(this).apply {
            isFillViewport = true
            addView(container)
        }
        dialog.setContentView(scroll)
        beginModal(dialog)
        dialog.show()
        dialog.window?.setLayout((resources.displayMetrics.widthPixels * 0.92f).toInt(), ViewGroup.LayoutParams.WRAP_CONTENT)
    }

    private fun showControlCenterDialog() {
        val palette = themePalette()
        val dialog = android.app.Dialog(this).apply {
            requestWindowFeature(android.view.Window.FEATURE_NO_TITLE)
            window?.setBackgroundDrawable(android.graphics.drawable.ColorDrawable(Color.TRANSPARENT))
        }
        val container = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            background = GradientDrawable().apply {
                setColor(palette.chrome)
                cornerRadius = dp(20).toFloat()
                setStroke(dp(1), palette.chromeStroke)
            }
            setPadding(dp(20), dp(18), dp(20), dp(18))
        }
        container.addView(TextView(this).apply {
            text = "控制中心"
            setTextColor(palette.label)
            textSize = 20f
            typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL)
        })
        container.addView(TextView(this).apply {
            text = "常用触控板操作"
            setTextColor(palette.secondary)
            textSize = 13f
            setPadding(0, dp(5), 0, dp(14))
        })

        val sectionLabel = TextView(this).apply {
            text = "快速控制"
            setTextColor(palette.secondary)
            textSize = 11f
            letterSpacing = .08f
            setPadding(dp(4), 0, dp(4), dp(6))
        }
        container.addView(sectionLabel)
        val sensitivityValue = TextView(this).apply {
            setTextColor(palette.label)
            textSize = 13f
            gravity = Gravity.CENTER
            minWidth = dp(88)
            text = "灵敏度 ${(pad.scale * 100).toInt()}%"
        }
        fun refreshSensitivity() { sensitivityValue.text = "灵敏度 ${(pad.scale * 100).toInt()}%" }
        val sensitivityRow = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(0, dp(2), 0, dp(8))
            addView(actionSheetButton("−", false) {
                pad.scale = (pad.scale / 1.15f).coerceIn(.55f, 1.6f)
                prefs.edit().putFloat(KEY_SCALE, pad.scale).apply()
                refreshSensitivity()
            }, LinearLayout.LayoutParams(0, dp(46), 1f).apply { marginEnd = dp(6) })
            addView(sensitivityValue, LinearLayout.LayoutParams(dp(96), dp(46)))
            addView(actionSheetButton("+", false) {
                pad.scale = (pad.scale * 1.15f).coerceIn(.55f, 1.6f)
                prefs.edit().putFloat(KEY_SCALE, pad.scale).apply()
                refreshSensitivity()
            }, LinearLayout.LayoutParams(0, dp(46), 1f).apply { marginStart = dp(6) })
        }
        container.addView(sensitivityRow)

        val hapticsOn = prefs.getBoolean(KEY_HAPTIC, true)
        pad.haptics.enabled = hapticsOn
        lateinit var hapticButton: Button
        hapticButton = actionSheetButton(if (hapticsOn) "震动开" else "震动关", false) {
            val next = !prefs.getBoolean(KEY_HAPTIC, true)
            pad.haptics.enabled = next
            prefs.edit().putBoolean(KEY_HAPTIC, next).apply()
            hapticButton.text = if (next) "震动开" else "震动关"
            if (next) pad.haptics.click()
        }
        val touchPointsOn = prefs.getBoolean(
            KEY_TOUCH_POINTS,
            prefs.getBoolean(KEY_VISUAL_EFFECTS, true),
        )
        lateinit var touchPointsButton: Button
        touchPointsButton = actionSheetButton(if (touchPointsOn) "触点开" else "触点关", false) {
            val next = !prefs.getBoolean(
                KEY_TOUCH_POINTS,
                prefs.getBoolean(KEY_VISUAL_EFFECTS, true),
            )
            prefs.edit().putBoolean(KEY_TOUCH_POINTS, next).remove(KEY_VISUAL_EFFECTS).apply()
            pad.visualEffectsEnabled = next
            touchPointsButton.text = if (next) "触点开" else "触点关"
        }
        val toggleRow = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            addView(hapticButton, LinearLayout.LayoutParams(0, dp(46), 1f).apply { marginEnd = dp(6) })
            addView(touchPointsButton, LinearLayout.LayoutParams(0, dp(46), 1f).apply { marginStart = dp(6) })
        }
        container.addView(toggleRow, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(52)))

        container.addView(TextView(this).apply {
            text = "更多"
            setTextColor(palette.secondary)
            textSize = 11f
            letterSpacing = .08f
            setPadding(dp(4), dp(10), dp(4), dp(6))
        })
        val secondaryRow = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            addView(actionSheetButton("深按条", false) { showDeepPressSettingsDialog() }, LinearLayout.LayoutParams(0, dp(46), 1f).apply { marginEnd = dp(6) })
            addView(actionSheetButton("测试", false) { showGestureTestDialog() }, LinearLayout.LayoutParams(0, dp(46), 1f).apply { marginStart = dp(6) })
        }
        container.addView(secondaryRow, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(52)))
        val appearanceRow = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            addView(actionSheetButton("外观", false) { showThemeDialog() }, LinearLayout.LayoutParams(0, dp(46), 1f).apply { marginEnd = dp(6) })
            addView(actionSheetButton("全屏", true) {
                dialog.dismiss()
                toggleFullscreen(true)
            }, LinearLayout.LayoutParams(0, dp(46), 1f).apply { marginStart = dp(6) })
        }
        container.addView(appearanceRow, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(52)))
        container.addView(actionSheetButton("完成", false) { dialog.dismiss() }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(48)).apply { topMargin = dp(10) })

        val scroll = android.widget.ScrollView(this).apply {
            isFillViewport = true
            addView(container)
        }
        dialog.setContentView(scroll)
        beginModal(dialog)
        dialog.show()
        dialog.window?.setLayout(
            (resources.displayMetrics.widthPixels * 0.92f).toInt(),
            (resources.displayMetrics.heightPixels * 0.82f).toInt(),
        )
    }

    private fun startQrScanner(dialog: android.app.Dialog) {
        dialog.dismiss()
        runCatching {
            startActivityForResult(Intent(this, QrScannerActivity::class.java), REQUEST_QR_SCANNER)
        }.onFailure {
            Toast.makeText(this, "无法打开扫码页面，请改用 IP 连接。", Toast.LENGTH_LONG).show()
            showConnectionDialog()
        }
    }

    private fun applyPairingTarget(target: PairingTarget): Boolean {
        if (!target.phoneEnabled) {
            Toast.makeText(this, "该 Mac 未开放手机连接。", Toast.LENGTH_LONG).show()
            return false
        }
        prefs.edit()
            .putString(KEY_HOST, target.host)
            .putString(KEY_PORT, target.port.toString())
            .putString(KEY_TOKEN, target.token.orEmpty())
            .putBoolean(KEY_WEB_ENABLED, target.webEnabled)
            .apply()
        return true
    }

    private fun showConnectionDialog() {
        val palette = themePalette()
        val dialog = android.app.Dialog(this).apply {
            requestWindowFeature(android.view.Window.FEATURE_NO_TITLE)
            window?.setBackgroundDrawable(android.graphics.drawable.ColorDrawable(Color.TRANSPARENT))
        }
        val container = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            background = GradientDrawable().apply {
                setColor(palette.chrome)
                cornerRadius = dp(20).toFloat()
                setStroke(dp(1), palette.chromeStroke)
            }
            setPadding(dp(22), dp(20), dp(22), dp(22))
        }
        val title = TextView(this).apply {
            text = "连接到 Mac"
            setTextColor(palette.label)
            textSize = 20f
            typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL)
        }
        val subtitle = TextView(this).apply {
            text = "选择附近的 Mac，或使用下方的手动地址。"
            setTextColor(palette.secondary)
            textSize = 13f
            setPadding(0, dp(5), 0, dp(16))
        }
        container.addView(title)
        container.addView(subtitle)

        if (sender.target != null || isConnecting) {
            val connectedHost = sender.target?.hostAddress ?: prefs.getString(KEY_HOST, "") ?: ""
            val disconnectBtn = actionSheetButton("断开当前连接 ($connectedHost)", false) {
                disconnectFromMac()
                dialog.dismiss()
                Toast.makeText(this@MainActivity, "已断开与 Mac 的连接", Toast.LENGTH_SHORT).show()
            }.apply {
                setTextColor(0xFFFF453A.toInt()) // Apple Red
            }
            container.addView(disconnectBtn, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(48)).apply {
                bottomMargin = dp(14)
            })
        }

        container.addView(actionSheetButton("扫描二维码", true) {
            startQrScanner(dialog)
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(50)).apply {
            bottomMargin = dp(14)
        })
        container.addView(TextView(this).apply {
            text = "推荐：扫描 Mac 设置页里的二维码，自动带入地址、端口和 Token。"
            setTextColor(palette.secondary)
            textSize = 12f
            setPadding(0, 0, 0, dp(10))
        })

        val nearby = discovery.snapshot()
        if (nearby.isNotEmpty()) {
            container.addView(TextView(this).apply {
                text = "附近的 Mac"
                setTextColor(palette.label)
                textSize = 14f
                typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL)
                setPadding(0, 0, 0, dp(8))
            })
            nearby.forEach { endpoint ->
                val row = Button(this).apply {
                    val transportLabel = if (endpoint.webEnabled) "Web + 手机" else "仅手机 UDP"
                    text = "${endpoint.name}\n${endpoint.host.hostAddress}:${endpoint.port} · $transportLabel"
                    isAllCaps = false
                    gravity = Gravity.START or Gravity.CENTER_VERTICAL
                    minHeight = dp(54)
                    setTextColor(palette.label)
                    background = GradientDrawable().apply {
                        setColor(palette.button)
                        cornerRadius = dp(12).toFloat()
                        setStroke(dp(1), palette.buttonStroke)
                    }
                    setOnClickListener {
                        val token = prefs.getString(KEY_TOKEN, "") ?: ""
                        if (endpoint.authentication == "token" && token.isEmpty()) {
                            Toast.makeText(this@MainActivity, "该 Mac 需要配对 Token，请扫描二维码。", Toast.LENGTH_LONG).show()
                            startQrScanner(dialog)
                        } else {
                            prefs.edit().putBoolean(KEY_WEB_ENABLED, endpoint.webEnabled).apply()
                            connectToMac(endpoint.host.hostAddress ?: "", endpoint.port.toString(), token, endpoint.webEnabled)
                            dialog.dismiss()
                        }
                    }
                }
                container.addView(row, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(58)).apply {
                    bottomMargin = dp(8)
                })
            }
            container.addView(TextView(this).apply {
                text = "找不到 Mac？确认两台设备在同一 Wi-Fi，或继续手动输入。"
                setTextColor(palette.secondary)
                textSize = 12f
                setPadding(0, 0, 0, dp(10))
            })
        } else {
            container.addView(TextView(this).apply {
                text = "正在搜索附近的 Mac；也可以直接手动输入地址。"
                setTextColor(palette.secondary)
                textSize = 12f
                setPadding(0, 0, 0, dp(12))
            })
        }

        fun input(label: String, value: String, password: Boolean = false): EditText {
            val field = EditText(this).apply {
                hint = label
                setText(value)
                setSingleLine()
                textSize = 14f
                setTextColor(palette.label)
                setHintTextColor(palette.secondary)
                setPadding(dp(14), 0, dp(14), 0)
                background = GradientDrawable().apply {
                    setColor(palette.input)
                    cornerRadius = dp(10).toFloat()
                    setStroke(dp(1), palette.buttonStroke)
                }
                if (password) {
                    inputType = android.text.InputType.TYPE_CLASS_TEXT or android.text.InputType.TYPE_TEXT_VARIATION_PASSWORD
                }
            }
            field.contentDescription = label
            container.addView(field, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(50)).apply {
                bottomMargin = dp(10)
            })
            return field
        }

        container.addView(TextView(this).apply {
            text = "IP 连接（备用）"
            setTextColor(palette.label)
            textSize = 14f
            typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL)
            setPadding(0, dp(6), 0, dp(8))
        })
        val host = input("Mac IP 地址或主机名", prefs.getString(KEY_HOST, "") ?: "")
        val port = input("端口（默认 4242）", prefs.getString(KEY_PORT, "4242") ?: "4242").apply {
            inputType = android.text.InputType.TYPE_CLASS_NUMBER
        }
        val token = input("配对 Token（二维码会自动带入）", prefs.getString(KEY_TOKEN, "") ?: "", true)

        val actions = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.END
        }
        val cancel = actionSheetButton("取消", false) { dialog.dismiss() }
        val connect = actionSheetButton("连接", true) {
            val address = host.text.toString().trim()
            if (address.isEmpty()) {
                host.error = "请输入 Mac IP 地址"
                return@actionSheetButton
            }
            connectToMac(address, port.text.toString().trim().ifEmpty { "4242" }, token.text.toString().trim())
            dialog.dismiss()
        }
        actions.addView(cancel, LinearLayout.LayoutParams(0, dp(48), 1f).apply { marginEnd = dp(8) })
        actions.addView(connect, LinearLayout.LayoutParams(0, dp(48), 1f))
        container.addView(actions)

        val scroll = android.widget.ScrollView(this).apply {
            isFillViewport = true
            clipToPadding = false
            overScrollMode = View.OVER_SCROLL_IF_CONTENT_SCROLLS
            addView(container, ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
            ))
        }
        dialog.setContentView(scroll)
        beginModal(dialog)
        dialog.show()
        dialog.window?.setSoftInputMode(android.view.WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
        dialog.window?.setLayout(
            (resources.displayMetrics.widthPixels * 0.92f).toInt(),
            (resources.displayMetrics.heightPixels * 0.82f).toInt(),
        )
    }

    private fun actionSheetButton(label: String, accent: Boolean, onClick: () -> Unit): Button = Button(this).apply {
        val palette = themePalette()
        text = label
        isAllCaps = false
        textSize = 14f
        minHeight = dp(48)
        setTextColor(if (accent) Color.WHITE else palette.label)
        background = GradientDrawable().apply {
            setColor(if (accent) palette.accent else palette.button)
            cornerRadius = dp(InteractionMetrics.BUTTON_RADIUS_DP).toFloat()
            setStroke(dp(1), if (accent) Color.argb(102, Color.red(palette.accent), Color.green(palette.accent), Color.blue(palette.accent)) else palette.buttonStroke)
        }
        elevation = dp(1).toFloat()
        clipToOutline = true
        setOnClickListener { onClick() }
        installPressFeedback(this)
    }

    private fun installPressFeedback(view: View) {
        view.setOnTouchListener { v, event ->
            when (event.actionMasked) {
                android.view.MotionEvent.ACTION_DOWN -> {
                    v.animate().cancel()
                    v.animate()
                        .scaleX(InteractionMetrics.PRESS_SCALE)
                        .scaleY(InteractionMetrics.PRESS_SCALE)
                        .translationY(0.5f)
                        .alpha(InteractionMetrics.PRESS_ALPHA)
                        .setDuration(InteractionMetrics.PRESS_DOWN_MS)
                        .setInterpolator(InteractionMetrics.PRESS_DOWN_INTERPOLATOR)
                        .start()
                }
                android.view.MotionEvent.ACTION_UP, android.view.MotionEvent.ACTION_CANCEL -> {
                    v.animate().cancel()
                    v.animate()
                        .scaleX(1f)
                        .scaleY(1f)
                        .translationY(0f)
                        .alpha(1f)
                        .setDuration(InteractionMetrics.PRESS_UP_MS)
                        .setInterpolator(InteractionMetrics.PRESS_UP_INTERPOLATOR)
                        .start()
                }
            }
            false
        }
    }

    private fun beginModal(dialog: android.app.Dialog) {
        modalDepth += 1
        if (!isFullscreenMode && ::header.isInitialized) {
            header.visibility = View.GONE
        }
        dialog.setOnDismissListener {
            modalDepth = (modalDepth - 1).coerceAtLeast(0)
            if (modalDepth == 0 && !isFullscreenMode && ::header.isInitialized) {
                header.visibility = View.VISIBLE
                setHeaderExpanded(headerExpanded)
            }
        }
    }

    private fun setHeaderExpanded(expanded: Boolean) {
        headerExpanded = expanded
        if (!::header.isInitialized || !::headerToggle.isInitialized) return
        val compact = isConnected && !expanded && !isFullscreenMode
        headerInfo.visibility = View.VISIBLE
        (headerInfo as? LinearLayout)?.let { info ->
            for (index in 0 until info.childCount) {
                info.getChildAt(index).visibility = if (compact && index < info.childCount - 1) View.GONE else View.VISIBLE
            }
        }
        (headerInfo.layoutParams as? LinearLayout.LayoutParams)?.let { infoLp ->
            if (compact) {
                infoLp.width = dp(48)
                infoLp.weight = 0f
                infoLp.marginStart = dp(8)
                infoLp.marginEnd = dp(4)
            } else {
                infoLp.width = 0
                infoLp.weight = 1f
                infoLp.marginStart = dp(10)
                infoLp.marginEnd = dp(10)
            }
            headerInfo.layoutParams = infoLp
        }
        connectButton.visibility = if (compact) View.GONE else View.VISIBLE
        controlsRail.visibility = if (isFullscreenMode) View.GONE else View.VISIBLE
        headerToggle.visibility = if (isFullscreenMode) View.GONE else View.VISIBLE
        headerToggle.text = "⛶"
        headerToggle.contentDescription = "进入全屏触控模式"
        val lp = (header.layoutParams as? FrameLayout.LayoutParams)
            ?: FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(52))
        if (compact) {
            // Status + control centre + fullscreen must fit inside the compact
            // capsule. The previous 190dp width let LinearLayout clip the
            // fullscreen button on high-density phones.
            lp.width = dp(HeaderLayoutMetrics.COMPACT_WIDTH_DP)
            lp.height = dp(44)
            lp.gravity = Gravity.TOP or Gravity.END
            lp.setMargins(0, dp(8), dp(12), 0)
            header.setPadding(dp(8), 0, dp(8), 0)
        } else {
            lp.width = ViewGroup.LayoutParams.MATCH_PARENT
            lp.height = dp(52)
            lp.gravity = Gravity.TOP
            lp.setMargins(dp(12), dp(6), dp(12), 0)
            header.setPadding(dp(18), dp(4), dp(12), dp(4))
        }
        header.layoutParams = lp
        applyPadChromeInsets()
    }

    private fun applyPadChromeInsets() {
        // While fullscreen, connection callbacks may still update the header
        // state. Do not let those callbacks rewrite the surface bounds during
        // an active fullscreen session; the exit transition restores them once.
        if (!::padHost.isInitialized || isFullscreenMode) return
        val lp = (padHost.layoutParams as? FrameLayout.LayoutParams)
            ?: FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
        // Fullscreen hides chrome, but keeps the same centered touch surface.
        // Keeping the outer inset also masks gesture/navigation reserve areas
        // that some Android skins paint as a dark strip at the display edge.
        val top = PadLayoutMetrics.topMargin(isFullscreenMode, isConnected, headerExpanded)
        lp.setMargins(
            dp(PadLayoutMetrics.SIDE_MARGIN_DP),
            dp(top),
            dp(PadLayoutMetrics.SIDE_MARGIN_DP),
            dp(PadLayoutMetrics.BOTTOM_MARGIN_DP),
        )
        padHost.layoutParams = lp
    }

    private fun toggleFullscreen(fullscreen: Boolean) {
        if (isFullscreenMode == fullscreen) return
        isFullscreenMode = fullscreen
        val motion = if (fullscreen) {
            InteractionMetrics.FULLSCREEN_ENTER_INTERPOLATOR
        } else {
            InteractionMetrics.FULLSCREEN_EXIT_INTERPOLATOR
        }
        val duration = if (fullscreen) {
            InteractionMetrics.FULLSCREEN_ENTER_MS
        } else {
            InteractionMetrics.FULLSCREEN_EXIT_MS
        }
        val targetOpacity = surfaceOpacityFraction()
        header.animate().cancel()
        fullscreenFloatBtn.animate().cancel()
        padHost.animate().cancel()
        if (fullscreen) {
            header.visibility = View.VISIBLE
            header.alpha = 1f
            header.translationY = 0f
            header.animate()
                .alpha(0f)
                .translationY(-dp(12).toFloat())
                .setDuration(170L)
                .setInterpolator(motion)
                .withEndAction { if (isFullscreenMode) header.visibility = View.GONE }
                .start()
            fullscreenFloatBtn.visibility = View.VISIBLE
            fullscreenFloatBtn.alpha = 0f
            fullscreenFloatBtn.translationY = -dp(10).toFloat()
            fullscreenFloatBtn.scaleX = .92f
            fullscreenFloatBtn.scaleY = .92f
            fullscreenFloatBtn.pivotX = fullscreenFloatBtn.width.toFloat()
            fullscreenFloatBtn.pivotY = 0f
            fullscreenFloatBtn.animate()
                .alpha(.86f)
                .translationY(0f)
                .scaleX(1f)
                .scaleY(1f)
                .setDuration(duration)
                .setInterpolator(motion)
                .start()
        } else {
            header.visibility = View.VISIBLE
            header.alpha = 0f
            header.translationY = -dp(12).toFloat()
            header.animate()
                .alpha(1f)
                .translationY(0f)
                .setDuration(duration)
                .setInterpolator(motion)
                .start()
            fullscreenFloatBtn.pivotX = fullscreenFloatBtn.width.toFloat()
            fullscreenFloatBtn.pivotY = 0f
            fullscreenFloatBtn.animate()
                .alpha(0f)
                .translationY(-dp(10).toFloat())
                .scaleX(.92f)
                .scaleY(.92f)
                .setDuration(duration)
                .setInterpolator(motion)
                .withEndAction { if (!isFullscreenMode) fullscreenFloatBtn.visibility = View.GONE }
                .start()
        }
        if (::padHost.isInitialized) {
            val lp = (padHost.layoutParams as? FrameLayout.LayoutParams)
                ?: FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
            if (fullscreen) {
                // Fullscreen is a chrome mode, not a surface mode. Preserve
                // the exact bounds that were visible before the transition;
                // changing layout margins here makes the lens jump or appear
                // to disappear while the header is fading out.
                padHost.layoutParams = lp
                if (padHost is LiquidGlassView) {
                    (padHost as LiquidGlassView).cornerRadius = dp(30).toFloat()
                    (padHost as LiquidGlassView).enableEdgeHighlight = true
                }
                gpuGlassView?.setFullscreen(false)
                padFrame.setPadding(dp(8), dp(8), dp(8), dp(8))
                padFrame.background = normalPadFrameBackground
            } else {
                applyPadChromeInsets()
                padHost.layoutParams = lp
                if (padHost is LiquidGlassView) {
                    (padHost as LiquidGlassView).cornerRadius = dp(30).toFloat()
                    (padHost as LiquidGlassView).enableEdgeHighlight = true
                }
                gpuGlassView?.setFullscreen(false)
                padFrame.setPadding(dp(8), dp(8), dp(8), dp(8))
                padFrame.background = normalPadFrameBackground
            }
            // Keep the user's chosen material opacity stable. A temporary
            // alpha dip reads as a flash on bright wallpapers; the geometry
            // settle alone gives the transition enough acknowledgement.
            padHost.alpha = targetOpacity
            padHost.scaleX = if (fullscreen) {
                InteractionMetrics.FULLSCREEN_ENTER_SCALE
            } else {
                InteractionMetrics.FULLSCREEN_EXIT_SCALE
            }
            padHost.scaleY = padHost.scaleX
            padHost.animate()
                .alpha(targetOpacity)
                .scaleX(1f)
                .scaleY(1f)
                .setDuration(duration)
                .setInterpolator(motion)
                .start()
        }
        immersive()
        window.decorView.requestApplyInsets()
        excludeSystemGestures(pad)
    }

    private fun showGestureTestDialog() {
        val dialog = android.app.Dialog(this).apply {
            requestWindowFeature(android.view.Window.FEATURE_NO_TITLE)
            window?.setBackgroundDrawable(android.graphics.drawable.ColorDrawable(Color.TRANSPARENT))
        }
        val palette = themePalette()
        val scroll = android.widget.ScrollView(this)
        val container = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            val bg = GradientDrawable().apply {
                setColor(palette.chrome)
                cornerRadius = dp(16).toFloat()
                setStroke(dp(1), palette.chromeStroke)
            }
            background = bg
            setPadding(dp(20), dp(18), dp(20), dp(20))
        }
        val title = TextView(this).apply {
            text = "🛠 macOS 手势命令发射面板"
            setTextColor(palette.label)
            textSize = 16f
            typeface = android.graphics.Typeface.DEFAULT_BOLD
            gravity = Gravity.CENTER_HORIZONTAL
        }
        val subtitle = TextView(this).apply {
            text = "点击按钮直接模拟真实指尖轨迹派发至 Mac\n请将 Mac 鼠标先悬停在 Safari / 目标窗口上"
            setTextColor(palette.secondary)
            textSize = 11f
            gravity = Gravity.CENTER_HORIZONTAL
            val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                setMargins(0, dp(4), 0, dp(14))
            }
            layoutParams = lp
        }
        container.addView(title)
        container.addView(subtitle)
        fun makeTestBtn(name: String, desc: String, _color: Int, action: () -> Unit): View {
            val card = LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                val bg = GradientDrawable().apply {
                    setColor(palette.button)
                    cornerRadius = dp(10).toFloat()
                    setStroke(dp(1), palette.buttonStroke)
                }
                background = bg
                setPadding(dp(12), dp(8), dp(12), dp(8))
                val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                    setMargins(0, dp(4), 0, dp(4))
                }
                layoutParams = lp
                isClickable = true
                isFocusable = true
                setOnClickListener {
                    action()
                    Toast.makeText(this@MainActivity, "已发射：$name", Toast.LENGTH_SHORT).show()
                }
            }
            card.addView(TextView(this).apply {
                text = name
                setTextColor(palette.label)
                textSize = 13f
                typeface = android.graphics.Typeface.DEFAULT_BOLD
            })
            card.addView(TextView(this).apply {
                text = desc
                setTextColor(palette.secondary)
                textSize = 10f
            })
            return card
        }
        container.addView(makeTestBtn("🔍 智能缩放 (Smart Zoom)", "双指双击：Safari/预览 网页段落或图片平滑放大居中", 0xFF1E3A8A.toInt()) { GestureTestRunner.runSmartZoom(sender) })
        container.addView(makeTestBtn("📖 三指查词 (Look Up)", "三指轻点：弹出 macOS 系统词典释义气泡", 0xFF065F46.toInt()) { GestureTestRunner.runLookup(sender) })
        container.addView(makeTestBtn("◀️ 四指左轻扫 (切到右侧桌面)", "四指水平左扫：切换到下一个 Spaces 桌面", 0xFF3730A3.toInt()) { GestureTestRunner.runSwipeLeft(sender) })
        container.addView(makeTestBtn("▶️ 四指右轻扫 (切到左侧桌面)", "四指水平右扫：切换到上一个 Spaces 桌面", 0xFF3730A3.toInt()) { GestureTestRunner.runSwipeRight(sender) })
        container.addView(makeTestBtn("🔼 四指上滑 (调度中心)", "四指垂直上推：展开 macOS Mission Control", 0xFF6B21A8.toInt()) { GestureTestRunner.runSwipeUp(sender) })
        container.addView(makeTestBtn("🔽 四指下滑 (应用程序窗口)", "四指垂直下拉：展开 App Exposé 当前应用多窗口", 0xFF6B21A8.toInt()) { GestureTestRunner.runSwipeDown(sender) })
        container.addView(makeTestBtn("🔍➕ 双指捏合放大 (+30%)", "双指向外扩张：Safari/地图/文档 视口无级缩放", 0xFF831843.toInt()) { GestureTestRunner.runPinchIn(sender) })
        container.addView(makeTestBtn("🔍➖ 双指捏合缩小 (-30%)", "双指向内聚拢：Safari/地图/文档 视口缩小", 0xFF831843.toInt()) { GestureTestRunner.runPinchOut(sender) })
        container.addView(makeTestBtn("🔄 双指顺时针旋转 90°", "双指圆周旋转：在照片/预览中旋转图片", 0xFF92400E.toInt()) { GestureTestRunner.runRotate(sender) })
        container.addView(makeTestBtn("🖱 双指右键点击", "双指轻点：弹出光标所在处的系统右键上下文菜单", 0xFF1F2937.toInt()) { GestureTestRunner.runRightClick(sender) })
        container.addView(makeTestBtn("✋ 三指拖移测试", "三指接触并平移：选中文本或拖动窗口标题栏", 0xFF1F2937.toInt()) { GestureTestRunner.runThreeFingerDrag(sender) })
        container.addView(makeTestBtn("📬 通知中心 (Notification Center)", "双指从右边缘向左滑入：打开/关闭 macOS 系统通知中心", 0xFF0C4A6E.toInt()) { GestureTestRunner.runNotificationCenter(sender) })
        container.addView(makeTestBtn("🚀 启动台 (Launchpad)", "四指向内捏合：展开 macOS Launchpad 应用程序网格", 0xFF047857.toInt()) { GestureTestRunner.runLaunchpadPinch(sender) })
        container.addView(makeTestBtn("🖥️ 显示桌面 (Show Desktop)", "四指向外张开：推开所有应用窗口显示纯净桌面", 0xFF0369A1.toInt()) { GestureTestRunner.runShowDesktopSpread(sender) })
        container.addView(makeTestBtn("✊ 软件长按拖拽 (Press-and-Hold Drag)", "单指原地按住450ms扣住左键并拖拽选中，抬手释放", 0xFFB45309.toInt()) { GestureTestRunner.runPressAndHoldDrag(sender) })
        val closeBtn = actionSheetButton("关闭面板", false) { dialog.dismiss() }.apply {
            layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(48)).apply { topMargin = dp(12) }
        }
        container.addView(closeBtn)
        scroll.addView(container)
        dialog.setContentView(scroll)
        dialog.window?.setLayout((resources.displayMetrics.widthPixels * 0.90).toInt(), ViewGroup.LayoutParams.WRAP_CONTENT)
        beginModal(dialog)
        dialog.show()
    }

    private fun showDeepPressSettingsDialog() {
        val palette = themePalette()
        val dialog = android.app.Dialog(this).apply {
            requestWindowFeature(android.view.Window.FEATURE_NO_TITLE)
            window?.setBackgroundDrawable(android.graphics.drawable.ColorDrawable(Color.TRANSPARENT))
        }
        val container = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            background = GradientDrawable().apply {
                setColor(palette.chrome)
                cornerRadius = dp(16).toFloat()
                setStroke(dp(1), palette.chromeStroke)
            }
            setPadding(dp(20), dp(18), dp(20), dp(20))
        }
        val title = TextView(this).apply {
            text = "深按条设置"
            setTextColor(palette.label)
            textSize = 16f
            typeface = android.graphics.Typeface.DEFAULT_BOLD
            gravity = Gravity.CENTER_HORIZONTAL
        }
        container.addView(title)

        val enabled = android.widget.CheckBox(this).apply {
            text = "显示深按条"
            isChecked = prefs.getBoolean(KEY_DEEP_ENABLED, true)
            setTextColor(palette.label)
        }
        container.addView(enabled)

        var refreshPreview: () -> Unit = {}
        fun addSeekBar(
            label: String,
            min: Int,
            max: Int,
            initial: Int,
            suffix: String,
        ): android.widget.SeekBar {
            val value = TextView(this).apply {
                setTextColor(palette.secondary)
                textSize = 12f
            }
            val titleRow = LinearLayout(this).apply {
                orientation = LinearLayout.HORIZONTAL
                val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                layoutParams = lp
                addView(TextView(this@MainActivity).apply {
                    text = label
                    setTextColor(palette.label)
                    textSize = 12f
                    layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
                })
                addView(value)
            }
            container.addView(titleRow)
            val seek = android.widget.SeekBar(this).apply {
                this.max = (max - min).coerceAtLeast(1)
                progress = (initial - min).coerceIn(0, this.max)
                setOnSeekBarChangeListener(object : android.widget.SeekBar.OnSeekBarChangeListener {
                    override fun onProgressChanged(seekBar: android.widget.SeekBar?, progress: Int, fromUser: Boolean) {
                        value.text = "${progress + min}$suffix"
                        refreshPreview()
                    }
                    override fun onStartTrackingTouch(seekBar: android.widget.SeekBar?) = Unit
                    override fun onStopTrackingTouch(seekBar: android.widget.SeekBar?) = Unit
                })
            }
            value.text = "${initial.coerceIn(min, max)}$suffix"
            container.addView(seek)
            return seek
        }

        val hold = addSeekBar(
            "确认时长",
            200,
            2_000,
            prefs.getLong(KEY_DEEP_HOLD_MS, DEFAULT_DEEP_HOLD_MS).toInt(),
            " ms",
        )
        val hapticStrength = addSeekBar(
            "按下震动强度",
            40,
            255,
            prefs.getInt(KEY_DEEP_HAPTIC_STRENGTH, DEFAULT_DEEP_HAPTIC_STRENGTH),
            " / 255",
        )
        val x = addSeekBar("横向位置", 0, 100, (prefs.getFloat(KEY_DEEP_X, DEFAULT_DEEP_X) * 100f).toInt(), "%")
        val y = addSeekBar("纵向位置", 0, 100, (prefs.getFloat(KEY_DEEP_Y, DEFAULT_DEEP_Y) * 100f).toInt(), "%")
        val width = addSeekBar("宽度", 140, 520, prefs.getInt(KEY_DEEP_WIDTH, DEFAULT_DEEP_WIDTH), " dp")
        val height = addSeekBar("高度", 36, 120, prefs.getInt(KEY_DEEP_HEIGHT, DEFAULT_DEEP_HEIGHT), " dp")

        container.addView(TextView(this).apply {
            text = "直接拖动预览调整位置，拖右下角调整大小"
            setTextColor(palette.secondary)
            textSize = 11f
            setPadding(dp(4), dp(10), dp(4), dp(5))
        })
        val preview = FrameLayout(this).apply {
            background = GradientDrawable().apply {
                setColor(Color.argb(32, Color.red(palette.accent), Color.green(palette.accent), Color.blue(palette.accent)))
                cornerRadius = dp(12).toFloat()
                setStroke(dp(1), palette.buttonStroke)
            }
        }
        val previewButton = Button(this).apply {
            text = "深按"
            isAllCaps = false
            textSize = 12f
            setTextColor(palette.deepText)
            background = GradientDrawable().apply {
                setColor(palette.deep)
                cornerRadius = dp(10).toFloat()
                setStroke(dp(1), palette.deepStroke)
            }
            elevation = dp(2).toFloat()
            contentDescription = "深按条预览，可拖动"
        }
        val resizeHandle = View(this).apply {
            background = GradientDrawable().apply {
                setColor(palette.deepProgress)
                cornerRadius = dp(2).toFloat()
            }
            contentDescription = "拖动此角调整深按条大小"
        }
        preview.addView(previewButton)
        preview.addView(resizeHandle, FrameLayout.LayoutParams(dp(12), dp(12), Gravity.BOTTOM or Gravity.END))
        fun refreshPreviewGeometry() {
            if (preview.width <= 0 || preview.height <= 0) return
            val previewWidth = (width.progress + 140).coerceIn(140, 520)
            val previewHeight = (height.progress + 36).coerceIn(36, 120)
            val pxWidth = dp(previewWidth).coerceAtMost(preview.width)
            val pxHeight = dp(previewHeight).coerceAtMost(preview.height)
            val left = (preview.width * (x.progress / 100f) - pxWidth / 2f).toInt().coerceIn(0, (preview.width - pxWidth).coerceAtLeast(0))
            val top = (preview.height * (y.progress / 100f) - pxHeight / 2f).toInt().coerceIn(0, (preview.height - pxHeight).coerceAtLeast(0))
            previewButton.layoutParams = FrameLayout.LayoutParams(pxWidth, pxHeight).apply { leftMargin = left; topMargin = top }
            resizeHandle.layoutParams = FrameLayout.LayoutParams(dp(12), dp(12), Gravity.TOP or Gravity.START).apply {
                leftMargin = (left + pxWidth - dp(17)).coerceAtLeast(left)
                topMargin = (top + pxHeight - dp(17)).coerceAtLeast(top)
            }
        }
        refreshPreview = { refreshPreviewGeometry() }
        var dragStartX = 0f
        var dragStartY = 0f
        previewButton.setOnTouchListener { _, event ->
            when (event.actionMasked) {
                android.view.MotionEvent.ACTION_DOWN -> {
                    dragStartX = event.rawX
                    dragStartY = event.rawY
                    true
                }
                android.view.MotionEvent.ACTION_MOVE -> {
                    if (preview.width > 0 && preview.height > 0) {
                        val dx = event.rawX - dragStartX
                        val dy = event.rawY - dragStartY
                        val nextX = ((x.progress / 100f * preview.width + dx) / preview.width * 100f).toInt().coerceIn(0, 100)
                        val nextY = ((y.progress / 100f * preview.height + dy) / preview.height * 100f).toInt().coerceIn(0, 100)
                        x.progress = nextX
                        y.progress = nextY
                        dragStartX = event.rawX
                        dragStartY = event.rawY
                    }
                    true
                }
                android.view.MotionEvent.ACTION_UP, android.view.MotionEvent.ACTION_CANCEL -> true
                else -> false
            }
        }
        var resizeStartX = 0f
        var resizeStartY = 0f
        var resizeStartW = 0
        var resizeStartH = 0
        resizeHandle.setOnTouchListener { _, event ->
            when (event.actionMasked) {
                android.view.MotionEvent.ACTION_DOWN -> {
                    resizeStartX = event.rawX
                    resizeStartY = event.rawY
                    resizeStartW = width.progress + 140
                    resizeStartH = height.progress + 36
                    true
                }
                android.view.MotionEvent.ACTION_MOVE -> {
                    val nextW = resizeStartW + ((event.rawX - resizeStartX) / resources.displayMetrics.density).toInt()
                    val nextH = resizeStartH + ((event.rawY - resizeStartY) / resources.displayMetrics.density).toInt()
                    width.progress = (nextW - 140).coerceIn(0, width.max)
                    height.progress = (nextH - 36).coerceIn(0, height.max)
                    true
                }
                android.view.MotionEvent.ACTION_UP, android.view.MotionEvent.ACTION_CANCEL -> true
                else -> false
            }
        }
        preview.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> refreshPreviewGeometry() }
        container.addView(preview, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(150)).apply { bottomMargin = dp(4) })

        val save = actionSheetButton("保存", true) {
                prefs.edit()
                    .putBoolean(KEY_DEEP_ENABLED, enabled.isChecked)
                    .putLong(KEY_DEEP_HOLD_MS, (hold.progress + 200).toLong())
                    .putInt(KEY_DEEP_HAPTIC_STRENGTH, hapticStrength.progress + 40)
                    .putFloat(KEY_DEEP_X, (x.progress / 100f).coerceIn(0f, 1f))
                    .putFloat(KEY_DEEP_Y, (y.progress / 100f).coerceIn(0f, 1f))
                    .putInt(KEY_DEEP_WIDTH, width.progress + 140)
                    .putInt(KEY_DEEP_HEIGHT, height.progress + 36)
                    .apply()
                deepPressBar.holdDurationMs = hold.progress.toLong() + 200L
                pad.haptics.deepPressStrength = hapticStrength.progress + 40
                deepPressBar.visibility = if (enabled.isChecked) View.VISIBLE else View.GONE
                layoutDeepPressBar()
                dialog.dismiss()
            }.apply {
                layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(48)).apply {
                    topMargin = dp(14)
                }
            }
        container.addView(save)
        val scroll = android.widget.ScrollView(this).apply {
            isFillViewport = true
            addView(container)
        }
        dialog.setContentView(scroll)
        beginModal(dialog)
        dialog.show()
        dialog.window?.setLayout((resources.displayMetrics.widthPixels * 0.88).toInt(), (resources.displayMetrics.heightPixels * 0.88).toInt())
        preview.post { refreshPreviewGeometry() }
    }

    private fun layoutDeepPressBar() {
        if (!::padFrame.isInitialized || !::deepPressBar.isInitialized) return
        val width = dp(prefs.getInt(KEY_DEEP_WIDTH, DEFAULT_DEEP_WIDTH))
        val height = dp(prefs.getInt(KEY_DEEP_HEIGHT, DEFAULT_DEEP_HEIGHT))
        val x = prefs.getFloat(KEY_DEEP_X, DEFAULT_DEEP_X).coerceIn(0f, 1f)
        val y = prefs.getFloat(KEY_DEEP_Y, DEFAULT_DEEP_Y).coerceIn(0f, 1f)
        val availableX = (padFrame.width - width).coerceAtLeast(0)
        val availableY = (padFrame.height - height).coerceAtLeast(0)
        val lp = (deepPressBar.layoutParams as? FrameLayout.LayoutParams)
            ?: FrameLayout.LayoutParams(width, height)
        // X/Y are stored as the bar center position, so 50% is centered
        // regardless of the chosen bar dimensions. Avoid writing identical
        // LayoutParams during a layout callback, which would recurse forever.
        val left = (padFrame.width * x - width / 2f).toInt().coerceIn(0, availableX)
        val top = (padFrame.height * y - height / 2f).toInt().coerceIn(0, availableY)
        if (lp.width != width || lp.height != height || lp.leftMargin != left || lp.topMargin != top) {
            lp.width = width
            lp.height = height
            lp.leftMargin = left
            lp.topMargin = top
            deepPressBar.layoutParams = lp
        }
    }

    private fun sendVirtualButton(pressed: Boolean) {
        if (!::sender.isInitialized) return
        sender.send(
            FrameEncoder.encode(
                seq = sender.nextSeq(),
                scanTimeTicks = sender.nowTicks(),
                button = pressed,
                contacts = emptyList(),
            ),
        )
    }

    private fun setStatus(connected: Boolean, msg: String) {
        isConnected = connected
        isConnecting = msg == "连接中…"
        status.text = msg
        val d = GradientDrawable().apply {
            shape = GradientDrawable.OVAL
            setColor(if (connected) 0xFF3DDC84.toInt() else Color.GRAY)
        }
        dot.background = d
        if (::connectButton.isInitialized) {
            connectButton.text = if (isConnecting) "取消连接" else "连接 Mac"
            connectButton.contentDescription = if (isConnecting) "取消连接 Mac" else "配置并连接 Mac"
        }
        setHeaderExpanded(!connected)
    }

    /** Sticky fullscreen + hide nav so system edge gestures stop eating touches. */
    private fun immersive() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            window.attributes = window.attributes.apply {
                layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
            }
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            window.setDecorFitsSystemWindows(false)
            window.insetsController?.let { controller ->
                controller.hide(android.view.WindowInsets.Type.statusBars() or android.view.WindowInsets.Type.navigationBars())
                controller.systemBarsBehavior = android.view.WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            }
        }
        @Suppress("DEPRECATION")
        window.decorView.systemUiVisibility =
            View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or
            View.SYSTEM_UI_FLAG_FULLSCREEN or
            View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
            View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
            View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
            View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
    }

    /** API 29+: claim the whole pad as gesture-exclusion zone so edge back gestures don't trigger. */
    private fun excludeSystemGestures(root: View) {
        root.post {
            if (Build.VERSION.SDK_INT >= 29 && root.width > 0 && root.height > 0) {
                try {
                    root.systemGestureExclusionRects =
                        listOf(Rect(0, 0, root.width, root.height))
                } catch (_: Exception) {}
            }
        }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) immersive()
    }

    @Deprecated("Use OnBackInvokedDispatcher on API 33+; kept for API 26-32 compatibility")
    @Suppress("DEPRECATION")
    override fun onBackPressed() {
        // Back is an exit affordance for immersive touch mode. Users should
        // not lose the active pairing just because they want the chrome back.
        if (isFullscreenMode) {
            toggleFullscreen(false)
            return
        }
        if (isConnecting) {
            cancelMacConnection()
            return
        }
        super.onBackPressed()
    }

    override fun onNewIntent(intent: Intent?) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (applyPairingIntent(intent)) {
            if (::pad.isInitialized) {
                connectToMac(
                    prefs.getString(KEY_HOST, "") ?: "",
                    prefs.getString(KEY_PORT, "4242") ?: "4242",
                    prefs.getString(KEY_TOKEN, "") ?: "",
                    prefs.getBoolean(KEY_WEB_ENABLED, true),
                )
            }
            Toast.makeText(this, "已载入配对信息", Toast.LENGTH_SHORT).show()
        }
    }

    @Deprecated("Use Activity Result APIs when the app adopts AndroidX")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == REQUEST_QR_SCANNER) {
            if (resultCode != RESULT_OK) {
                showConnectionDialog()
                return
            }
            val target = PairingUri.parse(data?.getStringExtra(QrScannerActivity.EXTRA_QR_VALUE))
            if (target == null) {
                Toast.makeText(this, "这不是有效的 Trackpad Companion 配对二维码。", Toast.LENGTH_LONG).show()
                showConnectionDialog()
                return
            }
            if (!applyPairingTarget(target)) {
                showConnectionDialog()
                return
            }
            connectToMac(
                target.host,
                target.port.toString(),
                target.token.orEmpty(),
                target.webEnabled,
            )
            return
        }
        if (requestCode != REQUEST_WALLPAPER || resultCode != RESULT_OK) return
        val uri = data?.data ?: return
        runCatching {
            contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        prefs.edit()
            .putString(KEY_WALLPAPER_URI, uri.toString())
            .putString(KEY_WALLPAPER_PRESET, "custom")
            .apply()
        recreate()
    }

    private fun applyPairingIntent(intent: Intent?): Boolean {
        val target = PairingUri.parse(intent?.dataString) ?: return false
        return applyPairingTarget(target)
    }

    override fun onDestroy() {
        if (::deepPressBar.isInitialized) {
            deepPressBar.cancelPress()
            pad.removeCallbacks(deepButtonHeartbeat)
        }
        sender.close()
        if (::discovery.isInitialized) discovery.stop()
        wallpaperBitmap?.let { bitmap ->
            if (!bitmap.isRecycled) bitmap.recycle()
        }
        wallpaperBitmap = null
        super.onDestroy()
    }

    private fun dp(v: Int): Int = (v * resources.displayMetrics.density).toInt()

    private fun surfaceOpacityFraction(): Float =
        (prefs.getInt(KEY_SURFACE_OPACITY, DEFAULT_SURFACE_OPACITY).coerceIn(55, 100) / 100f)

    companion object {
        private const val KEY_HOST = "host"
        private const val KEY_PORT = "port"
        private const val KEY_TOKEN = "token"
        private const val KEY_WEB_ENABLED = "web_enabled"
        private const val KEY_THEME = "theme"
        private const val KEY_SCALE = "scale"
        private const val KEY_HAPTIC = "haptic"
        private const val KEY_TOUCH_POINTS = "touch_points"
        @Deprecated("Migrated to KEY_TOUCH_POINTS")
        private const val KEY_VISUAL_EFFECTS = "visual_effects"
        private const val KEY_GLASS_REFRACTION = "custom_glass_refraction"
        private const val KEY_GLASS_SATURATION = "custom_glass_saturation"
        private const val KEY_GLASS_BLUR = "custom_glass_blur"
        private const val KEY_GLASS_HIGHLIGHT = "custom_glass_highlight"
        private const val KEY_DEEP_ENABLED = "deep_press_enabled"
        private const val KEY_DEEP_HOLD_MS = "deep_press_hold_ms"
        private const val KEY_DEEP_HAPTIC_STRENGTH = "deep_press_haptic_strength"
        private const val KEY_DEEP_X = "deep_press_x"
        private const val KEY_DEEP_Y = "deep_press_y"
        private const val KEY_DEEP_WIDTH = "deep_press_width"
        private const val KEY_DEEP_HEIGHT = "deep_press_height"
        private const val KEY_WALLPAPER_PRESET = "wallpaper_preset"
        private const val KEY_WALLPAPER_URI = "wallpaper_uri"
        private const val KEY_WALLPAPER_OPACITY = "wallpaper_opacity"
        private const val KEY_WALLPAPER_SATURATION = "wallpaper_saturation"
        private const val KEY_WALLPAPER_BRIGHTNESS = "wallpaper_brightness"
        private const val KEY_SURFACE_OPACITY = "surface_opacity"
        private const val REQUEST_WALLPAPER = 4201
        private const val REQUEST_QR_SCANNER = 4202
        private const val DEFAULT_WALLPAPER_OPACITY = 100
        private const val DEFAULT_WALLPAPER_SATURATION = 100
        private const val DEFAULT_WALLPAPER_BRIGHTNESS = 100
        private const val DEFAULT_SURFACE_OPACITY = 92
        private const val DEFAULT_DEEP_HOLD_MS = 650L
        private const val DEFAULT_DEEP_HAPTIC_STRENGTH = 255
        private const val DEFAULT_DEEP_X = 0.5f
        private const val DEFAULT_DEEP_Y = 0.82f
        private const val DEFAULT_DEEP_WIDTH = 240
        private const val DEFAULT_DEEP_HEIGHT = 52
        private const val DEEP_BUTTON_HEARTBEAT_MS = 100L
    }
}
