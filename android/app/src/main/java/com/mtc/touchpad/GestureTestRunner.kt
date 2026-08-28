package com.mtc.touchpad

import java.util.concurrent.Executors
import kotlin.math.cos
import kotlin.math.sin

/**
 * Simulates high-precision macOS multi-touch gestures directly from the Android phone
 * by emitting ATP1 contact coordinate streams over UDP.
 */
object GestureTestRunner {

    private val executor = Executors.newSingleThreadExecutor()

    fun runSmartZoom(sender: UdpSender, onDone: (() -> Unit)? = null) {
        executor.execute {
            val c1 = 1
            val c2 = 2
            val baseX1 = 40f
            val baseX2 = 55f
            val baseY = 50f

            // First tap down (50ms)
            sendContacts(sender, listOf(
                FrameEncoder.Contact(c1, baseX1, baseY),
                FrameEncoder.Contact(c2, baseX2, baseY)
            ))
            sleep(20)
            sendContacts(sender, listOf(
                FrameEncoder.Contact(c1, baseX1, baseY),
                FrameEncoder.Contact(c2, baseX2, baseY)
            ))
            sleep(25)
            // First lift (70ms gap)
            sendLift(sender)
            sleep(70)

            // Second tap down (50ms)
            sendContacts(sender, listOf(
                FrameEncoder.Contact(c1, baseX1, baseY),
                FrameEncoder.Contact(c2, baseX2, baseY)
            ))
            sleep(20)
            sendContacts(sender, listOf(
                FrameEncoder.Contact(c1, baseX1, baseY),
                FrameEncoder.Contact(c2, baseX2, baseY)
            ))
            sleep(25)
            // Final lift
            sendLift(sender)
            onDone?.invoke()
        }
    }

    fun runLookup(sender: UdpSender, onDone: (() -> Unit)? = null) {
        executor.execute {
            val c1 = 1; val c2 = 2; val c3 = 3
            val y = 50f
            sendContacts(sender, listOf(
                FrameEncoder.Contact(c1, 35f, y),
                FrameEncoder.Contact(c2, 50f, y),
                FrameEncoder.Contact(c3, 65f, y)
            ))
            sleep(25)
            sendContacts(sender, listOf(
                FrameEncoder.Contact(c1, 35f, y),
                FrameEncoder.Contact(c2, 50f, y),
                FrameEncoder.Contact(c3, 65f, y)
            ))
            sleep(35)
            sendLift(sender)
            onDone?.invoke()
        }
    }

    fun runSwipeLeft(sender: UdpSender, onDone: (() -> Unit)? = null) {
        executor.execute {
            val ids = listOf(1, 2, 3, 4)
            val baseXs = listOf(40f, 52f, 64f, 76f)
            val y = 50f
            val steps = 8
            val totalDx = -25f

            for (i in 0..steps) {
                val progress = i.toFloat() / steps
                val currentDx = totalDx * (progress * progress)
                val contacts = ids.mapIndexed { idx, id ->
                    FrameEncoder.Contact(id, baseXs[idx] + currentDx, y)
                }
                sendContacts(sender, contacts)
                sleep(16)
            }
            sendLift(sender)
            onDone?.invoke()
        }
    }

    fun runSwipeRight(sender: UdpSender, onDone: (() -> Unit)? = null) {
        executor.execute {
            val ids = listOf(1, 2, 3, 4)
            val baseXs = listOf(25f, 37f, 49f, 61f)
            val y = 50f
            val steps = 8
            val totalDx = 25f

            for (i in 0..steps) {
                val progress = i.toFloat() / steps
                val currentDx = totalDx * (progress * progress)
                val contacts = ids.mapIndexed { idx, id ->
                    FrameEncoder.Contact(id, baseXs[idx] + currentDx, y)
                }
                sendContacts(sender, contacts)
                sleep(16)
            }
            sendLift(sender)
            onDone?.invoke()
        }
    }

    fun runSwipeUp(sender: UdpSender, onDone: (() -> Unit)? = null) {
        executor.execute {
            val ids = listOf(1, 2, 3, 4)
            val xs = listOf(30f, 42f, 54f, 66f)
            val baseY = 65f
            val steps = 8
            val totalDy = -25f

            for (i in 0..steps) {
                val progress = i.toFloat() / steps
                val currentDy = totalDy * (progress * progress)
                val contacts = ids.mapIndexed { idx, id ->
                    FrameEncoder.Contact(id, xs[idx], baseY + currentDy)
                }
                sendContacts(sender, contacts)
                sleep(16)
            }
            sendLift(sender)
            onDone?.invoke()
        }
    }

