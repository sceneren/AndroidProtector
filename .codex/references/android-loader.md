# android-loader

## 模块概述

`android-loader` 是将来注入目标 APK/AAB 的 Android 运行时骨架。它提供 `ProtectorApplication` 作为入口，初始化 native library `protector_vm`，读取原始 Application 元数据并委托生命周期；C++ 层目前包含反调试检测和 VM 调用占位。

## 元信息

| 项 | 值 |
|---|---|
| 类型 | Android library + CMake native library |
| 路径 | `loader-android/protector-loader/` |
| namespace | `com.protector.runtime` |
| compileSdk / minSdk | 35 / 23 |
| ABI | `arm64-v8a`, `armeabi-v7a`, `x86_64` |
| 源文件数 | 3 |

## 目录结构

```text
loader-android/protector-loader/
└── src/main/
    ├── AndroidManifest.xml
    ├── java/com/protector/runtime/
    │   ├── ProtectorApplication.java
    │   └── ProtectorRuntime.java
    └── cpp/
        ├── CMakeLists.txt
        └── protector_vm.cpp
```

## 类与接口详情

### `ProtectorApplication`

- 继承 `android.app.Application`。
- `attachBaseContext` 调用 `ProtectorRuntime.init(base)`，创建原始 Application，并通过反射调用其 `attach`。
- `onCreate` 先调用自身 `super.onCreate()`，再委托给原始 Application。

### `ProtectorRuntime`

- `init(Context)` 加载 `protector_vm`，调用 `nativeInit(packageName, sourceDir, sdkInt)`，并用 `initialized` 防重复。
- `createOriginalApplication(Context)` 从 metadata key `protector.original_application` 读取原始 Application 类名。
- `callAttachBaseContext(Application, Context)` 通过反射调用 Application.attach。
- `invokeVm(int, Object, Object[])` 转发到 `nativeInvoke`。

### `protector_vm.cpp`

- `nativeInit` 读取 `/proc/self/status` 的 `TracerPid` 检测调试器，并输出 Android log。
- `nativeInvoke` 当前返回 `nullptr`，是 VMP runtime 后续实现入口。
- 当前 C++ 使用 `GetStringUTFChars` 后释放，保持 JNI 字符串生命周期正确。

## 构建配置

- 根工程：`loader-android/settings.gradle.kts`
- Android Gradle Plugin：8.7.3
- 模块：`:protector-loader`
- CMake：`src/main/cpp/CMakeLists.txt`
- C++ 标准：C++20

## 约定与风险

- Java native 方法签名和 C++ 函数名必须同步：`nativeInit`、`nativeInvoke`。
- `System.loadLibrary("protector_vm")` 与 CMake target `protector_vm` 不能漂移。
- 真实 payload 解密、class loading、signature tamper check 和 VM interpreter 尚未完成。
- manifest 注入时必须把目标原 Application 写入 `protector.original_application` metadata。
