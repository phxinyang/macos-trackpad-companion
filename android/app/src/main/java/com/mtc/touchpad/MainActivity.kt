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
            addView(ip); addView(port); addView(connect)
        }

        controls = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.END or Gravity.CENTER_VERTICAL
            setPadding(dp(10), dp(4), dp(10), dp(8))

            val fullscreenBtn = Button(this@MainActivity).apply {
                text = "⛶ 全屏"
                setTextColor(Color.WHITE)
                textSize = 12f
                background = makeBtnBg()
                setPadding(dp(12), dp(4), dp(12), dp(4))
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
            if (host.isEmpty()) { Toast.makeText(this, "先填 Mac 的 IP", Toast.LENGTH_SHORT).show(); return@setOnClickListener }
            prefs.edit().putString(KEY_HOST, host).putString(KEY_PORT, portText).apply()
            setStatus(false, "连接中…")
            sender.connect(host, portText.toIntOrNull() ?: 4242, object : UdpSender.Listener {
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
        private const val KEY_SCALE = "scale"
        private const val KEY_HAPTIC = "haptic"
    }
}
