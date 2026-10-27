import java.io.File

plugins { id("com.android.application"); id("org.jetbrains.kotlin.android") }
val releaseSigning = listOf("WALLTA_ANDROID_KEYSTORE", "WALLTA_ANDROID_STORE_PASSWORD", "WALLTA_ANDROID_KEY_ALIAS", "WALLTA_ANDROID_KEY_PASSWORD").associateWith { System.getenv(it) }
val hasReleaseSigning = releaseSigning.values.all { !it.isNullOrBlank() }
gradle.taskGraph.whenReady {
    if (allTasks.any { it.project == project && it.name.contains("Release", ignoreCase = true) } && !hasReleaseSigning) {
        throw GradleException("Release signing requires WALLTA_ANDROID_KEYSTORE, WALLTA_ANDROID_STORE_PASSWORD, WALLTA_ANDROID_KEY_ALIAS and WALLTA_ANDROID_KEY_PASSWORD. Debug signing is never used for release.")
    }
}
android {
    buildFeatures { buildConfig = true }
    if (hasReleaseSigning) {
        signingConfigs.create("stableRelease") {
            val keystore = File(releaseSigning.getValue("WALLTA_ANDROID_KEYSTORE")!!)
            require(keystore.isAbsolute && keystore.isFile) { "WALLTA_ANDROID_KEYSTORE must be an existing absolute keystore path" }
            storeFile = keystore
            storePassword = releaseSigning.getValue("WALLTA_ANDROID_STORE_PASSWORD")
            keyAlias = releaseSigning.getValue("WALLTA_ANDROID_KEY_ALIAS")
            keyPassword = releaseSigning.getValue("WALLTA_ANDROID_KEY_PASSWORD")
        }
    }
    namespace = "app.wallta"
    compileSdk = 35
    defaultConfig { applicationId = "app.wallta"; minSdk = 26; targetSdk = 35; versionCode = 3; versionName = "0.1.2" }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
    kotlinOptions { jvmTarget = "17" }
    buildTypes { release { isMinifyEnabled = false; if (hasReleaseSigning) signingConfig = signingConfigs.getByName("stableRelease") } }
    testOptions {
        unitTests.isIncludeAndroidResources = true
        unitTests.all {
            it.jvmArgs("--add-opens=java.base/java.lang=ALL-UNNAMED", "--add-opens=java.base/java.util=ALL-UNNAMED",
                "--add-opens=java.base/java.io=ALL-UNNAMED", "--add-opens=java.base/java.net=ALL-UNNAMED",
                "--add-opens=java.base/java.security=ALL-UNNAMED", "--add-opens=java.base/java.text=ALL-UNNAMED",
                "--add-opens=java.base/jdk.internal.access=ALL-UNNAMED", "--add-opens=java.desktop/java.awt.font=ALL-UNNAMED",
                "--add-opens=jdk.compiler/com.sun.tools.javac.api=ALL-UNNAMED")
        }
    }
}
dependencies {
    implementation("androidx.webkit:webkit:1.13.0")
    implementation("androidx.core:core:1.15.0")
    implementation("androidx.work:work-runtime-ktx:2.10.1")
    implementation("androidx.documentfile:documentfile:1.0.1")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20240303")
    testImplementation("org.robolectric:robolectric:4.17")
}
