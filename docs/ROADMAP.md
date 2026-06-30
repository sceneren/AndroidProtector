# 后续优化与完善计划

## 当前状态

项目已经具备一个可运行的跨平台桌面端基础版本：

- Tauri + Rust + React/TypeScript 桌面应用。
- APK/AAB 扫描、DEX 统计、native ABI 识别。
- VMP 规则估算和跳过原因统计。
- 工具链自动探测，支持项目内置 `tools/` 目录。
- 签名信息库，支持 profile 增删改查、alias 自动识别和校验。
- 输出目录记忆，加固成功后自动打开输出目录。
- APK 多渠道打包：支持 `huawei`、`xiaomi`、`oppo`、`vivo`、`honor`、`yyb`，AAB 禁用。
- 独立“渠道包”页：选择已签名 APK、渠道包输出目录和渠道后，直接生成渠道包。
- 任务日志、失败修复建议、保存日志、复制输出路径、重新加固。
- DEX payload AES-256-GCM 加密封装。
- APK/AAB ZIP 重写，移除旧签名文件并写入 protector metadata。
- loader dex/so 产物发现与注入边界：`tools/loader/classes.dex` 和 `tools/loader/lib/<abi>/libprotector_vm.so`。
- 内置 Java loader dex fallback，避免 Manifest 已改为 `ProtectorApplication` 但包内缺少类导致启动闪退。
- binary/text AndroidManifest 初版 patch：将 `application android:name` 替换为 `com.protector.runtime.ProtectorApplication`。
- protection manifest 记录原始 Application、loader 注入状态、VMP plan、payload 信息。
- Android loader 源码骨架，运行时可从 `assets/protector/protection-manifest.json` 读取原始 Application 名称作为 fallback。
- CI 工作流：`pnpm build`、`cargo test`、`cargo check --release`。

当前版本仍不是完整生产级加固器。核心缺口集中在运行时解密加载、完整 binary manifest meta-data 写入、真实 DEX 重写、完整 VMP 执行链和 emulator 验收。

## 最近已完成

- 修复 Windows release 启动时命令行窗口问题。
- 修复 `apksigner` 参数顺序，避免 `Unexpected parameter(s) after input APK (--key-pass)`。
- 签名信息移动到左侧表单，支持保存、编辑、删除、选择。
- 新增签名文件 alias 自动识别：选择 keystore 后输入 store password 自动读取 alias。
- 输出目录移动到左侧表单，并默认使用上次输出目录。
- 加固成功后自动打开输出目录。
- 结果区增加“打开目录”“复制路径”“保存日志”“重新加固”。
- 失败时按阶段展示修复建议。
- 新增 `save_job_log` 后端命令。
- 新增 loader 注入模块和 `scripts/prepare_loader_artifacts.ps1`。
- 新增 manifest patch 模块，支持文本 manifest 和基础 binary AXML manifest。
- 扫描模块改为使用 manifest 解析器读取包名和 Application。
- loader runtime 增加从 protector metadata 读取原始 Application 的 fallback。
- 新增多渠道打包卡片和 APK Signing Block 渠道写入模块。
- 顶部新增“加固 / 渠道包”Tab，“渠道包”Tab 独立支持 APK 渠道包生成。
- 修复加固后启动即闪退：补齐 `tools/loader/classes.dex`，后端内置 dex fallback，并在 loader dex 缺失时停止产物生成。
- loader runtime 对 native so 缺失启用兼容降级，避免当前骨架阶段因 `System.loadLibrary` 失败直接崩溃。
- 优化加固包体积：当前兼容模式不再重复写入完整加密 DEX payload，VMP 计划文件改为摘要和少量样例。
- 签名信息编辑支持明文显示已保存的 store/key 密码，并新增 APK 签名方式选择：默认 `V1+V2`，可选 `V1+V2+V3`。
- 签名信息弹窗改为单列字段顺序：签名文件、Store password、Alias、Key password、签名方式，并隐藏 Store type 和底部别名详情。
- DEX 加密从 metadata-only 调整为真实加密 payload：输出包移除原始 `classes*.dex`，仅保留 loader dex、加密 payload 和 metadata，loader 启动时解密并安装运行时 DexClassLoader。
- 修复 DEX 加密包启动问题：loader 内置 AndroidX `CoreComponentFactory` 兼容类，DexClassLoader native 搜索路径补充 APK 内 `lib/<abi>`，运行时 dex 写入后设为只读。

