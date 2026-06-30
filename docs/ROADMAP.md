# 后续优化与完善计划

## 当前状态

项目已经具备一个可运行的桌面端基础版本：

- Tauri + Rust + React/TypeScript 桌面应用。
- APK/AAB 扫描、DEX 统计、VMP 规则估算。
- 工具链自动探测，支持项目内置 `tools/` 目录。
- 签名信息库，支持签名 profile 的增删改查和 alias 校验。
- 输出目录记忆。
- 任务日志、后台任务状态、取消任务。
- DEX payload 加密封装和 ZIP 重写骨架。
- Android loader 源码骨架。

当前版本还不是完整生产级加固器。核心缺口集中在真实 DEX 重写、binary manifest 重写、loader 产物注入、运行时解密加载和完整 VMP 执行链。

## 优先级最高的工作

1. 完成 binary AndroidManifest.xml 重写
   - 支持 APK 的二进制 AXML 修改。
   - 支持 AAB 的 `base/manifest/AndroidManifest.xml` 修改。
   - 将原始 `Application` 替换为 `com.protector.runtime.ProtectorApplication`。
   - 写入 `meta-data` 保存原始 Application 类名。
   - 保留 manifest 原有 namespace、权限、组件、provider authorities。
   - 验收：无 Application、有自定义 Application、相对类名 `.App` 三类样例均可重写并通过安装/启动。

2. 接入 loader dex/so 编译产物注入
   - 增加脚本构建 `loader-android/protector-loader`。
   - 从构建产物提取 loader dex 和 `libprotector_vm.so`。
   - 注入 APK 的 `classes*.dex` 与 `lib/<abi>/`。
   - 注入 AAB 的 `base/dex/` 与 `base/lib/<abi>/`。
   - 避免 dex 编号冲突，支持 multidex。
   - 验收：加固后的 APK 包含 loader dex、so，且能加载到 native library。

3. 完成真实 DEX 加密与运行时加载
   - 将原始 `classes*.dex` 移出明文 dex 列表，写入加密 payload。
   - loader 在 `attachBaseContext` 阶段解密 payload。
   - API 26+ 优先使用内存加载。
   - API 23-25 使用受控临时文件 fallback。
   - 运行时恢复原始 Application 生命周期。
   - 验收：样例 app 加固后可正常启动，业务类可加载，原 Application 的 `attachBaseContext/onCreate` 被调用。

4. 完成 VMP v1 的真实方法改写
   - 明确定义自定义 VM 字节码格式。
   - 将选中方法转换为 VM 字节码 payload。
   - 将原方法体替换为 stub，调用 `ProtectorRuntime.invokeVm(methodId, receiver, args)`。
   - 先支持低风险指令集合：常量、寄存器移动、基础算术、字段读写、静态/虚方法调用、分支、返回。
   - 明确跳过异常复杂控制流、同步块、构造方法、native/abstract、Android 组件生命周期入口。
   - 验收：开启 VMP 的样例方法返回值与原包一致，并能在 emulator 上运行。


## 推荐迭代顺序

### M1：打包与工具链稳定

- 固化内置工具链目录结构。
- 增加一键下载/校验 bundletool、build-tools、JDK 的脚本。
- 明确第三方工具许可和分发方式。
- Windows 生成安装包，macOS 生成 `.app/.dmg`。

验收标准：

- 新机器无需手动配置 SDK/JDK 即可打开应用并显示工具链就绪。
- Windows 启动无命令行窗口。
- UI 在 720px 宽度以上无水平滚动。

### M2：真实 APK 加固闭环

- 实现 binary manifest patch。
- 注入 loader dex/so。
- 原 dex 加密并运行时加载。
- APK zipalign、apksigner、verify 全流程稳定。

验收标准：

- 至少 4 个样例 APK 通过：无 Application、自定义 Application、multidex、带 native libs。
- 加固后 APK 可安装、启动、签名校验通过。

### M3：VMP v1 闭环

- 实现 DEX 方法选择到真实 stub 替换。
- 实现 native VM 字节码解释器。
- 增加 VMP payload 加密和 methodId 映射。
- UI 展示实际虚拟化数量、跳过数量和跳过原因。

验收标准：

- 样例方法覆盖返回值、分支、字段访问、静态调用、实例调用。
- 原包与加固包行为一致。
- 不支持的方法保持原样且有明确报告。

### M4：AAB 支持完善

- base module manifest/dex/native 注入。
- 使用 bundletool validate/build-apks 验证。
- 明确 dynamic feature 的处理策略：v1 可保留不加固，后续逐模块支持。

验收标准：

- base module AAB 可加固、签名、通过 bundletool 校验。
- build-apks 后安装到 emulator 可启动。

### M5：生产安全增强

- native 层反调试、ptrace 检测、TracerPid 检测、maps 检测。
- payload 完整性校验。
- 原始签名摘要校验。
- native 符号裁剪和字符串混淆。
- VM 字节码控制流扰动。

验收标准：

- 修改 payload 后运行时拒绝启动。
- 修改签名或重打包后触发防篡改逻辑。
- release loader so 去符号并可稳定加载。

## 测试建设

建议新增 `samples/` 和自动化测试：

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

- 任务日志支持保存到文件。
- 加固失败时展示阶段、工具输出摘要、可操作修复建议。
- VMP 规则支持包树/类树选择，而不仅是文本规则。
- 输出结果区增加“打开目录”“复制路径”“重新加固”。
- 增加暗色模式可选项。

## 工程化建议

- 增加 CI：`pnpm build`、`cargo test`、`cargo check --release`。
- 增加 release 脚本：构建前清理旧进程、生成版本号、打包产物。
- 增加 `docs/ARCHITECTURE.md`：描述扫描、VMP、加密、注入、签名的模块边界。
- 增加 `docs/SECURITY.md`：说明威胁模型、已实现保护和未覆盖风险。
- 增加 `docs/TOOLCHAIN.md`：说明内置工具链目录、下载来源、版本锁定和许可。

## 风险与注意事项

- 完整 VMP 是最大工作量，建议先限制支持指令集合，逐步扩大覆盖。
- Android manifest 是二进制 AXML，不能用普通 XML 字符串替换。
- 修改 APK/AAB 后不能保留原签名，必须重新签名。
- 保存签名密码必须迁移到系统安全存储后再面向真实用户使用。
- 内置 JDK/Android build-tools/bundletool 需要确认许可和分发合规。
- macOS 发布需要处理代码签名、notarization 和 Gatekeeper。

