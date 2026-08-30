package com.mtc.touchpad

import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.atomic.AtomicLong
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
    @Volatile private var probeSocket: Socket? = null
    @Volatile private var probeDatagramSocket: DatagramSocket? = null
    @Volatile var target: InetAddress? = null; private set
    @Volatile private var token: String? = null
    private val connectGeneration = AtomicLong(0L)

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
        connect(host, port, token, true, listener)
    }

    /**
     * Connect to the phone UDP transport. Web-enabled endpoints use the HTTP
     * health route; UDP-only endpoints use the authenticated datagram probe.
     */
    fun connect(host: String, port: Int, token: String?, probeWeb: Boolean, listener: Listener) {
        val generation = connectGeneration.incrementAndGet()
        handler.post {
            if (!isCurrent(generation)) return@post
            listener.onState(false, "连接中…")
            var nextSocket: DatagramSocket? = null
            try {
                if (!isCurrent(generation)) return@post
                closeSocket()
                val addr = InetAddress.getByName(host)
                val normalizedToken = token?.takeIf { it.isNotEmpty() }
                val probe = if (probeWeb) {
                    val webProbe = probeService(addr, port, normalizedToken, generation)
                    // A manual address may point at a phone-only Mac. Keep
                    // the address validation strict by falling back to the
                    // authenticated UDP probe only when TCP is absent; an
                    // HTTP 401 remains a real Token error.
                    if (!webProbe.reachable) {
                        probeUdp(addr, port, normalizedToken, generation)
                    } else {
                        webProbe
                    }
                } else {
                    probeUdp(addr, port, normalizedToken, generation)
                }
                if (!isCurrent(generation)) return@post
                if (!probe.reachable) {
                    error("Mac 服务不可达")
                }
                if (!probe.authenticated) {
                    error(probe.message ?: "Token 无效")
                }
                if (!isCurrent(generation)) return@post
                nextSocket = DatagramSocket().apply { trafficClass = 0x10 /* IPTOS_LOWDELAY */ }
                if (!isCurrent(generation)) {
                    nextSocket.close()
                    return@post
                }
                socket = nextSocket
                target = addr
                this.portField = port
                this.token = normalizedToken
                listener.onState(true, "已连接 $host")
            } catch (e: Exception) {
                runCatching { nextSocket?.close() }
                if (!isCurrent(generation)) return@post
                socket = null; target = null; this.token = null
                listener.onState(false, "连接失败：${e.message}")
            }
        }
    }

    private data class ProbeResult(
        val reachable: Boolean,
        val authenticated: Boolean,
        val message: String? = null,
    )

    /**
     * Probe the existing companion-net TCP listener before exposing a UDP
     * session as connected. A 404 is accepted only for legacy no-token
     * daemons, whose older binaries do not know /health but still serve HTTP.
     */
    private fun probeService(addr: InetAddress, port: Int, token: String?, generation: Long): ProbeResult {
        val socket = Socket()
        probeSocket = socket
        return try {
            socket.soTimeout = PROBE_TIMEOUT_MS
            socket.connect(InetSocketAddress(addr, port), PROBE_TIMEOUT_MS)
            val request = buildString {
                append("GET /health HTTP/1.1\r\n")
                append("Host: ").append(addr.hostAddress).append("\r\n")
                append("Connection: close\r\n")
                if (!token.isNullOrEmpty()) append("Authorization: Bearer ").append(token).append("\r\n")
                append("\r\n")
            }
            // Do not close the output stream here. On Android's Socket
            // implementation closing it also closes the underlying socket,
            // which would discard the response before we can read the status.
            val output = socket.getOutputStream()
            output.write(request.toByteArray(Charsets.US_ASCII))
            output.flush()
            val status = socket.getInputStream().bufferedReader(Charsets.US_ASCII).use { reader ->
                reader.readLine()?.trim()?.split(' ')?.getOrNull(1)?.toIntOrNull()
            }
            when {
                status == 200 -> ProbeResult(reachable = true, authenticated = true)
                status == 401 -> ProbeResult(reachable = true, authenticated = false, message = "Token 无效")
                status in setOf(404, 405) && token.isNullOrEmpty() -> ProbeResult(reachable = true, authenticated = true)
                status in setOf(404, 405) -> ProbeResult(reachable = true, authenticated = false, message = "Mac 服务不支持 Token 握手")
                else -> ProbeResult(reachable = true, authenticated = false, message = "Mac 服务响应异常")
            }
        } catch (_: Exception) {
            ProbeResult(reachable = false, authenticated = false)
        } finally {
            if (probeSocket === socket) probeSocket = null
            runCatching { socket.close() }
        }
    }

    private fun probeUdp(addr: InetAddress, port: Int, token: String?, generation: Long): ProbeResult {
        val socket = DatagramSocket()
        probeDatagramSocket = socket
        return try {
            socket.soTimeout = PROBE_TIMEOUT_MS
            val body = if (token.isNullOrEmpty()) {
                UDP_PROBE_MAGIC
            } else {
                UDP_PROBE_MAGIC + FrameEncoder.authenticate(token, ByteArray(0))
            }
            socket.send(DatagramPacket(body, body.size, addr, port))
            val response = ByteArray(8)
            val packet = DatagramPacket(response, response.size)
            socket.receive(packet)
            if (packet.address == addr && packet.port == port && packet.length >= UDP_PROBE_ACK.size && response.copyOfRange(0, UDP_PROBE_ACK.size).contentEquals(UDP_PROBE_ACK)) {
                ProbeResult(reachable = true, authenticated = true)
            } else {
                ProbeResult(reachable = true, authenticated = false, message = "Mac 服务响应异常")
            }
        } catch (_: java.net.SocketTimeoutException) {
            ProbeResult(reachable = false, authenticated = false)
        } catch (_: Exception) {
            ProbeResult(reachable = false, authenticated = false)
        } finally {
            if (probeDatagramSocket === socket) probeDatagramSocket = null
            runCatching { socket.close() }
        }
    }

    private fun isCurrent(generation: Long): Boolean = connectGeneration.get() == generation

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
        connectGeneration.incrementAndGet()
        runCatching { probeSocket?.close() }
        runCatching { probeDatagramSocket?.close() }
        handler.post { closeSocket() }
        thread.quitSafely()
    }

    /** Interrupt an in-flight DNS/TCP/UDP probe without terminating the sender. */
    fun cancelConnect() {
        connectGeneration.incrementAndGet()
        runCatching { probeSocket?.close() }
        runCatching { probeDatagramSocket?.close() }
        handler.post { closeSocket() }
    }

    private fun closeSocket() {
        runCatching { socket?.close() }
        socket = null; target = null; token = null
    }

    companion object {
        private const val PROBE_TIMEOUT_MS = 900
        private val UDP_PROBE_MAGIC = byteArrayOf(0x41, 0x54, 0x51, 0x31) // ATQ1
        private val UDP_PROBE_ACK = byteArrayOf(0x41, 0x54, 0x41, 0x31) // ATA1
    }
}
