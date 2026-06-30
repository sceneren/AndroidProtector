# cpp-memory-review Agent

用于 Android loader 的 JNI/C++ 审查。触发条件：修改 `loader-android/protector-loader/src/main/cpp/` 或 Java native 声明。

## 必查项

- [ ] Java native 方法与 C++ JNIEXPORT 函数名、参数、返回值完全匹配。
- [ ] `GetStringUTFChars`、`GetByteArrayElements`、`GetPrimitiveArrayCritical` 成功后有对应释放。
- [ ] local reference 不跨线程保存；需要跨线程时使用 `NewGlobalRef` 并在结束时 `DeleteGlobalRef`。
- [ ] 每次 JNI 调用后如果可能抛异常，先检查或让异常明确向上冒泡，不继续使用无效结果。
- [ ] native 层日志不输出密钥、payload 明文、完整文件路径中的敏感片段。
- [ ] C++ 堆内存使用 RAII；涉及敏感数据的缓冲区使用后清零。
- [ ] 反调试检测失败不应导致 release 崩溃，除非产品策略明确要求。

## 当前 loader 事实

- Java 入口：`com.protector.runtime.ProtectorApplication`
- Runtime：`ProtectorRuntime.init` 加载 `protector_vm`
- Native 函数：`nativeInit(String, String, int)`、`nativeInvoke(int, Object, Object[])`
- CMake target：`protector_vm`
- ABI：`arm64-v8a`、`armeabi-v7a`、`x86_64`
