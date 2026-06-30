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
  loader/
    classes.dex
    lib/
      arm64-v8a/libprotector_vm.so
      armeabi-v7a/libprotector_vm.so
      x86_64/libprotector_vm.so
```

For development on this machine, the app also auto-detects `ANDROID_HOME`, `ANDROID_SDK_ROOT`, and `JAVA_HOME`.

The Java loader dex is required because the protection pipeline patches `AndroidManifest.xml` to `com.protector.runtime.ProtectorApplication`.
The desktop backend also embeds `tools/loader/classes.dex` at build time as a fallback, so release builds can still inject the Java loader when external resources are not next to the executable.
Native libraries are optional in the current compatibility build; when present, they are injected into the matching APK/AAB `lib` path.

To prepare loader artifacts from the Android loader project:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\prepare_loader_artifacts.ps1
```

Use `-SkipBuild` when the Gradle project has already been built and you only want to collect artifacts.
