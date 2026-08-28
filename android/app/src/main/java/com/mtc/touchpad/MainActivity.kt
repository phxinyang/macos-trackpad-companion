package com.mtc.touchpad

import android.app.Activity
import android.content.Intent
import android.content.SharedPreferences
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.RadialGradient
import android.graphics.Rect
import android.graphics.Shader
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.widget.Button
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import com.example.liquidglass.GlassAccessibilityMode
import com.example.liquidglass.GlassMaterial
import com.example.liquidglass.LiquidGlassView

private enum class ThemeMode(val key: String, val title: String, val detail: String) {
    LIGHT_GLASS("light-glass", "浅色玻璃", "明亮、通透，默认外观"),
    DARK_GLASS("dark-glass", "深色玻璃", "深色背景与半透明控制层"),
    CLASSIC_LIGHT("classic-light", "经典浅色", "纯色表面，关闭玻璃层"),
    CLASSIC_DARK("classic-dark", "经典深色", "纯色深色，低干扰"),
    HIGH_CONTRAST("high-contrast", "高对比", "黑白边界，优先可读性");

    companion object {
        fun from(key: String?): ThemeMode = values().firstOrNull { it.key == key } ?: LIGHT_GLASS
    }
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
)

private fun paletteFor(mode: ThemeMode): ThemePalette = when (mode) {
    ThemeMode.LIGHT_GLASS -> ThemePalette(
        canvas = 0xFFF4F5F7.toInt(), chrome = 0xE8FFFFFF.toInt(), chromeStroke = 0x66FFFFFF,
        button = 0xBFFFFFFF.toInt(), buttonStroke = 0x3D8C929B, input = 0xF0FFFFFF.toInt(),
        pad = 0xFFFFFFFF.toInt(), padStroke = 0xFFDDE2EA.toInt(), label = 0xFF1D1D1F.toInt(),
        secondary = 0xFF6E6E73.toInt(), accent = 0xFF007AFF.toInt(), success = 0xFF1E9E5A.toInt(),
        warning = 0xFFB86A00.toInt(), danger = 0xFFC9342F.toInt(), deep = 0xFFE2ECFA.toInt(),
        deepProgress = 0xFF007AFF.toInt(), deepStroke = 0xFF6D9FE8.toInt(), deepText = 0xFF12345C.toInt(),
        usesLiquidGlass = true,
    )
    ThemeMode.DARK_GLASS -> ThemePalette(
        canvas = 0xFF0B0D12.toInt(), chrome = 0xC51B1F29.toInt(), chromeStroke = 0x35FFFFFF,
        button = 0xB5222833.toInt(), buttonStroke = 0x2CFFFFFF, input = 0xFF252A34.toInt(),
        pad = 0xFF151923.toInt(), padStroke = 0x3DFFFFFF, label = 0xFFF5F7FB.toInt(),
        secondary = 0xFFAEB6C5.toInt(), accent = 0xFF0A84FF.toInt(), success = 0xFF30D158.toInt(),
        warning = 0xFFFF9F0A.toInt(), danger = 0xFFFF453A.toInt(), deep = 0xFF24242E.toInt(),
        deepProgress = 0xFF0A84FF.toInt(), deepStroke = 0x66FFFFFF, deepText = 0xFFFFFFFF.toInt(),
        usesLiquidGlass = true,
    )
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
        usesLiquidGlass = false,
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
    private lateinit var deepPressBar: DeepPressBarView
    private lateinit var fullscreenFloatBtn: Button
    private lateinit var discovery: MacDiscovery
    private var discoveredEndpoints: List<MacDiscovery.MacEndpoint> = emptyList()
    private var isFullscreenMode = false

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
        prefs = getSharedPreferences("touchpad", MODE_PRIVATE)
        applyPairingIntent(intent)
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

        pad = TouchPadView(this).also {
            it.sender = sender
            it.scale = prefs.getFloat(KEY_SCALE, 1f)
            it.haptics.deepPressStrength = prefs.getInt(KEY_DEEP_HAPTIC_STRENGTH, DEFAULT_DEEP_HAPTIC_STRENGTH)
        }

        padFrame = FrameLayout(this)
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

        fun installPressFeedback(view: View) {
            view.setOnTouchListener { v, event ->
                when (event.actionMasked) {
                    android.view.MotionEvent.ACTION_DOWN -> v.animate().scaleX(0.97f).scaleY(0.97f).setDuration(90L).start()
                    android.view.MotionEvent.ACTION_UP, android.view.MotionEvent.ACTION_CANCEL ->
                        v.animate().scaleX(1f).scaleY(1f).setDuration(130L).start()
                }
                false
            }
        }

        fun actionButton(label: String, accent: Boolean = false, onClick: () -> Unit): Button = Button(this).apply {
            text = label
            isAllCaps = false
            setTextColor(if (accent) Color.WHITE else palette.label)
            textSize = 12f
            minHeight = dp(44)
            minimumHeight = dp(44)
            minWidth = dp(44)
            setPadding(dp(14), 0, dp(14), 0)
            background = surface(if (accent) palette.accent else palette.button, 12, if (accent) 0x660A84FF else palette.buttonStroke)
            stateListAnimator = null
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
            setTextColor(palette.label)
            textSize = 11f
            typeface = android.graphics.Typeface.create("sans-serif", android.graphics.Typeface.NORMAL)
            maxLines = 1
        }

        val headerBg = surface(palette.chrome, 16, palette.chromeStroke)
        header = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = if (palette.usesLiquidGlass) null else headerBg
            setPadding(dp(16), dp(10), dp(10), dp(10))
            val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                setMargins(dp(12), dp(10), dp(12), dp(8))
            }
            layoutParams = lp
            addView(dot)
            addView(LinearLayout(this@MainActivity).apply {
                orientation = LinearLayout.VERTICAL
                gravity = Gravity.CENTER_VERTICAL
                layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply {
                    marginStart = dp(10)
                    marginEnd = dp(10)
                }
                addView(TextView(this@MainActivity).apply {
                    text = "Mac 触控板"
                    setTextColor(palette.label)
                    textSize = 15f
                    typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL)
                })
                addView(status)
            })
            addView(actionButton("连接", true) { showConnectionDialog() }.apply {
                contentDescription = "配置并连接 Mac"
                layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, dp(44))
            })
        }

        val sensitivity = TextView(this).apply {
            setTextColor(palette.secondary)
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
            setPadding(dp(12), dp(4), dp(12), dp(10))

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

            addView(actionButton("深按") { showDeepPressSettingsDialog() }.apply {
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
            addView(actionButton("主题") { showThemeDialog() }.apply {
                contentDescription = "切换界面主题"
                layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, dp(44)).apply { marginStart = dp(8) }
            })
        }

        val padSurface = surface(palette.pad, 24, palette.padStroke)
        padFrame.background = if (palette.usesLiquidGlass) {
            val dark = ThemeMode.from(prefs.getString(KEY_THEME, ThemeMode.LIGHT_GLASS.key)) == ThemeMode.DARK_GLASS
            GradientDrawable().apply {
                setColor(if (dark) 0x241A2734 else 0x1CFFFFFF)
                cornerRadius = dp(24).toFloat()
                setStroke(dp(1), if (dark) 0x66FFFFFF else 0x80FFFFFF.toInt())
            }
        } else padSurface
        padFrame.setPadding(dp(6), dp(6), dp(6), dp(6))
        val rail = android.widget.HorizontalScrollView(this).apply {
            isHorizontalScrollBarEnabled = false
            overScrollMode = View.OVER_SCROLL_NEVER
            background = if (palette.usesLiquidGlass) null else surface(palette.chrome, 16, palette.chromeStroke)
            addView(controls, ViewGroup.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT))
        }

        fullscreenFloatBtn = actionButton("设置") { toggleFullscreen(false) }.apply {
            alpha = 0.86f
            visibility = View.GONE
            val lp = FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                gravity = Gravity.TOP or Gravity.END
                setMargins(0, dp(18), dp(18), 0)
            }
            layoutParams = lp
            contentDescription = "退出全屏并打开设置"
        }

        val backdropLayer = object : FrameLayout(this) {
            private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
            private var sheenPhase = 0f
            private val sheen = object : Runnable {
                override fun run() {
                    sheenPhase = (sheenPhase + 0.0125f) % 1f
                    invalidate()
                    postDelayed(this, 50L)
                }
            }

            override fun onAttachedToWindow() {
                super.onAttachedToWindow()
                post(sheen)
            }

            override fun onDetachedFromWindow() {
                removeCallbacks(sheen)
                super.onDetachedFromWindow()
            }

            override fun onDraw(canvas: android.graphics.Canvas) {
                val dark = ThemeMode.from(prefs.getString(KEY_THEME, ThemeMode.LIGHT_GLASS.key)) == ThemeMode.DARK_GLASS
                paint.shader = LinearGradient(
                    0f, 0f, width.toFloat(), height.toFloat(),
                    if (dark) intArrayOf(0xFF2A5774.toInt(), 0xFF101821.toInt(), 0xFF243B59.toInt())
                    else intArrayOf(0xFFB9E6FF.toInt(), 0xFFF4F6FA.toInt(), 0xFFFFD4B8.toInt()),
                    null,
                    Shader.TileMode.CLAMP,
                )
                canvas.drawRect(0f, 0f, width.toFloat(), height.toFloat(), paint)
                paint.shader = RadialGradient(width * .13f, height * .20f, width * .40f,
                    if (dark) intArrayOf(0xB04FB6E8.toInt(), 0x0013212D) else intArrayOf(0xA04A9FFF.toInt(), 0x00FFFFFF),
                    null, Shader.TileMode.CLAMP)
                canvas.drawCircle(width * .13f, height * .20f, width * .40f, paint)
                paint.shader = RadialGradient(width * .84f, height * .78f, width * .45f,
                    if (dark) intArrayOf(0xA86D7CFF.toInt(), 0x00101720) else intArrayOf(0xA0FF9A61.toInt(), 0x00FFFFFF),
                    null, Shader.TileMode.CLAMP)
                canvas.drawCircle(width * .84f, height * .78f, width * .45f, paint)
                // Quiet, large geometry gives the lens something to bend. The
                // alpha stays below the foreground content contrast target.
                paint.shader = null
                val drift = (sheenPhase - .5f) * width * .18f
                paint.color = if (dark) 0x7045D6FF else 0x684A90FF
                canvas.drawRoundRect(android.graphics.RectF(width * .06f + drift, height * .30f, width * .30f + drift, height * .78f), dp(30).toFloat(), dp(30).toFloat(), paint)
                paint.color = if (dark) 0x6A9D62FF else 0x62FF9C66
                canvas.drawRoundRect(android.graphics.RectF(width * .70f - drift, height * .12f, width * .92f - drift, height * .52f), dp(28).toFloat(), dp(28).toFloat(), paint)
                paint.color = if (dark) 0x625B8CFF else 0x5A7BC7D8
                canvas.drawCircle(width * .53f, height * .78f, width * .12f, paint)
                paint.color = if (dark) 0x45FFFFFF else 0x55FFFFFF
                for (i in 0..3) {
                    val y = height * (.22f + i * .18f)
                    canvas.drawRoundRect(android.graphics.RectF(width * (.38f + i * .035f), y, width * (.70f + i * .035f), y + dp(3)), dp(2).toFloat(), dp(2).toFloat(), paint)
                }
                // A slow diagonal sheen keeps the sampled backdrop alive, so
                // the refraction layer remains visible even when no fingers
                // are down. It is deliberately low contrast for readability.
                val offset = (sheenPhase * width * 2f) - width
                paint.shader = LinearGradient(offset, 0f, offset + width * .44f, height.toFloat(),
                    if (dark) intArrayOf(0x00101720, 0x453F9ED6, 0x00101720) else intArrayOf(0x00FFFFFF, 0x503A9BFF, 0x00FFFFFF),
                    null, Shader.TileMode.CLAMP)
                canvas.drawRect(0f, 0f, width.toFloat(), height.toFloat(), paint)
                paint.shader = null
                paint.color = if (dark) 0x42FFFFFF else 0x5AFFFFFF
                paint.strokeWidth = dp(1).toFloat()
                for (i in 1..5) canvas.drawLine(0f, height * i / 6f, width.toFloat(), height * i / 6f, paint)
                paint.color = if (dark) 0x3858C7FF else 0x385A86B8
                paint.strokeWidth = dp(2).toFloat()
                val arc = android.graphics.RectF(-width * .12f, height * .34f, width * 1.10f, height * 1.28f)
                canvas.drawArc(arc, 188f, 142f, false, paint)
            }
        }.apply {
            setWillNotDraw(false)
            if (!palette.usesLiquidGlass) background = padSurface
        }
        val contentLayer = FrameLayout(this).apply {
            // Keep the sampled scene full-window. Chrome glass at the top and
            // bottom must see the same continuous backdrop as the pad; if the
            // source stops at the pad bounds, those regions sample transparency
            // and become an opaque-looking black slab on some GPU drivers.
            addView(backdropLayer, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            if (palette.usesLiquidGlass) {
                val padGlass = LiquidGlassView(this@MainActivity).apply {
                    cornerRadius = dp(24).toFloat()
                    material = GlassMaterial.REGULAR
                    useShaderPipeline = true
                    enableDynamicBackground = true
                    enableBackdropBlur = true
                    enableChromaticAberration = true
                    enableChromaticDispersion = true
                    enableEdgeHighlight = true
                    enableSensorHighlight = true
                    enableAdaptiveTint = ThemeMode.from(prefs.getString(KEY_THEME, ThemeMode.LIGHT_GLASS.key)) != ThemeMode.LIGHT_GLASS
                    // Match the library's showcase scale: the large pad needs
                    // a real lens profile, otherwise the rim reads as blur.
                    bevelWidth = 46f
                    refractionHeight = 240f
                    dispersionStrength = 0.20f
                    blurAmount = 0.055f
                    saturation = 150f
                    edgeHighlightOpacity = 86f
                    enablePressEffect = false
                    addView(padFrame, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
                }
                addView(padGlass, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT).apply {
                    setMargins(dp(12), dp(74), dp(12), dp(80))
                })
                padGlass.post { padGlass.backdropSource = backdropLayer }
            } else {
                addView(padFrame, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT).apply {
                    setMargins(dp(12), dp(74), dp(12), dp(80))
                })
            }
        }
        val rootFrame = FrameLayout(this).apply {
            setBackgroundColor(palette.canvas)
            addView(contentLayer, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        }

        fun addChromeSurface(content: View, radius: Int, top: Boolean) {
            // LiquidGlassView measures its child with the incoming parent spec;
            // give both chrome bands a stable height so a wrap-content child
            // cannot expand the glass surface to the entire touch area.
            val lp = FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(if (top) 72 else 72)).apply {
                gravity = if (top) Gravity.TOP else Gravity.BOTTOM
                setMargins(dp(12), if (top) dp(10) else 0, dp(12), if (top) 0 else dp(8))
            }
            if (!palette.usesLiquidGlass) {
                content.layoutParams = lp
                rootFrame.addView(content, lp)
                return
            }
            val glass = LiquidGlassView(this).apply {
                cornerRadius = dp(radius).toFloat()
                material = GlassMaterial.REGULAR
                useShaderPipeline = true
                enableDynamicBackground = true
                enableBackdropBlur = true
                enableChromaticAberration = true
                enableChromaticDispersion = true
                enableEdgeHighlight = true
                enableSensorHighlight = true
                enableAdaptiveTint = ThemeMode.from(prefs.getString(KEY_THEME, ThemeMode.LIGHT_GLASS.key)) != ThemeMode.LIGHT_GLASS
                bevelWidth = if (top) 28f else 24f
                refractionHeight = if (top) 140f else 118f
                dispersionStrength = 0.16f
                blurAmount = 0.055f
                saturation = 145f
                edgeHighlightOpacity = 78f
                enablePressEffect = false
                if (Build.VERSION.SDK_INT < 33) background = surface(palette.chrome, radius, palette.chromeStroke)
                addView(content, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
            }
            rootFrame.addView(glass, lp)
            glass.post { glass.backdropSource = backdropLayer }
        }

        addChromeSurface(header, 16, top = true)
        addChromeSurface(rail, 16, top = false)
        rootFrame.addView(fullscreenFloatBtn)

        setContentView(rootFrame)
        padFrame.post { layoutDeepPressBar() }

        if (!prefs.getString(KEY_HOST, "").isNullOrEmpty()) {
            connectToMac(
                prefs.getString(KEY_HOST, "") ?: "",
                prefs.getString(KEY_PORT, "4242") ?: "4242",
                prefs.getString(KEY_TOKEN, "") ?: "",
            )
        }

        immersive()
        excludeSystemGestures(rootFrame)
    }

    private fun connectToMac(host: String, portText: String, tokenText: String) {
        val port = portText.toIntOrNull()?.coerceIn(1, 65535) ?: 4242
        prefs.edit()
            .putString(KEY_HOST, host)
            .putString(KEY_PORT, port.toString())
            .putString(KEY_TOKEN, tokenText)
            .apply()
        setStatus(false, "连接中…")
        sender.connect(host, port, tokenText.ifEmpty { null }, object : UdpSender.Listener {
            override fun onState(connected: Boolean, message: String) =
                runOnUiThread { setStatus(connected, message) }
        })
    }

    private fun themePalette(): ThemePalette = paletteFor(
        ThemeMode.from(prefs.getString(KEY_THEME, ThemeMode.LIGHT_GLASS.key))
    )

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

        ThemeMode.values().forEach { mode ->
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
                background = GradientDrawable().apply {
                    cornerRadius = dp(9).toFloat()
                    setColor(swatchPalette.chrome)
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
        val cancel = actionSheetButton("取消", false) { dialog.dismiss() }
        container.addView(cancel, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(46)).apply {
            topMargin = dp(4)
        })
        dialog.setContentView(container)
        dialog.show()
        dialog.window?.setLayout((resources.displayMetrics.widthPixels * 0.90f).toInt(), ViewGroup.LayoutParams.WRAP_CONTENT)
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
                    text = "${endpoint.name}\n${endpoint.host.hostAddress}:${endpoint.port}"
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
                            Toast.makeText(this@MainActivity, "该 Mac 需要配对 Token，请使用二维码或手动输入。", Toast.LENGTH_LONG).show()
                        } else {
                            connectToMac(endpoint.host.hostAddress ?: "", endpoint.port.toString(), token)
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

        val host = input("Mac IP 地址", prefs.getString(KEY_HOST, "") ?: "")
        val port = input("端口（默认 4242）", prefs.getString(KEY_PORT, "4242") ?: "4242").apply {
            inputType = android.text.InputType.TYPE_CLASS_NUMBER
        }
        val token = input("Token（可选）", prefs.getString(KEY_TOKEN, "") ?: "", true)

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

        dialog.setContentView(container)
        dialog.show()
        dialog.window?.setLayout((resources.displayMetrics.widthPixels * 0.90f).toInt(), ViewGroup.LayoutParams.WRAP_CONTENT)
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
            cornerRadius = dp(12).toFloat()
            setStroke(dp(1), if (accent) Color.argb(102, Color.red(palette.accent), Color.green(palette.accent), Color.blue(palette.accent)) else palette.buttonStroke)
        }
        setOnClickListener { onClick() }
        setOnTouchListener { v, event ->
            when (event.actionMasked) {
                android.view.MotionEvent.ACTION_DOWN -> v.animate().scaleX(0.97f).scaleY(0.97f).setDuration(90L).start()
                android.view.MotionEvent.ACTION_UP, android.view.MotionEvent.ACTION_CANCEL -> v.animate().scaleX(1f).scaleY(1f).setDuration(130L).start()
            }
            false
        }
    }

    private fun toggleFullscreen(fullscreen: Boolean) {
        isFullscreenMode = fullscreen
        header.visibility = if (fullscreen) View.GONE else View.VISIBLE
        controls.visibility = if (fullscreen) View.GONE else View.VISIBLE
        fullscreenFloatBtn.visibility = if (fullscreen) View.VISIBLE else View.GONE
        immersive()
        excludeSystemGestures(pad)
    }

    private fun showGestureTestDialog() {
        val dialog = android.app.Dialog(this).apply {
            requestWindowFeature(android.view.Window.FEATURE_NO_TITLE)
            window?.setBackgroundDrawable(android.graphics.drawable.ColorDrawable(Color.TRANSPARENT))
        }

        val scroll = android.widget.ScrollView(this)
        val container = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            val bg = GradientDrawable().apply {
                setColor(0xF0181822.toInt())
                cornerRadius = dp(16).toFloat()
                setStroke(dp(1), 0x44FFFFFF)
            }
            background = bg
            setPadding(dp(20), dp(18), dp(20), dp(20))
        }

        val title = TextView(this).apply {
            text = "🛠 macOS 手势命令发射面板"
            setTextColor(Color.WHITE)
            textSize = 16f
            typeface = android.graphics.Typeface.DEFAULT_BOLD
            gravity = Gravity.CENTER_HORIZONTAL
        }
        val subtitle = TextView(this).apply {
            text = "点击按钮直接模拟真实指尖轨迹派发至 Mac\n请将 Mac 鼠标先悬停在 Safari / 目标窗口上"
            setTextColor(0xFFAAAAAA.toInt())
            textSize = 11f
            gravity = Gravity.CENTER_HORIZONTAL
            val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                setMargins(0, dp(4), 0, dp(14))
            }
            layoutParams = lp
        }

        container.addView(title)
        container.addView(subtitle)

        fun makeTestBtn(name: String, desc: String, color: Int, action: () -> Unit): View {
            val card = LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                val bg = GradientDrawable().apply {
                    setColor(color)
                    cornerRadius = dp(10).toFloat()
                    setStroke(dp(1), 0x33FFFFFF)
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

            val tName = TextView(this).apply {
                text = name
                setTextColor(Color.WHITE)
                textSize = 13f
                typeface = android.graphics.Typeface.DEFAULT_BOLD
            }
            val tDesc = TextView(this).apply {
                text = desc
                setTextColor(0xCCFFFFFF.toInt())
                textSize = 10f
            }
            card.addView(tName)
            card.addView(tDesc)
            return card
        }

        // Add test triggers
        container.addView(makeTestBtn("🔍 智能缩放 (Smart Zoom)", "双指双击：Safari/预览 网页段落或图片平滑放大居中", 0xFF1E3A8A.toInt()) {
            GestureTestRunner.runSmartZoom(sender)
        })

        container.addView(makeTestBtn("📖 三指查词 (Look Up)", "三指轻点：弹出 macOS 系统词典释义气泡", 0xFF065F46.toInt()) {
            GestureTestRunner.runLookup(sender)
        })

        container.addView(makeTestBtn("◀️ 四指左轻扫 (切到右侧桌面)", "四指水平左扫：切换到下一个 Spaces 桌面", 0xFF3730A3.toInt()) {
            GestureTestRunner.runSwipeLeft(sender)
        })

        container.addView(makeTestBtn("▶️ 四指右轻扫 (切到左侧桌面)", "四指水平右扫：切换到上一个 Spaces 桌面", 0xFF3730A3.toInt()) {
            GestureTestRunner.runSwipeRight(sender)
        })

        container.addView(makeTestBtn("🔼 四指上滑 (调度中心)", "四指垂直上推：展开 macOS Mission Control", 0xFF6B21A8.toInt()) {
            GestureTestRunner.runSwipeUp(sender)
        })

        container.addView(makeTestBtn("🔽 四指下滑 (应用程序窗口)", "四指垂直下拉：展开 App Exposé 当前应用多窗口", 0xFF6B21A8.toInt()) {
            GestureTestRunner.runSwipeDown(sender)
        })

        container.addView(makeTestBtn("🔍➕ 双指捏合放大 (+30%)", "双指向外扩张：Safari/地图/文档 视口无级缩放", 0xFF831843.toInt()) {
            GestureTestRunner.runPinchIn(sender)
        })

        container.addView(makeTestBtn("🔍➖ 双指捏合缩小 (-30%)", "双指向内聚拢：Safari/地图/文档 视口缩小", 0xFF831843.toInt()) {
            GestureTestRunner.runPinchOut(sender)
        })

        container.addView(makeTestBtn("🔄 双指顺时针旋转 90°", "双指圆周旋转：在照片/预览中旋转图片", 0xFF92400E.toInt()) {
            GestureTestRunner.runRotate(sender)
        })

        container.addView(makeTestBtn("🖱 双指右键点击", "双指轻点：弹出光标所在处的系统右键上下文菜单", 0xFF1F2937.toInt()) {
            GestureTestRunner.runRightClick(sender)
        })

        container.addView(makeTestBtn("✋ 三指拖移测试", "三指接触并平移：选中文本或拖动窗口标题栏", 0xFF1F2937.toInt()) {
            GestureTestRunner.runThreeFingerDrag(sender)
        })

        container.addView(makeTestBtn("📬 通知中心 (Notification Center)", "双指从右边缘向左滑入：打开/关闭 macOS 系统通知中心", 0xFF0C4A6E.toInt()) {
            GestureTestRunner.runNotificationCenter(sender)
        })

        container.addView(makeTestBtn("🚀 启动台 (Launchpad)", "四指向内捏合：展开 macOS Launchpad 应用程序网格", 0xFF047857.toInt()) {
            GestureTestRunner.runLaunchpadPinch(sender)
        })

        container.addView(makeTestBtn("🖥️ 显示桌面 (Show Desktop)", "四指向外张开：推开所有应用窗口显示纯净桌面", 0xFF0369A1.toInt()) {
            GestureTestRunner.runShowDesktopSpread(sender)
        })

        container.addView(makeTestBtn("✊ 软件长按拖拽 (Press-and-Hold Drag)", "单指原地按住450ms扣住左键并拖拽选中，抬手释放", 0xFFB45309.toInt()) {
            GestureTestRunner.runPressAndHoldDrag(sender)
        })

        val closeBtn = Button(this).apply {
            text = "关闭面板"
            setTextColor(Color.WHITE)
            textSize = 12f
            val bg = GradientDrawable().apply {
                setColor(0xFF333340.toInt())
                cornerRadius = dp(8).toFloat()
            }
            background = bg
            val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                setMargins(0, dp(12), 0, 0)
            }
            layoutParams = lp
            setOnClickListener { dialog.dismiss() }
        }
        container.addView(closeBtn)

        scroll.addView(container)
        dialog.setContentView(scroll)
        dialog.window?.setLayout((resources.displayMetrics.widthPixels * 0.90).toInt(), ViewGroup.LayoutParams.WRAP_CONTENT)
        dialog.show()
    }

    private fun showDeepPressSettingsDialog() {
        val dialog = android.app.Dialog(this).apply {
            requestWindowFeature(android.view.Window.FEATURE_NO_TITLE)
            window?.setBackgroundDrawable(android.graphics.drawable.ColorDrawable(Color.TRANSPARENT))
        }
        val container = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            background = GradientDrawable().apply {
                setColor(0xF0181822.toInt())
                cornerRadius = dp(16).toFloat()
                setStroke(dp(1), 0x44FFFFFF)
            }
            setPadding(dp(20), dp(18), dp(20), dp(20))
        }
        val title = TextView(this).apply {
            text = "深按条设置"
            setTextColor(Color.WHITE)
            textSize = 16f
            typeface = android.graphics.Typeface.DEFAULT_BOLD
            gravity = Gravity.CENTER_HORIZONTAL
        }
        container.addView(title)

        val enabled = android.widget.CheckBox(this).apply {
            text = "显示深按条"
            isChecked = prefs.getBoolean(KEY_DEEP_ENABLED, true)
            setTextColor(Color.WHITE)
        }
        container.addView(enabled)

        fun addSeekBar(
            label: String,
            min: Int,
            max: Int,
            initial: Int,
            suffix: String,
        ): android.widget.SeekBar {
            val value = TextView(this).apply {
                setTextColor(0xFFCCCCCC.toInt())
                textSize = 12f
            }
            val titleRow = LinearLayout(this).apply {
                orientation = LinearLayout.HORIZONTAL
                val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                layoutParams = lp
                addView(TextView(this@MainActivity).apply {
                    text = label
                    setTextColor(Color.WHITE)
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

        val save = Button(this).apply {
            text = "保存"
            setTextColor(Color.WHITE)
            textSize = 12f
            background = GradientDrawable().apply {
                setColor(0xFF007AFF.toInt())
                cornerRadius = dp(8).toFloat()
            }
            val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                setMargins(0, dp(14), 0, 0)
            }
            layoutParams = lp
            setOnClickListener {
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
            }
        }
        container.addView(save)
        dialog.setContentView(container)
        dialog.show()
        dialog.window?.setLayout((resources.displayMetrics.widthPixels * 0.88).toInt(), ViewGroup.LayoutParams.WRAP_CONTENT)
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
        status.text = msg
        val d = GradientDrawable().apply {
            shape = GradientDrawable.OVAL
            setColor(if (connected) 0xFF3DDC84.toInt() else Color.GRAY)
        }
        dot.background = d
    }

    /** Sticky fullscreen + hide nav so system edge gestures stop eating touches. */
    private fun immersive() {
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

    override fun onNewIntent(intent: Intent?) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (applyPairingIntent(intent)) {
            if (::pad.isInitialized) {
                connectToMac(
                    prefs.getString(KEY_HOST, "") ?: "",
                    prefs.getString(KEY_PORT, "4242") ?: "4242",
                    prefs.getString(KEY_TOKEN, "") ?: "",
                )
            }
            Toast.makeText(this, "已载入配对信息", Toast.LENGTH_SHORT).show()
        }
    }

    private fun applyPairingIntent(intent: Intent?): Boolean {
        val target = PairingUri.parse(intent?.dataString) ?: return false
        prefs.edit()
            .putString(KEY_HOST, target.host)
            .putString(KEY_PORT, target.port.toString())
            .putString(KEY_TOKEN, target.token.orEmpty())
            .apply()
        return true
    }

    override fun onDestroy() {
        if (::deepPressBar.isInitialized) {
            deepPressBar.cancelPress()
            pad.removeCallbacks(deepButtonHeartbeat)
        }
        sender.close()
        if (::discovery.isInitialized) discovery.stop()
        super.onDestroy()
    }

    private fun dp(v: Int): Int = (v * resources.displayMetrics.density).toInt()

    companion object {
        private const val KEY_HOST = "host"
        private const val KEY_PORT = "port"
        private const val KEY_TOKEN = "token"
        private const val KEY_THEME = "theme"
        private const val KEY_SCALE = "scale"
        private const val KEY_HAPTIC = "haptic"
        private const val KEY_DEEP_ENABLED = "deep_press_enabled"
        private const val KEY_DEEP_HOLD_MS = "deep_press_hold_ms"
        private const val KEY_DEEP_HAPTIC_STRENGTH = "deep_press_haptic_strength"
        private const val KEY_DEEP_X = "deep_press_x"
        private const val KEY_DEEP_Y = "deep_press_y"
        private const val KEY_DEEP_WIDTH = "deep_press_width"
        private const val KEY_DEEP_HEIGHT = "deep_press_height"
        private const val DEFAULT_DEEP_HOLD_MS = 650L
        private const val DEFAULT_DEEP_HAPTIC_STRENGTH = 255
        private const val DEFAULT_DEEP_X = 0.5f
        private const val DEFAULT_DEEP_Y = 0.82f
        private const val DEFAULT_DEEP_WIDTH = 240
        private const val DEFAULT_DEEP_HEIGHT = 52
        private const val DEEP_BUTTON_HEARTBEAT_MS = 100L
    }
}