## 优先级最高的工作

### 1. 完善 binary AndroidManifest.xml 重写

已完成：

- 支持文本 manifest 的 `application android:name` 替换。
- 支持基础 binary AXML 的 `application android:name` 替换。
- 支持无原始 Application 时新增 `android:name` 属性。
- 支持相对类名 `.App` 归一化记录为完整类名。
- APK 和 AAB base manifest 都走同一 patch 边界。

待完成：

- 在 binary AXML 中原生写入 `<meta-data android:name="protector.original_application" ...>`。
- 支持更复杂的 string pool styles、资源引用和异常 manifest 结构。
- 增加真实 APK 样例回归：无 Application、自定义 Application、相对类名 `.App`。
- emulator 安装启动验证：`ProtectorApplication` 启动，原 Application 生命周期被调用。

### 2. 完成 loader dex/so 编译产物注入

已完成：

- 约定 `tools/loader/classes.dex` 作为 loader dex。
- 约定 `tools/loader/lib/<abi>/libprotector_vm.so` 作为 native loader。
- APK 注入到 `classes*.dex` 和 `lib/<abi>/`。
- AAB 注入到 `base/dex/classes*.dex` 和 `base/lib/<abi>/`。
- 避免 dex 编号冲突，支持 multidex 后追加。
- protection manifest 记录注入目标和问题。
- 提供 `scripts/prepare_loader_artifacts.ps1` 收集构建产物。

待完成：

- 完成 Gradle loader 构建脚本的稳定产物输出。
- 在 release 脚本中自动调用 loader artifact 准备流程。
- 验证加固后 APK 包含 loader dex/so，并能在设备上加载 `protector_vm`。

### 3. 完成真实 DEX 加密与运行时加载

待完成：

- 将原始 `classes*.dex` 从明文 dex 列表中移除或替换为受控 stub/loader dex。
- loader 在 `attachBaseContext` 阶段读取并解密 `assets/protector/dex-payload.json`。
- API 26+ 优先使用内存加载。
- API 23-25 使用受控临时文件 fallback。
- 恢复原始 Application 生命周期：`attachBaseContext/onCreate`。
- 验收：样例 app 加固后可正常启动，业务类可加载。

### 4. 完成 VMP v1 真实方法改写

待完成：

- 定义自定义 VM 字节码格式。
- 将选中方法转换为 VM 字节码 payload。
- 将原方法体替换为 stub，调用 `ProtectorRuntime.invokeVm(methodId, receiver, args)`。
- 先支持低风险指令集合：常量、寄存器移动、基础算术、字段读写、静态/虚方法调用、分支、返回。
- 跳过复杂异常控制流、同步块、构造方法、native/abstract、Android 组件生命周期入口。
- 验收：开启 VMP 的样例方法返回值与原包一致，并能在 emulator 上运行。

## 推荐迭代顺序

### M1：打包与工具链稳定

状态：基本完成，仍需分发合规确认。

- 固化内置工具链目录结构。
- 增加一键下载/校验 bundletool、build-tools、JDK 的脚本。
- 明确第三方工具许可和分发方式。
- Windows 生成安装包，macOS 生成 `.app/.dmg`。

验收标准：

- 新机器无需手动配置 SDK/JDK 即可打开应用并显示工具链就绪。
- Windows 启动无命令行窗口。
- UI 在 720px 宽度以上无水平滚动。

### M2：真实 APK 加固闭环

状态：进行中。

