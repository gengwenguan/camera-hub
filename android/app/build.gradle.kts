plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "site.gwghome.camerahub"
    compileSdk = 35

    defaultConfig {
        applicationId = "site.gwghome.camerahub"
        // The bundled ONNX Runtime release is built for Android API 27.
        minSdk = 27
        targetSdk = 35
        versionCode = 33
        versionName = "0.5.27"
        ndk { abiFilters += "arm64-v8a" }
    }

    sourceSets["main"].jniLibs.srcDir(layout.buildDirectory.dir("generated/native"))
    sourceSets["main"].assets.srcDir(layout.buildDirectory.dir("generated/engineAssets"))
    packaging {
        // Android 10+ allows exec from the installer-owned native library directory.
        jniLibs.useLegacyPackaging = true
        jniLibs.keepDebugSymbols += setOf("**/libcamera_hub_exec.so", "**/libffmpeg_exec.so")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    lint {
        abortOnError = true
        warningsAsErrors = true
        // Versions are deliberately pinned; upgrades are separate changes.
        disable += setOf("GradleDependency", "AndroidGradlePluginVersion", "OldTargetApi")
        // Native deployment currently targets ARM64 phones, including MI6.
        disable += "ChromeOsAbiSupport"
    }
}

dependencies {
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20240303")
}

val buildNative by tasks.registering(Exec::class) {
    workingDir(rootDir)
    commandLine("python3", "build-native.py")
    inputs.files(rootDir.resolve("build-native.py"), rootDir.resolve("../Cargo.toml"),
        rootDir.resolve("../Cargo.lock"), rootDir.resolve("../build.rs"))
    inputs.dir(rootDir.resolve("../src"))
    inputs.dir(rootDir.resolve("../vendor"))
    inputs.dir(rootDir.resolve("../web"))
    outputs.dir(layout.buildDirectory.dir("generated/native"))
    outputs.dir(layout.buildDirectory.dir("generated/engineAssets"))
}
tasks.named("preBuild") { dependsOn(buildNative) }
