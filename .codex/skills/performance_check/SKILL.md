# Performance Check Skill

用于检查本项目的性能、稳定性和安全运行风险。

## 前端

- 大型日志列表只渲染最近记录，避免无界增长。当前 `JobReporter` 后端限制 500 条，前端展示最近 12 条。
- 轮询 `get_job_status` 必须在任务结束和组件卸载时清理 timer。
- 路径、日志、错误文本必须允许换行或截断，避免破坏桌面窗口布局。
- 不在 React state 中长期保存不必要的签名密码副本。

## Rust 核心

- ZIP、DEX、manifest、payload 解析必须使用边界检查，不对外部文件使用 unchecked indexing。
- 大文件处理优先流式或临时文件，避免把完整 APK/AAB 多次复制到内存。
- 后台任务每个阶段检查取消标志，长循环中增加取消点。
- 外部命令输出摘要限制行数，避免 UI 或日志膨胀。
- 加密 payload 不记录明文、key、nonce 之外的敏感上下文；签名密码通过 env 传入工具。

## Android loader

- `attachBaseContext` 中只执行必要初始化，避免阻塞启动。
- native 反调试和 payload 加载失败需要明确失败策略，不吞异常。
- JNI 引用和 C++ 内存必须按生命周期释放。
- API 23-25 fallback 临时文件必须使用私有目录并及时清理。

## 验证建议

- `pnpm build`
- `cd src-tauri; cargo test`
- loader 可构建环境下运行 `gradle :protector-loader:assembleRelease`
- 对大 APK/AAB 运行扫描和 VMP 估算，观察内存、耗时和日志增长。
