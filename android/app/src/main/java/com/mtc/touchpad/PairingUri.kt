package com.mtc.touchpad

import java.net.URI
import java.net.URLDecoder

data class PairingTarget(
    val host: String,
    val port: Int,
    val token: String?,
    val webEnabled: Boolean = true,
    val phoneEnabled: Boolean = true,
)

object PairingUri {
    private const val MAX_TOKEN_BYTES = 256

    fun parse(raw: String?): PairingTarget? {
        if (raw.isNullOrBlank()) return null
        val normalized = raw.trim()
        val uri = runCatching { URI(normalized) }.getOrNull() ?: return null
        if (uri.scheme?.lowercase() != "mtc" || uri.host?.lowercase() != "pair") return null
        val params = uri.rawQuery.orEmpty().split('&')
            .mapNotNull { item ->
                val parts = item.split('=', limit = 2)
                if (parts.size != 2) null else {
                    val key = runCatching { URLDecoder.decode(parts[0], "UTF-8") }.getOrNull() ?: return@mapNotNull null
                    val value = runCatching { URLDecoder.decode(parts[1], "UTF-8") }.getOrNull() ?: return@mapNotNull null
                    key to value
                }
            }
            .toMap()
        val host = params["host"]?.trim().orEmpty()
        if (host.isEmpty() || host.any { it.isWhitespace() }) return null
        val port = params["port"]?.toIntOrNull() ?: return null
        if (port !in 1..65535) return null
        val token = params["token"]?.takeIf { it.isNotEmpty() }
        if (token != null && token.toByteArray(Charsets.UTF_8).size > MAX_TOKEN_BYTES) return null
        val webEnabled = params["web"]?.let { it != "0" } ?: true
        val phoneEnabled = params["phone"]?.let { it != "0" } ?: true
        return PairingTarget(host, port, token, webEnabled, phoneEnabled)
    }
}
