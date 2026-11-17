import java.util.Properties

plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

// Play upload key: android/key.properties (local, git-ignored) or, in CI, written from secrets
// by fastlane (`fastlane/Fastfile`). Without it a release build is signed with the debug key,
// which `flutter run --release` needs and Play refuses.
val upload = Properties().apply {
    rootProject.file("key.properties").takeIf { it.exists() }?.inputStream()?.use { load(it) }
}

android {
    namespace = "br.com.codeatlas.atlas.mobile"
    // permission_handler_android needs to compile against API 37 (Flutter 3.50 defaults to 36). Only the
    // APIs the code may reference: targetSdk (runtime behaviour) stays Flutter's.
    compileSdk = maxOf(flutter.compileSdkVersion, 37)
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        applicationId = "br.com.codeatlas.atlas.mobile"
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        // From pubspec.yaml; the beta lane overrides the build number per upload.
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    // Two apps from one codebase (flutter's `--flavor`, docs/mobile.md "Dev and store apps"). `dev` is
    // the default flavor (pubspec.yaml `default-flavor`): own id so its storage and permissions are its
    // own, the name in src/dev/res/values and the orange icon in src/dev/res, on every build type. `prod`
    // is the store app. The names are plain resources (AGP 9 turns generated resValue()s off).
    flavorDimensions += "env"
    productFlavors {
        create("dev") {
            dimension = "env"
            applicationIdSuffix = ".dev"
        }
        create("prod") {
            dimension = "env"
        }
    }

    signingConfigs {
        if (upload.isNotEmpty()) {
            create("upload") {
                storeFile = file(upload.getProperty("storeFile"))
                storePassword = upload.getProperty("storePassword")
                keyAlias = upload.getProperty("keyAlias")
                keyPassword = upload.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            signingConfig =
                signingConfigs.findByName("upload") ?: signingConfigs.getByName("debug")
            // R8 + resource shrinking are already on for release (Flutter's Gradle plugin turns them on);
            // this adds the native symbol table to the app bundle, so Play can symbolize a crash in
            // libatlas_mobile_native.so (the APK itself is stripped; users do not download the symbols).
            ndk {
                debugSymbolLevel = "SYMBOL_TABLE"
            }
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}

// Apply the google-services plugin only when the config file is present.
// This lets the app build without Firebase config (push is disabled).
if (file("google-services.json").exists()) {
    apply(plugin = "com.google.gms.google-services")
}
