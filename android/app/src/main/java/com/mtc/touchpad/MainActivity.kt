package com.mtc.touchpad

import android.app.Activity
import android.content.SharedPreferences
import android.graphics.Color
import android.graphics.Rect
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

class MainActivity : Activity() {

    private lateinit var prefs: SharedPreferences
    private lateinit var sender: UdpSender
    private lateinit var pad: TouchPadView
    private lateinit var dot: View
    private lateinit var status: TextView
    private lateinit var header: LinearLayout
    private lateinit var controls: LinearLayout
    private lateinit var fullscreenFloatBtn: Button
    private var isFullscreenMode = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        prefs = getSharedPreferences("touchpad", MODE_PRIVATE)
        sender = UdpSender()

        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)

        pad = TouchPadView(this).also {
            it.sender = sender
            it.scale = prefs.getFloat(KEY_SCALE, 1f)
        }

        dot = View(this).apply {
            val d = GradientDrawable().apply {
                shape = GradientDrawable.OVAL
                setColor(Color.GRAY)
            }
            background = d
            layoutParams = LinearLayout.LayoutParams(dp(10), dp(10)).apply { gravity = Gravity.CENTER_VERTICAL }
        }
        status = TextView(this).apply {
            text = "未连接"
            setTextColor(0xFFCCCCCC.toInt())
            textSize = 13f
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
                .apply { marginStart = dp(8) }
            maxLines = 1
        }

        fun makeInputBg(): GradientDrawable = GradientDrawable().apply {
            setColor(0xFF1E1E24.toInt())
            cornerRadius = dp(8).toFloat()
            setStroke(dp(1), 0x33FFFFFF)
        }

        val ip = EditText(this).apply {
            hint = "Mac IP"
            setText(prefs.getString(KEY_HOST, ""))
            setSingleLine()
            textSize = 14f
            background = makeInputBg()
            setPadding(dp(10), dp(6), dp(10), dp(6))
            setTextColor(Color.WHITE); setHintTextColor(0xFF777788.toInt())
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1.5f).apply { marginEnd = dp(6) }
        }
        val port = EditText(this).apply {
            hint = "4242"
            setText(prefs.getString(KEY_PORT, "4242"))
            setSingleLine(); inputType = android.text.InputType.TYPE_CLASS_NUMBER
            textSize = 14f
            background = makeInputBg()
            setPadding(dp(10), dp(6), dp(10), dp(6))
            setTextColor(Color.WHITE); setHintTextColor(0xFF777788.toInt())
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 0.8f).apply { marginEnd = dp(6) }
        }
        val token = EditText(this).apply {
            hint = "Token（可选）"
            setText(prefs.getString(KEY_TOKEN, ""))
            setSingleLine()
            inputType = android.text.InputType.TYPE_CLASS_TEXT or
                android.text.InputType.TYPE_TEXT_VARIATION_PASSWORD
            textSize = 14f
            background = makeInputBg()
            setPadding(dp(10), dp(6), dp(10), dp(6))
            setTextColor(Color.WHITE); setHintTextColor(0xFF777788.toInt())
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1.2f).apply { marginEnd = dp(6) }
        }

        fun makeBtnBg(activeColor: Int = 0xFF2A2A36.toInt()): GradientDrawable = GradientDrawable().apply {
            setColor(activeColor)
            cornerRadius = dp(8).toFloat()
            setStroke(dp(1), 0x44FFFFFF)
        }

        val connect = Button(this).apply {
            text = "连接"
            setTextColor(Color.WHITE)
            textSize = 13f
            background = makeBtnBg(0xFF007AFF.toInt())
            setPadding(dp(14), dp(6), dp(14), dp(6))
        }

        val headerBg = GradientDrawable().apply {
            setColor(0xEE14141A.toInt())
            cornerRadius = dp(12).toFloat()
            setStroke(dp(1), 0x22FFFFFF)
        }

        header = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = headerBg
            setPadding(dp(12), dp(8), dp(12), dp(8))
            val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                setMargins(dp(10), dp(8), dp(10), dp(4))
            }
            layoutParams = lp
            addView(dot)
            addView(status)
            addView(ip); addView(port); addView(token); addView(connect)
        }

        controls = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.END or Gravity.CENTER_VERTICAL
            setPadding(dp(10), dp(4), dp(10), dp(8))

            val testBtn = Button(this@MainActivity).apply {
                text = "🛠 手势测试"
                setTextColor(Color.WHITE)
                textSize = 12f
                background = makeBtnBg(0xFF5856D6.toInt())
                setPadding(dp(12), dp(4), dp(12), dp(4))
                val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                    marginStart = dp(8)
                }
                layoutParams = lp
                setOnClickListener { showGestureTestDialog() }
            }
            addView(testBtn)

            val fullscreenBtn = Button(this@MainActivity).apply {
                text = "⛶ 全屏"
                setTextColor(Color.WHITE)
                textSize = 12f
                background = makeBtnBg()
                setPadding(dp(12), dp(4), dp(12), dp(4))
                val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                    marginStart = dp(8)
                }
                layoutParams = lp
                setOnClickListener { toggleFullscreen(true) }
            }
            addView(fullscreenBtn)

            val hapticBtn = Button(this@MainActivity).apply {
                var hapticOn = prefs.getBoolean(KEY_HAPTIC, true)
                pad.haptics.enabled = hapticOn
                text = if (hapticOn) "震动: 开" else "震动: 关"
                setTextColor(Color.WHITE)
                textSize = 12f
                background = makeBtnBg()
                setPadding(dp(12), dp(4), dp(12), dp(4))
                val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                    marginStart = dp(8)
                }
                layoutParams = lp
                setOnClickListener {
                    hapticOn = !hapticOn
                    pad.haptics.enabled = hapticOn
                    prefs.edit().putBoolean(KEY_HAPTIC, hapticOn).apply()
                    text = if (hapticOn) "震动: 开" else "震动: 关"
                    if (hapticOn) pad.haptics.click(this)
                }
            }
            addView(hapticBtn)

            fun calBtn(label: String, mul: Float) = Button(this@MainActivity).apply {
                text = label
                setTextColor(Color.WHITE)
                textSize = 12f
                background = makeBtnBg()
                setPadding(dp(12), dp(4), dp(12), dp(4))
                val lp = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                    marginStart = dp(8)
                }
                layoutParams = lp
                setOnClickListener {
                    pad.scale = (pad.scale * mul).coerceIn(0.55f, 1.6f)
                    prefs.edit().putFloat(KEY_SCALE, pad.scale).apply()
                    Toast.makeText(this@MainActivity, "缩放 ${(pad.scale * 100).toInt()}%", Toast.LENGTH_SHORT).show()
                }
            }
            addView(calBtn("A−", 1 / 1.15f))
            addView(calBtn("A＋", 1.15f))
        }

        val mainLayout = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(0xFF0D0D11.toInt())
            addView(header)
            addView(pad, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
            addView(controls)
        }

        fullscreenFloatBtn = Button(this).apply {
            text = "⚙ 设置"
            setTextColor(0xDDFFFFFF.toInt())
            textSize = 11f
            alpha = 0.55f
            background = GradientDrawable().apply {
                setColor(0xAA1C1C24.toInt())
                cornerRadius = dp(16).toFloat()
                setStroke(dp(1), 0x55FFFFFF)
            }
            visibility = View.GONE
            setPadding(dp(12), dp(4), dp(12), dp(4))
            val lp = FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                gravity = Gravity.TOP or Gravity.END
                setMargins(0, dp(16), dp(16), 0)
            }
            layoutParams = lp
            setOnClickListener { toggleFullscreen(false) }
        }

        val rootFrame = FrameLayout(this).apply {
            addView(mainLayout, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            addView(fullscreenFloatBtn)
        }

        setContentView(rootFrame)

        connect.setOnClickListener {
            val host = ip.text.toString().trim()
            val portText = port.text.toString().trim().ifEmpty { "4242" }
            val tokenText = token.text.toString().trim()
            if (host.isEmpty()) { Toast.makeText(this, "先填 Mac 的 IP", Toast.LENGTH_SHORT).show(); return@setOnClickListener }
            prefs.edit().putString(KEY_HOST, host).putString(KEY_PORT, portText).putString(KEY_TOKEN, tokenText).apply()
            setStatus(false, "连接中…")
            sender.connect(host, portText.toIntOrNull() ?: 4242, tokenText.ifEmpty { null }, object : UdpSender.Listener {
                override fun onState(connected: Boolean, message: String) =
                    runOnUiThread { setStatus(connected, message) }
            })
        }

        if (!prefs.getString(KEY_HOST, "").isNullOrEmpty()) {
            connect.performClick()
        }

        immersive()
        excludeSystemGestures(rootFrame)
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

    override fun onDestroy() {
        sender.close()
        super.onDestroy()
    }

    private fun dp(v: Int): Int = (v * resources.displayMetrics.density).toInt()

    companion object {
        private const val KEY_HOST = "host"
        private const val KEY_PORT = "port"
        private const val KEY_TOKEN = "token"
        private const val KEY_SCALE = "scale"
        private const val KEY_HAPTIC = "haptic"
    }
}
