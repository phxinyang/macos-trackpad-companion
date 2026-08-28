plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.mtc.touchpad"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.mtc.touchpad"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}

dependencies {
    testImplementation("junit:junit:4.13.2")
    // MIT: QWEA0/Liquid-Glass-Android View library. The API 33+ path is a
    // real SDF/AGSL lens with backdrop capture, refraction and dispersion;
    // older devices use the library's blur/scrim fallback.
    implementation("com.github.QWEA0:liquidglass:v2.0.2")
}
