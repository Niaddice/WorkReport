# 架构说明

这份文档描述 WorkReport 的运行时结构、关键链路和设计取舍。功能说明见 [README](README.md)。

## 分层

```text
Vue 界面层      AppToolbar / AppRail / CalendarPane / JournalEditor / SettingsView / 各类弹窗
      │
状态编排层      useJournal  useAi  useExcel  useSummaryOutput  useAppearance  useSplitter
      │
IPC（38 个）    文件读写 · 历史版本 · 界面偏好 · AI 请求 · Excel 两阶段 · 设置与假日
      │
Rust 领域层     workspace  history  ui  ai  excel  database  holidays  settings  summary  appearance
      │
持久化          年份目录文件 / .history 版本快照 / SQLite / MySQL / PostgreSQL / 外部 AI 与 holiday-cn
```

前端不直接访问文件系统或网络：所有 IO 都通过 `invoke` 交给 Rust，WebView 的 CSP 只允许 `self` 与 `ipc:`。

## 关键文件

| 文件                         | 职责                                                                        |
| ---------------------------- | --------------------------------------------------------------------------- |
| `src/App.vue`                | 页面编排：工作空间门槛、首页/设置切换、框选总结、Excel 输出、弹窗协调       |
| `src/lib/useJournal.ts`      | 日报草稿、串行自动保存、撤销栈、格式切换、索引与预览失效                    |
| `src/lib/useExcel.ts`        | Excel 设置、日期范围校验、预览与确认两步调用                                |
| `src/lib/useSplitter.ts`     | 日历/编辑区分隔条的拖动、键盘调整与宽度夹取                                 |
| `src-tauri/src/workspace.rs` | 年份目录索引、按需预览、原子保存、冲突检测、双存储与迁移                    |
| `src-tauri/src/history.rs`   | 日报版本快照：内容去重、按容量上限清理最旧日期、每天 100 份上限、跨日期列表 |
| `src-tauri/src/ui.rs`        | 界面偏好：行/列方向、模块顺序与皮肤 id，非法值回落默认                      |
| `src-tauri/src/ai.rs`        | CLI 检测与调用、HTTP 兼容接口、取消与超时、提示词拼装                       |
| `src-tauri/src/excel.rs`     | 模板解析、AI 填表计划校验、样式写入、预览缓存与发布                         |
| `src-tauri/src/database.rs`  | SQLx 三库连接、工作空间隔离、按 revision 写入                               |
| `src-tauri/src/settings.rs`  | `workreport.json` 一次性读写，临时文件 + `persist` 原子替换                 |

## 日报保存链路

```text
输入 → edit() → 800ms 内的连续键入合并为一次撤销点 → 650ms 防抖 → flush()
flush() → 若已有保存任务则排队 → 循环直到 draft 与已保存内容一致
        → 后端校验 revision（内容 SHA-256）
        → 写入同目录临时文件 → fsync → 原子替换
        → 返回新 revision，前端更新索引并只刷新这一天的预览
```

要点：

- 保存失败会保留草稿并停止自动重试，必须显式重试，避免反复写坏磁盘。
- 保存期间继续输入不会丢失：`while (dirty)` 会一直写到最新内容。
- 外部编辑器改动同一文件时 revision 不匹配，保存被拒绝而不是覆盖。

## 设置链路

设置只有一个写入口：`save_workspace_settings`。

1. 设置页把改动保存在 `useSettingsDraft` 的草稿中，切换分类不丢失。
2. 点击“保存设置”时统一校验，通过后一次写入工作空间根目录的 `workreport.json`。
3. 写入成功后才调用各模块的 `acceptSaved()`，把草稿转为基线。
4. 未保存就离开会弹出确认框，可选择保存、放弃或继续编辑。

各模块不再保留自己的写命令，`load_*` 只负责读取；这样设置永远不会由两条路径写入同一份 `workreport.json`。

## AI 与 Excel

- 润色与总结只发送当前所选文本；CLI 在独立临时目录运行，不把工作空间作为工作目录。
- 润色弹窗打开时只准备原文，不发出请求；用户点击“开始润色”后才调用模型，确认使用才会回填日报。
- 看啦啦啦啦啦啦啦啦啦啦啦啦啦啦啦啦啦啦啦利益鸡鸡鸡鸡鸡鸡鸡鸡鸡鸡鸡鸡鸡鸡鸡鸡鸡鸡，。》》》》》》》》》》》》》》》》》》》》》》》》》》》》》》》

  Www'w'w'w'w'wWwwwwwww

- 确认时会重新校验工作空间、模板指纹、Excel 设置与所选日报，任一变化都要求重新生成预览，避免“预览的是 A、写出去的是 B”。

## 测试策略

| 层                         | 数量 | 覆盖重点                                                                                                    |
| -------------------------- | ---- | ----------------------------------------------------------------------------------------------------------- |
| 前端（Vitest）             | 203  | 日历与农历、保存串行化、设置草稿、AI 请求生命周期、Excel 范围校验、功能栏/皮肤/布局、App 挂载冒烟           |
| Rust（`cargo test --lib`） | 154  | 路径穿越与符号链接、原子保存、冲突、双存储与迁移回滚、Excel 预览与发布、历史容量清理、布局校验、AI 模拟接口 |

`tests/app.test.ts` 在 jsdom 中挂载整个 App，覆盖首页骨架、工作空间引导、设置页切换与确认弹窗的开关；其余界面行为仍由 `vue-tsc`、生产构建与人工操作把关。Rust 侧的临时文件与模拟 HTTP 服务覆盖真实磁盘和真实请求路径。

## 已知取舍

- 工作空间在进程内由单个 `Mutex` 保护，文件与数据库操作串行执行。单窗口场景足够，代价是大 Excel 输出期间其他操作排队。
- 没有端到端浏览器测试；`tests/app.test.ts` 只挂载首页与设置页骨架，弹窗交互之外的行为仍依赖人工操作。
- Rust 侧尚未接入 `rustfmt`/`clippy` 门禁。
