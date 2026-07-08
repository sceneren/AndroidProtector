# Android 渠道读取工具

`ChannelReader.kt` 是一个可复制到业务 Android 项目中的独立 Kotlin 工具类，用于读取本桌面工具写入 APK 的当前渠道信息。

当前项目本身没有业务 Android 侧代码，所以该文件放在 `tools/channel-reader/` 目录下，不参与桌面工具编译。

## 写入格式

桌面工具会把渠道信息写入 APK Signing Block：

- Block ID：`0x71777777`
- Payload 格式：

```json
{"chn":"huawei"}
```

## 接入方式

1. 将 `ChannelReader.kt` 复制到你的 Android App 工程中。
2. 按你的项目包名修改文件第一行 `package com.androidprotector.channel`。
3. 在需要的位置调用：

```kotlin
val channel = ChannelReader.getChannel(context, "default")
```

## 返回值

`getChannel(context, defaultValue)` 会返回渠道名，例如：

- `huawei`
- `xiaomi`
- `oppo`
- `vivo`
- `honor`
- `yyb`

如果 APK 没有渠道信息、读取失败或 APK 结构不符合预期，会返回传入的 `defaultValue`。

如需读取原始 payload，可以使用：

```kotlin
val payload = ChannelReader.readChannelPayload(context)
```

返回示例：

```json
{"chn":"huawei"}
```

## 注意事项

- 该工具类用于运行时读取已安装 APK 自身的渠道信息。
- 只读取 APK Signing Block 中的渠道信息，不读取 Manifest、assets 或 meta-data。
- 多渠道包必须由当前桌面工具生成，或者写入相同的 Block ID 和 payload 格式。
- AAB 本身不直接用于安装运行，最终安装到设备的是由 AAB 构建出的 APK，因此这里读取的是运行时 APK。
