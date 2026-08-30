package com.mtc.touchpad

import java.util.Locale

internal object I18n {
    val isZh: Boolean
        get() = Locale.getDefault().language.lowercase().startsWith("zh")

    fun tr(en: String, zh: String): String = if (isZh) zh else en
}
