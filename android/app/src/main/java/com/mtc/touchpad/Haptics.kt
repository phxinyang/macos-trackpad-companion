package com.mtc.touchpad

import android.content.Context
import android.media.AudioAttributes
import android.os.Build
import android.os.VibrationEffect
import android.os.VibrationAttributes
import android.os.Vibrator
import android.os.VibratorManager
import android.view.HapticFeedbackConstants
import android.view.View

/**
 * High-fidelity haptic feedback controller.
 * Emulates the crisp click/latch feel of the Apple Force Touch / Taptic Engine
 * using the Android device vibrator and its calibrated driver.
 */
class Haptics(context: Context) {

    private val vibrator: Vibrator? = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        val manager = context.getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as? VibratorManager
        manager?.defaultVibrator
    } else {
        @Suppress("DEPRECATION")
        context.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator
    }

    var enabled: Boolean = true

    /** Peak amplitude for the synthesized deep-press latch, in Android's 1..255 range. */
    var deepPressStrength: Int = 255
        set(value) {
            field = value.coerceIn(40, 255)
        }

    /** Single crisp click (Apple Trackpad tap click - ultra-sharp Taptic Engine click) */
    fun click(view: View? = null) {
        if (!enabled) return
        if (vibrator != null && vibrator.hasVibrator()) {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && vibrator.hasAmplitudeControl()) {
                try {
                    vibrate(
                        VibrationEffect.createWaveform(
                            HapticProfiles.normalClickTimings,
                            HapticProfiles.normalClickAmplitudes,
                            -1,
                        ),
                        VibrationAttributes.USAGE_TOUCH,
                    )
                    return
                } catch (_: Exception) {}
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                try {
                    vibrator.vibrate(VibrationEffect.createPredefined(VibrationEffect.EFFECT_CLICK))
                    return
                } catch (_: Exception) {}
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                try {
                    val comp = VibrationEffect.startComposition()
                        .addPrimitive(VibrationEffect.Composition.PRIMITIVE_CLICK, 1.0f)
                        .compose()
                    vibrator.vibrate(comp)
                    return
                } catch (_: Exception) {}
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                try {
                    vibrator.vibrate(VibrationEffect.createOneShot(8, 200))
                    return
                } catch (_: Exception) {}
            }
        }
        view?.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
    }

    /** Heavy / prominent click (like right-click or secondary Force Touch press) */
    fun heavyClick(view: View? = null) {
        if (!enabled) return
        if (vibrator != null && vibrator.hasVibrator()) {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                try {
                    vibrator.vibrate(VibrationEffect.createPredefined(VibrationEffect.EFFECT_HEAVY_CLICK))
                    return
                } catch (_: Exception) {}
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                try {
                    val comp = VibrationEffect.startComposition()
                        .addPrimitive(VibrationEffect.Composition.PRIMITIVE_CLICK, 1.0f)
                        .addPrimitive(VibrationEffect.Composition.PRIMITIVE_TICK, 0.7f, 15)
                        .compose()
                    vibrator.vibrate(comp)
                    return
                } catch (_: Exception) {}
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                try {
                    vibrator.vibrate(VibrationEffect.createOneShot(14, 255))
                    return
                } catch (_: Exception) {}
            }
        }
        view?.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
    }

    /**
     * Confirmation for the APK's deep-press bar.
     *
     * This phone advertises amplitude control but no haptic primitives. On
     * Xiaomi/HyperOS, the launcher exposes a calibrated hardware-feedback
     * effect (163), so that path is preferred. Other devices use a short
     * custom waveform instead of HEAVY_CLICK, whose vendor-defined fallback
     * is a relatively soft ~75 ms pulse here.
     * The normal click is emitted by the bar on ACTION_DOWN. This event is the
     * deeper second click: a sharp full-amplitude onset followed by a short
     * damped tail, instead of two equally prominent buzzing pulses.
     */
    fun deepPress(view: View? = null) {
        if (!enabled) return
        if (vibrator != null && vibrator.hasVibrator()) {
            if (tryXiaomiHardwareClick()) return
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && vibrator.hasAmplitudeControl()) {
                try {
                    val peak = deepPressStrength.coerceIn(40, 255)
                    val effect = VibrationEffect.createWaveform(
                        HapticProfiles.deepPressTimings,
                        HapticProfiles.deepPressAmplitudes(peak),
                        -1,
                    )
                    vibrate(effect, VibrationAttributes.USAGE_PHYSICAL_EMULATION)
                    return
                } catch (_: Exception) {}
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                try {
                    vibrate(
                        VibrationEffect.createPredefined(VibrationEffect.EFFECT_HEAVY_CLICK),
                        VibrationAttributes.USAGE_PHYSICAL_EMULATION,
                    )
                    return
                } catch (_: Exception) {}
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                try {
                    vibrate(
                        VibrationEffect.createOneShot(24, deepPressStrength),
                        VibrationAttributes.USAGE_PHYSICAL_EMULATION,
                    )
                    return
                } catch (_: Exception) {}
            }
        }
        view?.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
    }

    /**
     * Xiaomi/HyperOS ships calibrated hardware-feedback effects outside the
     * public Android constants. Effect 163 is the same pattern used by the
     * system launcher on this device. Use it only when the vendor and HAL both
     * advertise support; every other device stays on the portable waveform.
     */
    private fun tryXiaomiHardwareClick(): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return false
        if (!HapticProfiles.isXiaomiOrRedmi(Build.MANUFACTURER, Build.BRAND)) return false
        val v = vibrator ?: return false
        if (v.areAllEffectsSupported(HapticProfiles.XIAOMI_HARDWARE_EFFECT_ID) != Vibrator.VIBRATION_EFFECT_SUPPORT_YES) return false
        return try {
            vibrate(
                VibrationEffect.createPredefined(HapticProfiles.XIAOMI_HARDWARE_EFFECT_ID),
                VibrationAttributes.USAGE_HARDWARE_FEEDBACK,
            )
            true
        } catch (_: Exception) {
            false
        }
    }

    /** Subtle tactile tick (for drag lock engagement or fine notch) */
    fun dragEngage(view: View? = null) {
        if (!enabled) return
        if (vibrator != null && vibrator.hasVibrator()) {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                try {
                    vibrator.vibrate(VibrationEffect.createPredefined(VibrationEffect.EFFECT_TICK))
                    return
                } catch (_: Exception) {}
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                try {
                    val comp = VibrationEffect.startComposition()
                        .addPrimitive(VibrationEffect.Composition.PRIMITIVE_TICK, 0.85f)
                        .compose()
                    vibrator.vibrate(comp)
                    return
                } catch (_: Exception) {}
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                try {
                    vibrator.vibrate(VibrationEffect.createOneShot(5, 140))
                    return
                } catch (_: Exception) {}
            }
        }
        view?.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
    }

    /** Soft milestone pulse for 4-finger Mission Control / Spaces swipe switch */
    fun swipeCommit(view: View? = null) {
        if (!enabled) return
        if (vibrator != null && vibrator.hasVibrator()) {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                try {
                    val comp = VibrationEffect.startComposition()
                        .addPrimitive(VibrationEffect.Composition.PRIMITIVE_SLOW_RISE, 0.6f)
                        .addPrimitive(VibrationEffect.Composition.PRIMITIVE_CLICK, 0.9f)
                        .compose()
                    vibrator.vibrate(comp)
                    return
                } catch (_: Exception) {}
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                try {
                    vibrator.vibrate(VibrationEffect.createPredefined(VibrationEffect.EFFECT_DOUBLE_CLICK))
                    return
                } catch (_: Exception) {}
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                try {
                    vibrator.vibrate(VibrationEffect.createWaveform(longArrayOf(0, 8, 20, 10), intArrayOf(0, 140, 0, 200), -1))
                    return
                } catch (_: Exception) {}
            }
        }
        heavyClick(view)
    }

    @Suppress("DEPRECATION")
    private fun vibrate(effect: VibrationEffect, usage: Int) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            vibrator?.vibrate(effect, VibrationAttributes.createForUsage(usage))
        } else {
            vibrator?.vibrate(
                effect,
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_ASSISTANCE_SONIFICATION)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                    .build(),
            )
        }
    }
}
