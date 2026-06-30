# Embedded Toolchain

The desktop app automatically searches this directory before falling back to environment variables.

Supported layout:

```text
tools/
  android-sdk/
    build-tools/<version>/
      zipalign(.exe)
      apksigner(.bat/.cmd/.jar)
  jdk/
    bin/java
    bin/jarsigner
  bundletool/
    bundletool.jar
```

For development on this machine, the app also auto-detects `ANDROID_HOME`, `ANDROID_SDK_ROOT`, and `JAVA_HOME`.