    fun runSwipeDown(sender: UdpSender, onDone: (() -> Unit)? = null) {
        executor.execute {
            val ids = listOf(1, 2, 3, 4)
            val xs = listOf(30f, 42f, 54f, 66f)
            val baseY = 35f
            val steps = 8
            val totalDy = 25f

            for (i in 0..steps) {
                val progress = i.toFloat() / steps
                val currentDy = totalDy * (progress * progress)
                val contacts = ids.mapIndexed { idx, id ->
                    FrameEncoder.Contact(id, xs[idx], baseY + currentDy)
                }
                sendContacts(sender, contacts)
                sleep(16)
            }
            sendLift(sender)
            onDone?.invoke()
        }
    }

    fun runPinchIn(sender: UdpSender, onDone: (() -> Unit)? = null) {
        executor.execute {
            val c1 = 1; val c2 = 2
            val midX = 50f; val midY = 50f
            val startDist = 12f
            val endDist = 32f
            val steps = 10

            for (i in 0..steps) {
                val progress = i.toFloat() / steps
                val halfDist = (startDist + (endDist - startDist) * progress) / 2f
                val contacts = listOf(
                    FrameEncoder.Contact(c1, midX - halfDist, midY),
                    FrameEncoder.Contact(c2, midX + halfDist, midY)
                )
                sendContacts(sender, contacts)
                sleep(18)
            }
            sendLift(sender)
            onDone?.invoke()
        }
    }

    fun runPinchOut(sender: UdpSender, onDone: (() -> Unit)? = null) {
        executor.execute {
            val c1 = 1; val c2 = 2
            val midX = 50f; val midY = 50f
            val startDist = 32f
            val endDist = 12f
            val steps = 10

            for (i in 0..steps) {
                val progress = i.toFloat() / steps
                val halfDist = (startDist + (endDist - startDist) * progress) / 2f
                val contacts = listOf(
                    FrameEncoder.Contact(c1, midX - halfDist, midY),
                    FrameEncoder.Contact(c2, midX + halfDist, midY)
                )
                sendContacts(sender, contacts)
                sleep(18)
            }
            sendLift(sender)
            onDone?.invoke()
        }
    }

    fun runRotate(sender: UdpSender, onDone: (() -> Unit)? = null) {
        executor.execute {
            val c1 = 1; val c2 = 2
            val midX = 50f; val midY = 50f
            val radius = 15f
            val steps = 12
            val totalRad = Math.toRadians(90.0).toFloat()

            for (i in 0..steps) {
                val angle = totalRad * (i.toFloat() / steps)
                val dx = radius * cos(angle.toDouble()).toFloat()
                val dy = radius * sin(angle.toDouble()).toFloat()
                val contacts = listOf(
                    FrameEncoder.Contact(c1, midX - dx, midY - dy),
                    FrameEncoder.Contact(c2, midX + dx, midY + dy)
                )
                sendContacts(sender, contacts)
                sleep(18)
            }
            sendLift(sender)
            onDone?.invoke()
        }
    }

    fun runRightClick(sender: UdpSender, onDone: (() -> Unit)? = null) {
        executor.execute {
            val c1 = 1; val c2 = 2
            sendContacts(sender, listOf(
                FrameEncoder.Contact(c1, 40f, 50f),
                FrameEncoder.Contact(c2, 55f, 50f)
            ))
            sleep(40)
            sendLift(sender)
            onDone?.invoke()
        }
    }

    fun runThreeFingerDrag(sender: UdpSender, onDone: (() -> Unit)? = null) {
        executor.execute {
            val ids = listOf(1, 2, 3)
            val baseXs = listOf(35f, 48f, 61f)
            val y = 50f
            val steps = 12
            val totalDx = 25f

            for (i in 0..steps) {
                val progress = i.toFloat() / steps
                val currentDx = totalDx * progress
                val contacts = ids.mapIndexed { idx, id ->
                    FrameEncoder.Contact(id, baseXs[idx] + currentDx, y)
                }
                sendContacts(sender, contacts)
                sleep(18)
            }
            sleep(50)
            sendLift(sender)
            onDone?.invoke()
        }
    }

