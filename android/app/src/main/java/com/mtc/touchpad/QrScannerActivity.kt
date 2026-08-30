package com.mtc.touchpad

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.graphics.ImageFormat
import android.graphics.drawable.GradientDrawable
import android.hardware.Camera
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.view.Gravity
import android.view.MotionEvent
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import com.google.mlkit.vision.barcode.BarcodeScanner
import com.google.mlkit.vision.barcode.BarcodeScannerOptions
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.common.InputImage

/**
 * Self-contained QR scanner used by the connection sheet. Unlike Google's
 * Play-services Code Scanner this screen owns the camera and uses ML Kit's
 * bundled barcode model, so first-run scanning does not depend on a deferred
 * `barcode_ui` download.
 */
class QrScannerActivity : Activity(), SurfaceHolder.Callback {
    private lateinit var preview: SurfaceView
    private lateinit var status: TextView
    private var focusBox: View? = null
    private var permissionButton: Button? = null
    private var camera: Camera? = null
    private var scanner: BarcodeScanner? = null
    private var previewSize: Camera.Size? = null
    private var focusMode: String? = null
    private var rotationDegrees = 0
    private var surfaceReady = false
    private var processingFrame = false
    private var finished = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        window.statusBarColor = Color.BLACK
        window.navigationBarColor = Color.BLACK

