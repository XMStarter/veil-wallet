plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

android {
    namespace = "io.github.xmstarter.veil"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        // The name Android knows the app by. It cannot change once people
        // have the app: another name is another app. It is taken from the
        // project's place on GitHub and is to be settled before a release;
        // see "Open decisions" in docs/concept.md.
        applicationId = "io.github.xmstarter.veil"
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        // Uses the version code from pubspec.yaml. When using split APKs, 1000 * ABI_VERSION
        // is added automatically by Flutter. (https://developer.android.com/studio/build/configure-apk-splits#configure-APK-versions)
        // You can force using the value of versionCode by specifying the `-P force-version-code-ignoring-abi=true`
        // flag during build.
        versionCode = flutter.versionCode
        versionName = flutter.versionName
        // The core and the wallet library exist for 64-bit ARM only, so
        // the package holds nothing for any other processor.
        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    // The transport program for bridges is a program, not a library, and
    // Android lets an app start a program only from the folder it unpacks
    // a package's native parts into. So they are unpacked when the app is
    // installed, which this asks for, where Android would otherwise use
    // them from inside the package; and the program is named like a
    // library (`liblyrebird.so`), because nothing else is unpacked.
    packaging {
        jniLibs {
            useLegacyPackaging = true
            // The program goes into the package as it was built, byte for
            // byte: the build's tool for trimming libraries is made for
            // libraries, and this is a program written in Go.
            keepDebugSymbols += "**/liblyrebird.so"
        }
    }

    buildTypes {
        release {
            // Signed with the key Android's tools make on the machine that
            // builds, which is enough to install the app on one's own
            // phone and for nothing else: every machine has another key,
            // and Android takes an update only from the key the installed
            // app was signed with. A release key belongs to the release
            // step.
            signingConfig = signingConfigs.getByName("debug")
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

dependencies {
    // Reads the code of an address with the camera, on the device. It is
    // ZXing, open source under the Apache licence, with the screen that
    // shows the camera's picture; nothing of Google's services is in it.
    implementation("com.journeyapps:zxing-android-embedded:4.3.0")
}
