plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// Địa chỉ server gắn sẵn lúc build: biến môi trường MUSE_URL hoặc -PMUSE_URL=...
val museUrl: String = (System.getenv("MUSE_URL")?.takeIf { it.isNotBlank() }
    ?: providers.gradleProperty("MUSE_URL").orNull
    ?: "").trim().trimEnd('/')

// Discord Social SDK (tùy chọn): CI giải nén SDK vào các thư mục này. Thiếu thì app vẫn build, chỉ không có tính năng.
val sdkDir: String = (System.getenv("DISCORD_SDK_DIR") ?: "").trim()
val aarDir: String = (System.getenv("DISCORD_AAR_DIR") ?: "").trim()
val hasSdk = sdkDir.isNotEmpty() && aarDir.isNotEmpty() && file("$sdkDir/include/discordpp.h").exists() && file("libs/discord_partner_sdk.aar").exists()

android {
    namespace = "vn.muse.app"
    compileSdk = 34
    if (hasSdk) ndkVersion = "26.3.11579264"

    defaultConfig {
        applicationId = "vn.muse.app"
        minSdk = 26
        targetSdk = 34
        versionCode = 6
        versionName = "0.8.0"
        buildConfigField("String", "MUSE_URL", "\"${museUrl.replace("\"", "")}\"")
        if (hasSdk) {
            ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a") }
            externalNativeBuild {
                cmake {
                    arguments += listOf(
                        "-DANDROID_STL=c++_static",
                        "-DDISCORD_SDK_DIR=$sdkDir",
                        "-DDISCORD_AAR_DIR=$aarDir",
                        "-DMUSE_NATIVE_DIR=${rootDir.parentFile.resolve("native").absolutePath}"
                    )
                }
            }
        }
    }
    if (hasSdk) {
        externalNativeBuild { cmake { path = file("src/main/cpp/CMakeLists.txt") } }
    }
    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("debug") // đổi sang keystore riêng khi phát hành
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    buildFeatures { buildConfig = true }
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.activity:activity-ktx:1.9.1")
    if (hasSdk) implementation(files("libs/discord_partner_sdk.aar"))
}
