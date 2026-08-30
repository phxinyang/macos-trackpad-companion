package com.mtc.touchpad

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.net.wifi.WifiManager
import java.net.InetAddress
import java.util.concurrent.CopyOnWriteArrayList

/**
 * Discovers Trackpad Companion services on the local network. Discovery is a
 * convenience layer only: callers can always fall back to a manually entered
 * host and port when multicast DNS is blocked by the Wi-Fi network.
 */
class MacDiscovery(context: Context, private val listener: Listener) {
    data class MacEndpoint(
        val name: String,
        val host: InetAddress,
        val port: Int,
        val authentication: String,
        val serviceId: String,
        val webEnabled: Boolean = true,
        val phoneEnabled: Boolean = true,
    )

    interface Listener {
        fun onDiscoveryChanged(endpoints: List<MacEndpoint>)
        fun onDiscoveryError(message: String)
    }

    private val manager = context.getSystemService(Context.NSD_SERVICE) as NsdManager
    private val multicastLock = (context.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager)
        .createMulticastLock("trackpad-companion-mdns")
    private val endpoints = CopyOnWriteArrayList<MacEndpoint>()
    @Volatile private var started = false

    private val discoveryListener = object : NsdManager.DiscoveryListener {
        override fun onDiscoveryStarted(serviceType: String) { started = true }
        override fun onDiscoveryStopped(serviceType: String) {
            started = false
            releaseMulticastLock()
        }
        override fun onServiceFound(serviceInfo: NsdServiceInfo) {
            if (serviceInfo.serviceType.trimEnd('.') == SERVICE_TYPE.trimEnd('.')) {
                runCatching { manager.resolveService(serviceInfo, resolveListener) }
                    .onFailure { listener.onDiscoveryError(I18n.tr("Failed to resolve Mac service: ${it.message ?: "Unknown error"}", "解析 Mac 服务失败：${it.message ?: "未知错误"}")) }
            }
        }
        override fun onServiceLost(serviceInfo: NsdServiceInfo) {
            endpoints.removeAll { it.name == serviceInfo.serviceName }
            listener.onDiscoveryChanged(endpoints.toList())
        }
        override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {
            started = false
            runCatching { manager.stopServiceDiscovery(this) }
            releaseMulticastLock()
            listener.onDiscoveryError(I18n.tr("Cannot search nearby Macs (error $errorCode)", "无法搜索附近的 Mac（错误 $errorCode）"))
        }
        override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {
            started = false
            runCatching { manager.stopServiceDiscovery(this) }
        }
    }

    private val resolveListener = object : NsdManager.ResolveListener {
        override fun onServiceResolved(serviceInfo: NsdServiceInfo) {
            val host = serviceInfo.host ?: return
            val attrs = serviceInfo.attributes
            val endpoint = MacEndpoint(
                name = serviceInfo.serviceName,
                host = host,
                port = serviceInfo.port,
                authentication = attrs["auth"]?.toString(Charsets.UTF_8) ?: "none",
                serviceId = attrs["id"]?.toString(Charsets.UTF_8) ?: "",
                webEnabled = attrs["web"]?.toString(Charsets.UTF_8) != "0",
                phoneEnabled = attrs["phone"]?.toString(Charsets.UTF_8) != "0",
            )
            endpoints.removeAll { it.name == endpoint.name }
            endpoints += endpoint
            listener.onDiscoveryChanged(endpoints.toList())
        }

        override fun onResolveFailed(serviceInfo: NsdServiceInfo, errorCode: Int) {
            listener.onDiscoveryError(I18n.tr("Failed to resolve ${serviceInfo.serviceName} (error $errorCode)", "解析 ${serviceInfo.serviceName} 失败（错误 $errorCode）"))
        }
    }

    fun start() {
        if (started) return
        runCatching { if (!multicastLock.isHeld) multicastLock.acquire() }
            .onFailure {
                listener.onDiscoveryError(I18n.tr("Cannot acquire local discovery lock: ${it.message ?: "Unknown error"}", "无法取得局域网搜索权限：${it.message ?: "未知错误"}"))
                return
            }
        runCatching {
            manager.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, discoveryListener)
        }.onFailure {
            releaseMulticastLock()
            listener.onDiscoveryError(I18n.tr("Cannot start local discovery: ${it.message ?: "Unknown error"}", "无法启动局域网搜索：${it.message ?: "未知错误"}"))
        }
    }

    fun stop() {
        if (started) runCatching { manager.stopServiceDiscovery(discoveryListener) }
        started = false
        releaseMulticastLock()
    }

    private fun releaseMulticastLock() {
        if (multicastLock.isHeld) runCatching { multicastLock.release() }
    }

    fun snapshot(): List<MacEndpoint> = endpoints.toList()

    companion object {
        const val SERVICE_TYPE = "_mtc-trackpad._tcp."
    }
}
