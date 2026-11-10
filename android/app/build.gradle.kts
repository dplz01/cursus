plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
}

// Version from the Cargo workspace, so every Cursus app reports the same one.
val cursusVersion: String = rootDir.resolve("../Cargo.toml").readLines()
    .first { it.startsWith("version = ") }.substringAfter('"').substringBefore('"')

android {
    namespace = "chat.cursus"
    compileSdk = 35

    defaultConfig {
        applicationId = "chat.cursus.android"
        minSdk = 26
        targetSdk = 35
        versionName = cursusVersion
        versionCode = cursusVersion.split('.').fold(0) { acc, part -> acc * 100 + part.toInt() }
        // The official SimpleX core is published for 64-bit ARM only.
        ndk { abiFilters += "arm64-v8a" }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            // Until release signing is set up: installable debug-signed builds.
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }

    buildFeatures { compose = true }

    packaging {
        // libsimplex is loaded by path from nativeLibraryDir (cursus-core
        // dlopens it), so the libraries must be extracted on install.
        jniLibs { useLegacyPackaging = true }
    }
}

// The Rust side (cursus-core + bridge + JNI) for arm64, via cargo-ndk.
val cargoBuild by tasks.registering(Exec::class) {
    group = "build"
    description = "Build libcursus_android.so with cargo-ndk"
    workingDir = rootDir.resolve("..")
    val profile = if (gradle.startParameter.taskNames.any { it.contains("Release", ignoreCase = true) }) "--release" else null
    // cargo-ndk hard-links the library into jniLibs and keeps an existing
    // file, so a debug build's copy would survive into a release APK.
    doFirst { delete(rootDir.resolve("app/src/main/jniLibs/arm64-v8a/libcursus_android.so")) }
    commandLine(listOfNotNull(
        "cargo", "ndk", "-t", "arm64-v8a", "--platform", "26",
        "-o", rootDir.resolve("app/src/main/jniLibs").absolutePath,
        "build", profile, "-p", "cursus-android", "--locked",
    ))
}
// The official SimpleX core (pinned tag + SHA-256); a no-op once it is in
// jniLibs, so a wiped jniLibs can't produce an APK without libsimplex.so.
val fetchLibsimplex by tasks.registering(Exec::class) {
    group = "build"
    description = "Fetch libsimplex.so from the pinned official SimpleX APK"
    commandLine(rootDir.resolve("fetch-libsimplex.sh").absolutePath)
}
tasks.named("preBuild") { dependsOn(cargoBuild, fetchLibsimplex) }

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.process)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    debugImplementation(libs.compose.ui.tooling)
    implementation(libs.kotlinx.serialization.json)
    testImplementation("junit:junit:4.13.2")
}