        scanner = BarcodeScanning.getClient(
            BarcodeScannerOptions.Builder()
                .setBarcodeFormats(Barcode.FORMAT_QR_CODE)
                .build(),
        )
        setContentView(buildContent())
        if (checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) {
            status.text = "将 Mac 上的二维码放入框内"
        } else {
            requestPermissions(arrayOf(Manifest.permission.CAMERA), REQUEST_CAMERA)
        }
    }

    private fun buildContent(): View {
        val root = FrameLayout(this).apply { setBackgroundColor(Color.BLACK) }
        preview = SurfaceView(this)
        preview.holder.addCallback(this)
        preview.setOnTouchListener { _, event ->
            if (event.action == MotionEvent.ACTION_UP) {
                requestFocus()
            }
            true
        }
        root.addView(preview, FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.MATCH_PARENT,
        ))

        focusBox = View(this).apply {
            background = GradientDrawable().apply {
                setColor(Color.TRANSPARENT)
                setStroke(dp(2), Color.WHITE)
                cornerRadius = dp(18).toFloat()
            }
            alpha = 0.9f
            contentDescription = "二维码取景框"
        }
        root.addView(focusBox, FrameLayout.LayoutParams(dp(280), dp(280), Gravity.CENTER))

        val top = TextView(this).apply {
            text = "扫描 Mac 配对二维码"
            textSize = 20f
            setTextColor(Color.WHITE)
            gravity = Gravity.CENTER
            setPadding(dp(18), dp(14), dp(18), dp(14))
            background = ColorDrawableCompat.argb(180, 0, 0, 0)
        }
        root.addView(top, FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.WRAP_CONTENT,
            Gravity.TOP,
        ))

        val bottom = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            setPadding(dp(18), dp(14), dp(18), dp(18))
            background = ColorDrawableCompat.argb(190, 0, 0, 0)
        }
        status = TextView(this).apply {
            text = "正在准备相机…"
            textSize = 14f
            gravity = Gravity.CENTER
            setTextColor(Color.WHITE)
            setPadding(0, 0, 0, dp(10))
        }
        bottom.addView(status, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.WRAP_CONTENT,
        ))
        permissionButton = Button(this).apply {
            text = "打开相机权限"
            isAllCaps = false
            visibility = View.GONE
            setOnClickListener { openPermissionSettings() }
        }
        bottom.addView(permissionButton, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            dp(48),
        ))
        val cancel = Button(this).apply {
            text = "取消"
            isAllCaps = false
            setOnClickListener { finishCanceled() }
        }
        bottom.addView(cancel, LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            dp(48),
        ).apply { topMargin = dp(8) })
        root.addView(bottom, FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            ViewGroup.LayoutParams.WRAP_CONTENT,
            Gravity.BOTTOM,
        ))
        return root
    }

    override fun surfaceCreated(holder: SurfaceHolder) {
        surfaceReady = true
        openCameraIfReady(holder)
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        surfaceReady = false
        releaseCamera()
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        if (width > 0 && height > 0) openCameraIfReady(holder)
    }

    private fun openCameraIfReady(holder: SurfaceHolder = preview.holder) {
        if (!surfaceReady || camera != null || checkSelfPermission(Manifest.permission.CAMERA) != PackageManager.PERMISSION_GRANTED) return
        try {
            val cameraId = findBackCamera()
            if (cameraId < 0) throw IllegalStateException("未找到后置摄像头")
            val next = Camera.open(cameraId)
            val parameters = next.parameters
            val selected = selectPreviewSize(parameters)
            if (selected != null) {
                parameters.setPreviewSize(selected.width, selected.height)
                previewSize = selected
            }
            parameters.previewFormat = ImageFormat.NV21
            focusMode = configureFocus(parameters)
            next.parameters = parameters
            rotationDegrees = inputRotation(cameraId)
            next.setDisplayOrientation(displayRotation(cameraId))
            next.setPreviewDisplay(holder)
            next.setPreviewCallbackWithBuffer { data, _ -> processFrame(data) }
            val size = previewSize ?: next.parameters.previewSize
            val bufferSize = size.width * size.height * ImageFormat.getBitsPerPixel(ImageFormat.NV21) / 8
            next.addCallbackBuffer(ByteArray(bufferSize))
            camera = next
            next.startPreview()
            requestFocus()
            status.text = "将 Mac 上的二维码放入框内"
            permissionButton?.visibility = View.GONE
        } catch (error: Exception) {
            releaseCamera()
            status.text = "相机不可用，请返回使用 IP 连接"
            permissionButton?.visibility = View.GONE
            Toast.makeText(this, error.localizedMessage ?: "无法打开相机", Toast.LENGTH_LONG).show()
        }
    }

    private fun processFrame(data: ByteArray) {
        val size = previewSize ?: run {
            returnCameraBuffer(data)
            return
        }
        if (finished || processingFrame || scanner == null) {
            returnCameraBuffer(data)
            return
        }
        processingFrame = true
        val image = InputImage.fromByteArray(data, size.width, size.height, rotationDegrees, ImageFormat.NV21)
        scanner?.process(image)
            ?.addOnSuccessListener { codes ->
                val value = codes.asSequence().mapNotNull { it.rawValue }.firstOrNull { !it.isBlank() }
                if (value != null) finishWithValue(value)
            }
            ?.addOnFailureListener { error ->
                status.text = "识别中…${error.localizedMessage.orEmpty()}"
            }
            ?.addOnCompleteListener {
                processingFrame = false
                returnCameraBuffer(data)
            }
    }

    private fun returnCameraBuffer(data: ByteArray) {
        camera?.let { runCatching { it.addCallbackBuffer(data) } }
    }

    private fun finishWithValue(value: String) {
        if (finished) return
        finished = true
        releaseCamera()
        setResult(RESULT_OK, Intent().putExtra(EXTRA_QR_VALUE, value))
        finish()
    }

    private fun finishCanceled() {
        if (finished) return
        finished = true
        releaseCamera()
        setResult(RESULT_CANCELED)
        finish()
    }

    private fun releaseCamera() {
        camera?.let { active ->
            runCatching { active.setPreviewCallbackWithBuffer(null) }
            runCatching { active.cancelAutoFocus() }
            runCatching { active.stopPreview() }
            runCatching { active.release() }
        }
        camera = null
        focusMode = null
        processingFrame = false
    }

    /**
     * Use the highest detail stream within a full-HD budget. The old fixed
     * 1280x720 choice was visibly soft on modern high-density phones, and
     * ranking aspect ratio first could pick 1440x720 over a sharper 1920x1080
     * stream on extra-wide displays.
     */
    private fun selectPreviewSize(parameters: Camera.Parameters): Camera.Size? {
        val supported = parameters.supportedPreviewSizes ?: return null
        if (supported.isEmpty()) return null
        val displayWidth = preview.width.takeIf { it > 0 } ?: resources.displayMetrics.widthPixels
        val displayHeight = preview.height.takeIf { it > 0 } ?: resources.displayMetrics.heightPixels
        val targetAspect = aspectRatio(displayWidth, displayHeight)
        val fullHdOrSmaller = supported.filter {
            it.width * it.height <= MAX_PREVIEW_PIXELS && it.width >= MIN_PREVIEW_WIDTH
        }
        val candidates = if (fullHdOrSmaller.isNotEmpty()) fullHdOrSmaller else supported
        return candidates.maxWithOrNull(
            compareBy<Camera.Size> { it.width * it.height }
                .thenBy { -kotlin.math.abs(aspectRatio(it.width, it.height) - targetAspect) },
        )
    }

    private fun aspectRatio(width: Int, height: Int): Double {
        val longEdge = maxOf(width, height).toDouble()
        val shortEdge = minOf(width, height).toDouble().coerceAtLeast(1.0)
        return longEdge / shortEdge
    }

    private fun configureFocus(parameters: Camera.Parameters): String? {
        val modes = parameters.supportedFocusModes ?: emptyList()
        val selected = when {
            Camera.Parameters.FOCUS_MODE_CONTINUOUS_PICTURE in modes -> Camera.Parameters.FOCUS_MODE_CONTINUOUS_PICTURE
            Camera.Parameters.FOCUS_MODE_AUTO in modes -> Camera.Parameters.FOCUS_MODE_AUTO
            Camera.Parameters.FOCUS_MODE_CONTINUOUS_VIDEO in modes -> Camera.Parameters.FOCUS_MODE_CONTINUOUS_VIDEO
            else -> null
        }
        if (selected != null) parameters.focusMode = selected
        return selected
    }

    private fun requestFocus() {
        val active = camera ?: return
        if (focusMode != Camera.Parameters.FOCUS_MODE_AUTO) return
        runCatching {
            active.autoFocus { _, _ -> }
        }
    }

    private fun findBackCamera(): Int {
        val info = Camera.CameraInfo()
        for (index in 0 until Camera.getNumberOfCameras()) {
            Camera.getCameraInfo(index, info)
            if (info.facing == Camera.CameraInfo.CAMERA_FACING_BACK) return index
        }
        return -1
    }

    private fun displayRotation(cameraId: Int): Int {
        val info = Camera.CameraInfo()
        Camera.getCameraInfo(cameraId, info)
        val degrees = displayDegrees()
        return if (info.facing == Camera.CameraInfo.CAMERA_FACING_FRONT) {
            (info.orientation + degrees) % 360
        } else {
            (info.orientation - degrees + 360) % 360
        }
    }

    private fun inputRotation(cameraId: Int): Int = displayRotation(cameraId)

    private fun displayDegrees(): Int = when (windowManager.defaultDisplay.rotation) {
        Surface.ROTATION_90 -> 90
        Surface.ROTATION_180 -> 180
        Surface.ROTATION_270 -> 270
        else -> 0
    }

    private fun openPermissionSettings() {
        runCatching {
            startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS).apply {
                data = Uri.parse("package:$packageName")
            })
        }.onFailure {
            Toast.makeText(this, "请在系统设置中允许相机权限", Toast.LENGTH_LONG).show()
        }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode != REQUEST_CAMERA) return
        if (grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED) {
            status.text = "将 Mac 上的二维码放入框内"
            openCameraIfReady()
        } else {
            status.text = "需要相机权限才能扫描二维码，也可以返回使用 IP 连接"
            permissionButton?.visibility = View.VISIBLE
        }
    }

    override fun onPause() {
        releaseCamera()
        super.onPause()
    }

    override fun onDestroy() {
        releaseCamera()
        scanner?.close()
        scanner = null
        super.onDestroy()
    }

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()

    private object ColorDrawableCompat {
        fun argb(alpha: Int, red: Int, green: Int, blue: Int): android.graphics.drawable.ColorDrawable =
            android.graphics.drawable.ColorDrawable(Color.argb(alpha, red, green, blue))
    }

    companion object {
        const val EXTRA_QR_VALUE = "qr_value"
        private const val REQUEST_CAMERA = 41
        private const val MIN_PREVIEW_WIDTH = 1280
        private const val MAX_PREVIEW_PIXELS = 1920 * 1080
    }
}
