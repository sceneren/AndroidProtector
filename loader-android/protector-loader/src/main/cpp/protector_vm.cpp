#include <android/log.h>
#include <jni.h>

#include <fstream>
#include <string>

namespace {

constexpr const char* kTag = "ProtectorVM";

bool is_debugger_attached() {
    std::ifstream status("/proc/self/status");
    std::string line;
    while (std::getline(status, line)) {
        if (line.rfind("TracerPid:", 0) == 0) {
            return line.find_first_of("123456789") != std::string::npos;
        }
    }
    return false;
}

}  // namespace

extern "C" JNIEXPORT void JNICALL
Java_com_protector_runtime_ProtectorRuntime_nativeInit(
    JNIEnv* env,
    jclass,
    jstring package_name,
    jstring source_dir,
    jint sdk_int) {
    const char* pkg = env->GetStringUTFChars(package_name, nullptr);
    const char* src = env->GetStringUTFChars(source_dir, nullptr);
    if (is_debugger_attached()) {
        __android_log_print(ANDROID_LOG_ERROR, kTag, "debugger detected for %s", pkg);
    }
    __android_log_print(ANDROID_LOG_INFO, kTag, "init package=%s sdk=%d source=%s", pkg, sdk_int, src);
    env->ReleaseStringUTFChars(package_name, pkg);
    env->ReleaseStringUTFChars(source_dir, src);
}

extern "C" JNIEXPORT jobject JNICALL
Java_com_protector_runtime_ProtectorRuntime_nativeInvoke(
    JNIEnv*,
    jclass,
    jint,
    jobject,
    jobjectArray) {
    return nullptr;
}

