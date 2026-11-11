# VasDolly command-line tool

- Upstream: https://github.com/Tencent/VasDolly
- Binary source: https://raw.githubusercontent.com/Tencent/VasDolly/master/command/jar/VasDolly.jar
- Retrieved: 2026-09-30
- SHA-256: `15364fd2d3725bed78d5bb4b00ce63b18e3a018438faf8fe030049099aa4da27`
- License: upstream BSD 3-Clause and third-party notices in `LICENSE`.

The checked-in `VasDolly.jar` is the unmodified official command JAR; its version is not inferred from the Gradle plugin version. Runtime packaging uses `java -jar VasDollyBridge.jar INPUT.apk OUTPUT.apk CHANNEL`. Arguments are passed separately without a shell. The existing Tauri `../tools` resource includes both JARs and the license, and the backend embeds both JARs as a fallback.

The Java 8-compatible adapter source is `bridge/VasDollyBridge.java`. It calls official `ChannelWriter`, `ChannelReader` and `VerifyApk` APIs. This avoids the upstream CLI's V1 empty-channel early return and zero exit codes after failures. It verifies the source signature, writes to a copy, reads the channel back, and verifies the output signature and signature schemes. V2/V3 writes use the official low-memory mode. No channel format is reimplemented here.

Rebuild the adapter with JDK 9 or later (runtime requires Java 8 or later):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-vasdolly-bridge.ps1
```

After changing the adapter, rebuild `VasDollyBridge.jar` and run `scripts/test-vasdolly.ps1` to test real V1/V2/V3-signed APKs, channel read-back, signatures, source preservation, output collisions and unsigned APK pre-signing.

Android apps should use the upstream helper library to read VasDolly channels. The original custom JSON channel format is a separate option.
