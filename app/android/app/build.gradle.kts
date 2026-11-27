import java.util.Base64

plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

// What the build was told with `--dart-define`. Flutter hands all of it to
// Gradle as one property: the definitions, each in base64, with commas
// between them.
val dartDefines: Map<String, String> =
    ((project.findProperty("dart-defines") as String?) ?: "")
        .split(",")
        .filter { it.isNotEmpty() }
        .map { String(Base64.getDecoder().decode(it)) }
        .filter { it.contains("=") }
        .associate { it.substringBefore("=") to it.substringAfter("=") }

// The network the app is built for, as the Dart side reads it too
// (lib/core/network.dart): stagenet where nothing was said.
val veilNetwork: String = dartDefines["VEIL_NETWORK"] ?: "stagenet"

// Whether this is the build of a release, which is said the same way
// (`--dart-define=VEIL_RELEASE=yes`) and by one command only:
// `VEIL_NETWORK=mainnet ./check.sh --android release`.
val veilRelease: Boolean = when (val said = dartDefines["VEIL_RELEASE"] ?: "") {
    "yes" -> true
    "" -> false
    else -> throw GradleException("VEIL_RELEASE is '$said'; it is 'yes' or not there")
}

// A build is for trying unless it was said to be a release. One for
// trying is signed with the key of the machine that made it and is an
// app of its own, by a name of its own, so that it can be on a phone
// beside the released Veil and can never be taken for it or installed in
// its place: Android knows an app by its identifier, and removing an app
// removes its wallet. A release carries the plain identifier and the
// plain name, is for the main network, and leaves the command that
// builds it without a signature; the key for releases signs it on the
// owner's machine (native/release.sh).
val (veilIdSuffix, veilLabel) = when {
    veilRelease && veilNetwork == "mainnet" -> "" to "Veil"
    veilRelease -> throw GradleException(
        "a release is for the main network, and this build is for '$veilNetwork'"
    )
    veilNetwork == "stagenet" -> ".stagenet" to "Veil stagenet"
    veilNetwork == "mainnet" -> ".preview" to "Veil preview"
    else -> throw GradleException(
        "VEIL_NETWORK is '$veilNetwork'; the app is built for mainnet or for stagenet"
    )
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
        // have the app: another name is another app. The owner settled on
        // this one (docs/concept.md, "Decided for the release"); a build
        // for trying adds to it, as said above.
        applicationId = "io.github.xmstarter.veil"
        applicationIdSuffix = veilIdSuffix.takeIf { it.isNotEmpty() }
        // The name under the icon, which the manifest takes from here.
        manifestPlaceholders["appName"] = veilLabel
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
            // app was signed with. The build of a release is signed the
            // same way here, and that signature is taken off again by the
            // command that builds it (native/check.sh, `release`): the
            // key for releases is nowhere near a build.
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
