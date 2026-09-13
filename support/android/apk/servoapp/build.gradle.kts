plugins {
    alias(libs.plugins.android.application)
}

// Keep engine branding separate from www and the game's replaceable launcher icon.
val generateRovesBrandAssets by tasks.registering(Sync::class) {
    from(rootProject.file("../../../resources")) {
        include("servo_1024.png", "fonts/MetalMania-Regular.ttf", "fonts/MetalMania-OFL.txt")
        into("assets/roves-brand")
    }
    from(rootProject.file("../../../resources/servo_1024.png")) {
        into("res/drawable")
        rename { "roves_boot_icon.png" }
    }
    into(layout.buildDirectory.dir("generated/rovesBrand"))
}

android {
    sourceSets.getByName("main").assets.srcDir(layout.buildDirectory.dir("generated/rovesBrand/assets"))
    sourceSets.getByName("main").res.srcDir(layout.buildDirectory.dir("generated/rovesBrand/res"))
    compileSdk = 37
    buildToolsVersion = "36.0.0"

    namespace = "org.servo.servoshell"

    defaultConfig {
        applicationId = "org.servo.servoshell"
        minSdk = libs.versions.android.sdk.min.get().toInt()
        targetSdk = 34
        versionCode = generatedVersionCode
        versionName = "0.5.0"

        // Sourced from the game's own web app manifest (`manifest.webmanifest`/
        // `manifest.json`/`site.webmanifest`) by `mach bundle --android --content-dir` --
        // see python/servo/post_build_commands.py's `_bundle_android`/`_read_web_manifest` --
        // or from that command's own --android-orientation/--android-app-name/
        // --android-theme-color overrides, passed here as Gradle project properties. Both
        // default to values that reproduce today's un-overridden behavior for a build with
        // no bundled content, e.g. the plain engine-shell build
        // `.github/workflows/android.yml` produces: "unspecified" leaves the screen
        // orientation up to the OS/sensor, and "@string/app_name" is itself a resource
        // reference -- once substituted into AndroidManifest.xml's `android:label`, Android
        // resolves it exactly like the un-placeholdered `@string/app_name` this replaced.
        manifestPlaceholders["screenOrientation"] =
            (project.findProperty("servoScreenOrientation") as String?) ?: "unspecified"
        manifestPlaceholders["appName"] =
            (project.findProperty("servoAppName") as String?) ?: "@string/app_name"

        // Not a manifest placeholder like the two above: `android:statusBarColor` lives in a
        // *theme* (res/values/styles.xml), which manifestPlaceholders can't reach (those only
        // substitute inside AndroidManifest.xml itself). Instead this generates a new string
        // resource MainActivity.kt reads and applies to the status bar at runtime via
        // `Window.setStatusBarColor` -- see that file. Empty string (not a color) by default,
        // so "no theme_color set" means "don't touch the status bar" rather than forcing some
        // placeholder color.
        resValue("string", "servoThemeColor", (project.findProperty("servoThemeColor") as String?) ?: "")
    }

    // AGP 8+ requires this explicit opt-in for defaultConfig's `resValue` (used above for
    // `servoThemeColor`) -- off by default since AGP stopped generating it unconditionally
    // for build-time-cost reasons.
    buildFeatures {
        resValues = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }

    val signingKeyInfo = getSigningKeyInfo()

    if (signingKeyInfo != null) {
        signingConfigs {
            register("release") {
                storeFile = signingKeyInfo["storeFile"] as File
                storePassword = signingKeyInfo["storePassword"] as String
                keyAlias = signingKeyInfo["keyAlias"] as String
                keyPassword = signingKeyInfo["keyPassword"] as String
            }
        }
    }

    buildTypes {
        debug {
        }

        release {
            signingConfig =
                signingConfigs.getByName(if (signingKeyInfo != null) "release" else "debug")
            isMinifyEnabled = false
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
        }

        // Custom build types

        val debug = getByName("debug")
        val release = getByName("release")


        register("armv7Debug") {
            initWith(debug)
            ndk {
                abiFilters.add(getNDKAbi("armv7"))
            }
        }
        register("armv7Release") {
            initWith(release)
            ndk {
                abiFilters.add(getNDKAbi("armv7"))
            }
        }
        register("arm64Debug") {
            initWith(debug)
            ndk {
                abiFilters.add(getNDKAbi("arm64"))
            }
        }
        register("arm64Release") {
            initWith(release)
            ndk {
                abiFilters.add(getNDKAbi("arm64"))
            }
        }
        register("x86Debug") {
            initWith(debug)
            ndk {
                abiFilters.add(getNDKAbi("x86"))
            }
        }
        register("x86Release") {
            initWith(release)
            ndk {
                abiFilters.add(getNDKAbi("x86"))
            }
        }
        register("x64Debug") {
            initWith(debug)
            ndk {
                abiFilters.add(getNDKAbi("x64"))
            }
        }
        register("x64Release") {
            initWith(release)
            ndk {
                abiFilters.add(getNDKAbi("x64"))
            }
        }
    }
}

// Ignore default "debug" and "release" build types
androidComponents {
    beforeVariants {
        if (it.buildType == "release" || it.buildType == "debug") {
            it.enable = false
        }
    }
}

dependencies {
    implementation("androidx.webkit:webkit:1.12.1")
}

tasks.named("preBuild").configure { dependsOn(generateRovesBrandAssets) }
