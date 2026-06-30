plugins {
    id("com.android.library")
}

android {
    namespace = "com.protector.runtime"
    compileSdk = 35

    defaultConfig {
        minSdk = 23
        externalNativeBuild {
            cmake {
                cppFlags += "-std=c++20"
            }
        }
        ndk {
            abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64")
        }
    }

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
        }
    }
}