    /**
     * Simulates a two-finger right-edge swipe to toggle Notification Center.
     * Both fingers start at x >= 28mm (right edge zone) and sweep left by ~12mm.
     */
    fun runNotificationCenter(sender: UdpSender, onDone: (() -> Unit)? = null) {
        executor.execute {
            val c1 = 1; val c2 = 2
            // Start from the right edge (x >= 28mm triggers right_edge_candidate)
            val startX1 = 30f; val startX2 = 34f
            val y1 = 45f; val y2 = 55f
            val steps = 10
            val totalDx = -15f  // sweep left

            for (i in 0..steps) {
                val progress = i.toFloat() / steps
                // Ease-out curve for natural feel
                val eased = 1f - (1f - progress) * (1f - progress)
                val currentDx = totalDx * eased
                val contacts = listOf(
                    FrameEncoder.Contact(c1, startX1 + currentDx, y1),
                    FrameEncoder.Contact(c2, startX2 + currentDx, y2)
                )
                sendContacts(sender, contacts)
                sleep(16)
            }
            sendLift(sender)
            onDone?.invoke()
        }
    }

    /**
     * Simulates a 4-finger radial pinch-in to trigger Launchpad (启动台).
     */
    fun runLaunchpadPinch(sender: UdpSender, onDone: (() -> Unit)? = null) {
        executor.execute {
            val ids = listOf(1, 2, 3, 4)
            val midX = 50f; val midY = 50f
            val startR = 20f
            val endR = 8f
            val steps = 10

            for (i in 0..steps) {
                val progress = i.toFloat() / steps
                val r = startR + (endR - startR) * progress
                val contacts = listOf(
                    FrameEncoder.Contact(ids[0], midX - r, midY - r),
                    FrameEncoder.Contact(ids[1], midX + r, midY - r),
                    FrameEncoder.Contact(ids[2], midX - r, midY + r),
                    FrameEncoder.Contact(ids[3], midX + r, midY + r)
                )
                sendContacts(sender, contacts)
                sleep(16)
            }
            sendLift(sender)
            onDone?.invoke()
        }
    }

    /**
     * Simulates a 4-finger radial spread-out to trigger Show Desktop (显示桌面).
     */
    fun runShowDesktopSpread(sender: UdpSender, onDone: (() -> Unit)? = null) {
        executor.execute {
            val ids = listOf(1, 2, 3, 4)
            val midX = 50f; val midY = 50f
            val startR = 10f
            val endR = 22f
            val steps = 10

            for (i in 0..steps) {
                val progress = i.toFloat() / steps
                val r = startR + (endR - startR) * progress
                val contacts = listOf(
                    FrameEncoder.Contact(ids[0], midX - r, midY - r),
                    FrameEncoder.Contact(ids[1], midX + r, midY - r),
                    FrameEncoder.Contact(ids[2], midX - r, midY + r),
                    FrameEncoder.Contact(ids[3], midX + r, midY + r)
                )
                sendContacts(sender, contacts)
                sleep(16)
            }
            sendLift(sender)
            onDone?.invoke()
        }
    }

    /**
     * Simulates 1-finger Press-and-Hold Drag.
     * Finger rests stationary for 480ms (latches left button), then drags across.
     */
    fun runPressAndHoldDrag(sender: UdpSender, onDone: (() -> Unit)? = null) {
        executor.execute {
            val id = 1
            val startX = 40f; val y = 50f
            // Touchdown and hold stationary for ~480ms
            for (i in 0..24) {
                sendContacts(sender, listOf(FrameEncoder.Contact(id, startX, y)))
                sleep(20)
            }
            // Drag across by +18mm
            val steps = 10
            for (i in 1..steps) {
                val dx = 18f * (i.toFloat() / steps)
                sendContacts(sender, listOf(FrameEncoder.Contact(id, startX + dx, y)))
                sleep(16)
            }
            sendLift(sender)
            onDone?.invoke()
        }
    }

    private fun sendContacts(sender: UdpSender, contacts: List<FrameEncoder.Contact>) {
        val payload = FrameEncoder.encode(sender.nextSeq(), sender.nowTicks(), false, contacts)
        sender.send(payload)
    }

    private fun sendLift(sender: UdpSender) {
        val payload = FrameEncoder.encode(sender.nextSeq(), sender.nowTicks(), false, emptyList())
        for (i in 0..2) {
            sender.send(payload)
        }
    }

    private fun sleep(ms: Long) {
        try {
            Thread.sleep(ms)
        } catch (_: InterruptedException) {
        }
    }
}
