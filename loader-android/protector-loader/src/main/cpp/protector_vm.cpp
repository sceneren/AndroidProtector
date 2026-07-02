#include <android/log.h>
#include <jni.h>

#include <array>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <string>

namespace {

constexpr const char* kTag = "ProtectorVM";
constexpr const char* kWrapLabel = "android-protector-dex-wrap-v2";
constexpr std::array<uint8_t, 32> kWrapSeedA = {
    0x19, 0x7d, 0x42, 0xb8, 0xc1, 0x0e, 0x6a, 0x90,
    0x25, 0xf4, 0x38, 0xdd, 0x61, 0xab, 0x0c, 0x73,
    0xe7, 0x56, 0x2d, 0x81, 0x9c, 0x04, 0xfa, 0x3b,
    0x68, 0x12, 0xcf, 0xa5, 0x4e, 0x91, 0x37, 0xd0,
};
constexpr std::array<uint8_t, 32> kWrapSeedB = {
    0xc3, 0x55, 0x2f, 0x80, 0x0a, 0xee, 0x41, 0x72,
    0xd9, 0x66, 0x13, 0xac, 0x5f, 0x98, 0x21, 0x47,
    0xbe, 0x02, 0xf6, 0x35, 0x8c, 0x7a, 0x10, 0xd4,
    0x29, 0xb1, 0x6e, 0x03, 0x95, 0x4c, 0xea, 0x1f,
};
constexpr std::array<uint32_t, 64> kSha256RoundConstants = {
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
};

struct Sha256Context {
    std::array<uint32_t, 8> state = {
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    };
    std::array<uint8_t, 64> buffer = {};
    uint64_t bit_len = 0;
    size_t buffer_len = 0;
};

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

uint32_t rotate_right(uint32_t value, uint32_t bits) {
    return (value >> bits) | (value << (32 - bits));
}

void sha256_transform(Sha256Context& context, const uint8_t* chunk) {
    std::array<uint32_t, 64> schedule = {};
    for (size_t index = 0; index < 16; index++) {
        size_t offset = index * 4;
        schedule[index] = (static_cast<uint32_t>(chunk[offset]) << 24) |
                          (static_cast<uint32_t>(chunk[offset + 1]) << 16) |
                          (static_cast<uint32_t>(chunk[offset + 2]) << 8) |
                          static_cast<uint32_t>(chunk[offset + 3]);
    }
    for (size_t index = 16; index < 64; index++) {
        uint32_t s0 = rotate_right(schedule[index - 15], 7) ^
                      rotate_right(schedule[index - 15], 18) ^
                      (schedule[index - 15] >> 3);
        uint32_t s1 = rotate_right(schedule[index - 2], 17) ^
                      rotate_right(schedule[index - 2], 19) ^
                      (schedule[index - 2] >> 10);
        schedule[index] = schedule[index - 16] + s0 + schedule[index - 7] + s1;
    }

    uint32_t a = context.state[0];
    uint32_t b = context.state[1];
    uint32_t c = context.state[2];
    uint32_t d = context.state[3];
    uint32_t e = context.state[4];
    uint32_t f = context.state[5];
    uint32_t g = context.state[6];
    uint32_t h = context.state[7];

    for (size_t index = 0; index < 64; index++) {
        uint32_t s1 = rotate_right(e, 6) ^ rotate_right(e, 11) ^ rotate_right(e, 25);
        uint32_t choice = (e & f) ^ (~e & g);
        uint32_t temp1 = h + s1 + choice + kSha256RoundConstants[index] + schedule[index];
        uint32_t s0 = rotate_right(a, 2) ^ rotate_right(a, 13) ^ rotate_right(a, 22);
        uint32_t majority = (a & b) ^ (a & c) ^ (b & c);
        uint32_t temp2 = s0 + majority;
        h = g;
        g = f;
        f = e;
        e = d + temp1;
        d = c;
        c = b;
        b = a;
        a = temp1 + temp2;
    }

    context.state[0] += a;
    context.state[1] += b;
    context.state[2] += c;
    context.state[3] += d;
    context.state[4] += e;
    context.state[5] += f;
    context.state[6] += g;
    context.state[7] += h;
}

void sha256_update(Sha256Context& context, const uint8_t* data, size_t len) {
    for (size_t index = 0; index < len; index++) {
        context.buffer[context.buffer_len++] = data[index];
        if (context.buffer_len == context.buffer.size()) {
            sha256_transform(context, context.buffer.data());
            context.bit_len += 512;
            context.buffer_len = 0;
        }
    }
}

std::array<uint8_t, 32> sha256_finalize(Sha256Context& context) {
    size_t index = context.buffer_len;
    context.buffer[index++] = 0x80;
    if (index > 56) {
        while (index < 64) {
            context.buffer[index++] = 0;
        }
        sha256_transform(context, context.buffer.data());
        index = 0;
    }
    while (index < 56) {
        context.buffer[index++] = 0;
    }

    context.bit_len += context.buffer_len * 8;
    for (int shift = 7; shift >= 0; shift--) {
        context.buffer[index++] = static_cast<uint8_t>((context.bit_len >> (shift * 8)) & 0xff);
    }
    sha256_transform(context, context.buffer.data());

    std::array<uint8_t, 32> digest = {};
    for (size_t state_index = 0; state_index < context.state.size(); state_index++) {
        digest[state_index * 4] = static_cast<uint8_t>((context.state[state_index] >> 24) & 0xff);
        digest[state_index * 4 + 1] = static_cast<uint8_t>((context.state[state_index] >> 16) & 0xff);
        digest[state_index * 4 + 2] = static_cast<uint8_t>((context.state[state_index] >> 8) & 0xff);
        digest[state_index * 4 + 3] = static_cast<uint8_t>(context.state[state_index] & 0xff);
    }
    return digest;
}

std::array<uint8_t, 32> derive_wrap_key(const char* package_name, const uint8_t* salt, size_t salt_len) {
    std::array<uint8_t, 32> secret = {};
    for (size_t index = 0; index < secret.size(); index++) {
        secret[index] = static_cast<uint8_t>(kWrapSeedA[index] ^ kWrapSeedB[31 - index] ^ 0x5a);
    }

    Sha256Context context;
    sha256_update(context, reinterpret_cast<const uint8_t*>(kWrapLabel), std::strlen(kWrapLabel));
    sha256_update(context, secret.data(), secret.size());
    sha256_update(context, salt, salt_len);
    if (package_name != nullptr) {
        sha256_update(context, reinterpret_cast<const uint8_t*>(package_name), std::strlen(package_name));
    }
    return sha256_finalize(context);
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

extern "C" JNIEXPORT jbyteArray JNICALL
Java_com_protector_runtime_ProtectorRuntime_nativeDeriveWrapKey(
    JNIEnv* env,
    jclass,
    jstring package_name,
    jbyteArray salt_bytes) {
    if (salt_bytes == nullptr) {
        return nullptr;
    }
    const char* package_chars = package_name == nullptr
        ? nullptr
        : env->GetStringUTFChars(package_name, nullptr);
    jsize salt_len = env->GetArrayLength(salt_bytes);
    jbyte* salt = env->GetByteArrayElements(salt_bytes, nullptr);
    auto key = derive_wrap_key(
        package_chars,
        reinterpret_cast<const uint8_t*>(salt),
        static_cast<size_t>(salt_len));
    env->ReleaseByteArrayElements(salt_bytes, salt, JNI_ABORT);
    if (package_chars != nullptr) {
        env->ReleaseStringUTFChars(package_name, package_chars);
    }
    jbyteArray result = env->NewByteArray(static_cast<jsize>(key.size()));
    if (result == nullptr) {
        return nullptr;
    }
    env->SetByteArrayRegion(result, 0, static_cast<jsize>(key.size()), reinterpret_cast<const jbyte*>(key.data()));
    return result;
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
