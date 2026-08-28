package com.mtc.touchpad

import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import kotlin.random.Random

/**
 * Sends encoded frames over UDP from a dedicated thread; the UI thread
 * never blocks on networking. Frames are fire-and-forget — realtime
 * touch beats guaranteed delivery, and gesture-end echoes (the only
 * stateful transition) are tripled by the caller.
 */
class UdpSender {

    interface Listener {
        fun onState(connected: Boolean, message: String)
    }

    private val thread = HandlerThread("udp-sender").apply { start() }
    private val handler = Handler(thread.looper)

    @Volatile private var socket: DatagramSocket? = null
    @Volatile var target: InetAddress? = null; private set
    @Volatile private var token: String? = null

    /** Random session start per protocol docs; wraps in Int space. */
    private val seq = Random.nextInt()

    fun nextSeq(): Int = seqUpdater.updateAndGet { it + 1 } - 1 + seqBase()
    private val seqUpdater = java.util.concurrent.atomic.AtomicInteger(0)
    private fun seqBase(): Int = seq

    /** 100 µs ticks since boot, like every other sender of this protocol. */
    fun nowTicks(): Int = (SystemClock.elapsedRealtimeNanos() / 100_000L).toInt()

    /** Backward-compatible unauthenticated connection entry point. */
    fun connect(host: String, port: Int, listener: Listener) {
        connect(host, port, null, listener)
    }

    fun connect(host: String, port: Int, token: String? = null, listener: Listener) {
        handler.post {
            try {
                closeSocket()
                val addr = InetAddress.getByName(host)
                val sock = DatagramSocket().apply { trafficClass = 0x10 /* IPTOS_LOWDELAY */ }
                socket = sock
                target = addr
                this.portField = port
                this.token = token?.takeIf { it.isNotEmpty() }
                listener.onState(true, "已连接 $host")
            } catch (e: Exception) {
                socket = null; target = null; this.token = null
                listener.onState(false, "连接失败：${e.message}")
            }
        }
    }

    @Volatile var portField: Int = 4242; private set

    fun send(bytes: ByteArray) {
        handler.post {
            val sock = socket ?: return@post
            val addr = target ?: return@post
            try {
                val payload = token?.let { FrameEncoder.authenticate(it, bytes) } ?: bytes
                sock.send(DatagramPacket(payload, payload.size, addr, portField))
            } catch (_: Exception) {
                // Dropped mid-stream: next frame self-corrects. Lift loss
                // is covered by echoLift() retransmits at the source.
            }
        }
    }

    fun close() {
        handler.post {
            closeSocket()
            thread.quitSafely()
        }
    }

    private fun closeSocket() {
        runCatching { socket?.close() }
        socket = null; target = null; token = null
    }
}
