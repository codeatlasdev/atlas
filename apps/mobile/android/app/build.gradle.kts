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

// Atlas Lab (`bun run mobile:lab`, docs/mobile.md "Lab lane"): `-Platlab=1` (flutter's
// `--android-project-arg=atlas.lab=1`) installs a second app beside the real one: its own application
// id (so its storage and permissions are its own), its name and an orange icon (src/lab/res, debug builds). Without
// the property nothing changes: same id, name, icon and output paths as before, so the release
// lanes (fastlane, CI) need no flavor. No push or Firebase is configured for the app, so the lab has none.
val atlasLab = project.findProperty("atlas.lab") == "1"

android {
    namespace = "br.com.codeatlas.atlas.mobile"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // AGP 9 turns generated resource values off; the app name below is one.
    buildFeatures {
        resValues = true
    }

    defaultConfig {
        applicationId = "br.com.codeatlas.atlas.mobile" + if (atlasLab) ".lab" else ""
        resValue("string", "app_name", if (atlasLab) "Atlas Lab" else "Atlas")
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        // From pubspec.yaml; the beta lane overrides the build number per upload.
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    sourceSets {
        if (atlasLab) {
            // A build type's resources override main's of the same name (inside main they would be a
            // duplicate error): the lab is always a debug build, so its orange launcher icons ride on `debug`.
            getByName("debug").res.srcDir("src/lab/res")
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