- 已完成 loader dex/so 注入边界。
- 已完成 manifest Application 替换初版。
- 待完成运行时 DEX 解密加载。
- 待完成真实样例 APK 安装启动验证。

验收标准：

- 至少 4 个样例 APK 通过：无 Application、自定义 Application、multidex、带 native libs。
- 加固后 APK 可安装、启动、签名校验通过。

### M3：VMP v1 闭环

状态：未开始真实改写。

- 实现 DEX 方法选择到真实 stub 替换。
- 实现 native VM 字节码解释器。
- 增加 VMP payload 加密和 methodId 映射。
- UI 展示实际虚拟化数量、跳过数量和跳过原因。

验收标准：

- 样例方法覆盖返回值、分支、字段访问、静态调用、实例调用。
- 原包与加固包行为一致。
- 不支持的方法保持原样且有明确报告。

### M4：AAB 支持完善

状态：基础 ZIP/签名/验证边界已有，真实运行验收未完成。

- base module manifest/dex/native 注入。
- 使用 bundletool validate/build-apks 验证。
- dynamic feature v1 保留不加固，后续逐模块支持。
- 多渠道打包不支持 AAB，UI 禁用且后端校验拒绝。

验收标准：

- base module AAB 可加固、签名、通过 bundletool 校验。
- build-apks 后安装到 emulator 可启动。

### M5：生产安全增强

状态：未开始生产级增强。

- native 层反调试：ptrace、TracerPid、maps 检测。
- payload 完整性校验。
- 原始签名摘要校验。
- native 符号裁剪和字符串混淆。
- VM 字节码控制流扰动。

验收标准：

- 修改 payload 后运行时拒绝启动。
- 修改签名或重打包后触发防篡改逻辑。
- release loader so 去符号并可稳定加载。

## 测试建设

建议新增 `samples/`：

- `samples/no-application`
- `samples/custom-application`
- `samples/multidex`
- `samples/native-lib`
- `samples/vmp-basic`
- `samples/aab-base`

测试类型：

- Rust unit tests：DEX 解析、VMP 规则、manifest patch、payload 加密、签名 profile。
- Integration tests：对样例包执行完整加固流程。
- Emulator tests：安装加固包并验证启动、日志、关键方法返回值。
- Cross-platform tests：Windows/macOS 路径、中文目录、空格目录。

## UI 后续优化

已完成：

- 任务日志保存到文件。
- 加固失败时展示阶段、工具输出摘要、可操作修复建议。
- 输出结果区增加“打开目录”“复制路径”“重新加固”。
- 输出目录上方新增多渠道打包卡片，支持 APK 渠道多选，AAB 自动禁用。

待完成：

- VMP 规则支持包树/类树选择，而不仅是文本规则。
- 增加暗色模式可选项。
- 输出结果区展示 manifest patch、loader 注入、签名验证的结构化摘要。

## 工程化建议

已完成：

- CI：`pnpm build`、`cargo test`、`cargo check --release`。
- Windows/macOS release packaging 脚本基础版。

待完成：

- release 脚本构建前清理旧进程、生成版本号、打包产物。
- `docs/ARCHITECTURE.md`：描述扫描、VMP、加密、注入、签名的模块边界。
- `docs/SECURITY.md`：说明威胁模型、已实现保护和未覆盖风险。
- `docs/TOOLCHAIN.md`：说明内置工具链目录、下载来源、版本锁定和许可。

## 风险与注意事项

- 完整 VMP 是最大工作量，建议先限制支持指令集合，逐步扩大覆盖。
- Android manifest 是二进制 AXML，不能依赖普通 XML 字符串替换。
- 修改 APK/AAB 后不能保留原签名，必须重新签名。
- 保存签名密码必须迁移到系统安全存储后再面向真实用户使用。
- 内置 JDK/Android build-tools/bundletool 需要确认许可和分发合规。
- macOS 发布需要处理代码签名、notarization 和 Gatekeeper。
