package com.mtc.touchpad

import java.util.Locale

internal object I18n {
    var overrideLanguage: String? = null

    val isZh: Boolean
        get() {
            val lang = overrideLanguage ?: Locale.getDefault().language.lowercase()
            return lang.startsWith("zh")
        }

    fun tr(en: String, zh: String): String = if (isZh) zh else en
}
