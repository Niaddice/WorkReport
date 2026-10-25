# 架构说明

这份文档描述 WorkReport 的运行时结构、关键链路和设计取舍。功能说明见 [README](README.md)。

## 分层

```text
Vue 界面层      AppRail / ModuleBoard / ModulePanel / 模块内容组件 / 布局与设置弹窗
      │
状态编排层      useJournal  useAi  useExcel  useSummaryOutput  useAppearance  useModuleLayout
      │
IPC（38 个）    文件读写 · 历史版本 · 模块布局 · AI 请求 · Excel 两阶段 · 设置与假日
      │
Rust 领域层     workspace  history  ui  ai  excel  database  holidays  settings  summary  appearance
      │
持久化          年份目录文件 / .history 版本快照 / SQLite / MySQL / PostgreSQL / 外部 AI 与 holiday-cn
```

前端不直接访问文件系统或网络：所有 IO 都通过 `invoke` 交给 Rust，WebView 的 CSP 只允许 `self` 与 `ipc:`。

## 模块化拖放界面

```text
registry.ts          模块元数据（id / 标题 / 图标 / 组件）
      │
ModuleBoard.vue      布局树 → 像素几何 → 绝对定位 ModulePanel + 间隙手柄
      │
ModulePanel.vue      模块外壳：拖动手柄、放大/还原、移除
      │
context.ts           provide/inject 的 ModuleContext（journal / ai / excel / summaryOutput / actions / handles）
      │
useModuleLayout.ts   归一化、增删、分栏、比例、皮肤 + 串行持久化
      │
save_ui_preferences → workreport.json 的 ui.layout（原子写入）
```

- 模块声明与渲染分离：新增模块只需登记 `ModuleId`、实现一个通过 `useModuleContext()` 取状态的组件，并在 `src/modules/registry.ts` 注册；模块 id 在 Rust 侧按安全字符串保存，新增模块不需要改后端 schema。模块 id 固定为日历、日报编辑、数据概览、历史记录、AI 总结、Excel 周报；最近日报、放假与调休模块已移除，放假信息保留在设置中。
- 右侧功能栏固定存在，进入设置页也不隐藏；设置按钮使用 `margin-top: auto` 固定在侧栏最下方，进入设置时高亮。点击模块/布局按钮会先切回首页，避免在设置页添加模块。原“布局与模块”目录弹窗改为只负责分栏操作提示与恢复默认的 `LayoutDialog`。
- 布局是嵌套分栏树：叶子 `{ type:'leaf', id }`，分支 `{ type:'split', direction:'row'|'column', ratio, first, second }`；根节点始终填满模块板，split 的 ratio 表示 first 的占比。`src/modules/layout.ts` 用纯函数完成递归归一化、旧 `columns`/`modules`/`axis`/`order` 迁移、增删/分栏/比例计算、几何切分与拖放命中判定，由 `tests/layout.test.ts` 覆盖。保存时携带工作空间路径，Rust 校验通过才写入，切换工作空间后迟到的保存会被拒绝。
- 拖放使用 Pointer Events 和几何计算，无第三方拖放库：按下手柄移动超过 4px 后开始拖动；指针进入另一个模块时按归一化偏移判定目标的上/下/左/右半边，渲染半区预览和“上方分栏/下方分栏”等标签；松开后 `splitLayout` 把目标叶子替换成新的 split。指针落在模块间的 gutter 上时改为高亮滑块并提示“插入此处”，松开后 `insertLayoutModuleAtGutter` 按边界叶子定位 split，把拖动模块插到相邻两个模块之间（三块各占 1/3）。目标不存在、拖回自身、拖出窗口、`Esc`、`pointercancel` 或窗口失焦都会取消，并带 aria-live 播报。
- AI 总结模块打开后由 `SummaryModule` 自动读取所选日期原文，`AiSummaryDialog` 内嵌视图直接显示 AI/直接总结按钮，不再有准备步骤或 setup 界面；未配置默认 AI 时隐藏 AI 按钮。右上角 AI 状态图标（已配置蓝色/未配置红色）跳转设置中的 AI 分类；结果区复制使用与日报编辑一致的图标按钮，footer 只保留导出与输出目录。历史记录、Excel 周报在模块内完成，不再弹窗：`AiSummaryDialog` / `ExcelPreviewDialog` 提供 `inline` 模式复用原有内容与交互，内嵌时渲染为普通文档流 `<dialog open>`，不进入顶层 modal。模块板不再为某类模块保留固定行高，所有模块共享一屏空间，内容在模块内部滚动。
- 分栏之间的间隙同时是比例手柄：`layoutGeometry` 为每个 split 计算覆盖 gap 区域的 gutter（含 0/1 路径、方向、parent 矩形）。拖动 gutter 时 ModuleBoard 用 `resizeLayoutLocally` 实时生成预览布局，松手才 emit 新比例并持久化；gutter 可聚焦后用方向键微调。
- `resizeLayoutLocally` 把 split 比例变化换算成根坐标位移，只增减间隙左右/上下两个边界叶子的占比，其他叶子占比不变，因此只改变相邻两个模块。仅当边界叶子到根的路径方向都与间隙一致时才生效；若路径穿过相反方向的嵌套分栏，无法同时保持其他叶子和几何对齐，会回退到整块子树缩放。
- 模块板不再用 CSS grid 排布：布局树递归切分出一个无滚动的矩形区域，模块面板按像素位置绝对定位。同一个 panel 组件始终以模块 id 为 key 复用，分栏和调整比例只改变它所在的绝对矩形，不会重建日历/编辑器或丢失内部状态。模块板本身 `overflow: hidden`，不出现滚动条；每个模块面板内部独立滚动。
- 布局动画统一用 Web Animations API 做 FLIP：分栏/增删/比例提交用 280ms；窗口尺寸变化由 `ResizeObserver` 驱动，回调里先捕获模块当前屏幕矩形，再更新几何并补 160ms 缩放过渡，因此连续拖动窗口、最大化/还原都会平滑跟随，而不是逐帧硬跳；拖动 gutter 只做局部几何预览；`prefers-reduced-motion` 下直接切换。
- 模块可以临时放大为覆盖 App 可视区域的悬浮层：遮罩铺在模块板之上，被放大的面板用 CSS `position: fixed` 提升到顶层（仍是同一个 DOM 节点，因此日历/编辑器的内部状态不重置）。过渡用 Web Animations API 做 FLIP：展开时从卡片当前矩形缩放平移到全屏矩形，还原时缩回原网格槽位，遮罩同周期淡入淡出；`prefers-reduced-motion` 下直接切换。`Esc`、遮罩点击或再次点击按钮还原；放大状态不写入持久化布局，切换工作空间时自动关闭。
- 全局瞬时提示统一走顶部 `MessageHost`：业务组件只调用 `pushMessage`，提示队列负责去重、自动关闭和操作按钮；表单字段校验与原生 dialog 内的错误保持原位，避免破坏表单上下文。总结导出、Excel 输出成功等全局反馈也通过顶部 Message 提示。

## 关键文件

| 文件                                       | 职责                                                                  |
| ------------------------------------------ | --------------------------------------------------------------------- |
| `src/App.vue`                              | 页面编排：工作空间门槛、首页/设置切换、框选总结、Excel 输出、弹窗协调 |
| `src/components/ModuleBoard.vue`           | 分栏几何、绝对定位、拖放分栏预览、间隙拖动、最大化与无障碍播报        |
| `src/components/ModulePanel.vue`           | 模块外壳：拖动手柄、放大/还原、移除                                   |
| `src/components/AppRail.vue`               | 固定右侧功能栏：模块入口、布局、皮肤、刷新，设置固定在底部            |
| `src/components/LayoutDialog.vue`          | 布局弹窗：分栏操作提示、恢复默认布局                                  |
| `src/components/modules/HistoryModule.vue` | 历史记录模块：当天版本列表与全文预览                                  |
| `src/components/modules/SummaryModule.vue` | AI 总结模块：自动读取原文、AI 状态入口、内嵌总结面板与导出            |
| `src/components/modules/ExcelModule.vue`   | Excel 周报模块：同周范围、生成预览、内嵌确认输出                      |
| `src/modules/registry.ts`                  | 模块注册表；`src/modules/layout.ts`：布局纯函数与旧布局迁移           |
| `src/modules/context.ts`                   | 模块运行上下文（composable、动作与 handles 的注入契约）               |
| `src/modules/useModuleLayout.ts`           | 布局状态、操作与串行持久化                                            |
| `src/lib/useJournal.ts`                    | 日报草稿、串行自动保存、撤销栈、格式切换、索引与预览失效              |
| `src/lib/useExcel.ts`                      | Excel 设置、日期范围校验、预览与确认两步调用                          |
| `src-tauri/src/workspace.rs`               | 年份目录索引、按需预览、原子保存、冲突检测、双存储与迁移              |
| `src-tauri/src/history.rs`                 | 日报版本快照：内容去重、按容量上限清理最旧日期、每天 100 份上限       |
| `src-tauri/src/ui.rs`                      | 模块布局偏好：layout 树透传、皮肤、旧布局迁移                         |
| `src-tauri/src/ai.rs`                      | CLI 检测与调用、HTTP 兼容接口、取消与超时、提示词拼装                 |
| `src-tauri/src/excel.rs`                   | 模板解析、AI 填表计划校验、样式写入、预览缓存与发布                   |
| `src-tauri/src/database.rs`                | SQLx 三库连接、工作空间隔离、按 revision 写入                         |
| `src-tauri/src/settings.rs`                | `workreport.json` 一次性读写，临时文件 + `persist` 原子替换           |

样式统一放在 `src/style.css`，组件不写 scoped 样式；模块样式使用 `.module-*`、`.overview-*`、`.history-module-*`、`.summary-module-*`、`.excel-module-*` 前缀。

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

模块布局与皮肤走独立的 `save_ui_preferences`，因为它们是即时生效的界面偏好，不参与设置草稿。两处写入都经过同一份 `workreport.json`，所以统一设置保存时如果载荷不带 `ui`，后端会保留磁盘上已有的布局，避免保存一次设置就清空拖放布局。

## AI 与 Excel

- 润色与总结只发送当前所选文本；CLI 在独立临时目录运行，不把工作空间作为工作目录。
- 润色弹窗打开时只准备原文，不发出请求；用户点击“开始润色”后才调用模型，确认使用才会回填日报。
- 确认时会重新校验工作空间、模板指纹、Excel 设置与所选日报，任一变化都要求重新生成预览，避免“预览的是 A、写出去的是 B”。

## 测试策略

| 层                         | 数量 | 覆盖重点                                                                                                                                                                                                   |
| -------------------------- | ---- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 前端（Vitest）             | 280  | 分栏树归一化/迁移/几何、PointerEvent 拖放分栏与预览、间隙拖动比例、模块面板/模块板、布局动画、顶部 Message、历史/总结/Excel 内嵌模块、右侧功能栏集成、日历与农历、保存串行化、设置草稿、AI 生命周期、Excel |
| Rust（`cargo test --lib`） | 165  | 路径穿越与符号链接、原子保存、冲突、双存储与迁移回滚、Excel 预览与发布、历史容量清理、UI 偏好迁移、AI 模拟接口                                                                                             |

`tests/appModules.test.ts` 在模拟的 Tauri IPC 下打开工作空间、从右侧功能栏添加模块并核对写回的分栏布局载荷；`tests/inlineModules.test.ts` 覆盖历史版本预览、总结准备与内嵌面板、Excel 预览内嵌；`tests/moduleBoard.test.ts` 用真实 PointerEvent 覆盖拖放分栏、半区预览、Escape 取消、间隙拖动比例与浮层放大/还原；`tests/layout.test.ts` 覆盖几何与迁移纯函数；`tests/app.test.ts` 覆盖首页骨架、工作空间引导与设置页切换。另外用无头 Chromium + CDP 做过拖动/键盘/取消的渲染验收，原生 WebKit 窗口仍建议人工复核。

## 已知取舍

- 工作空间在进程内由单个 `Mutex` 保护，文件与数据库操作串行执行。单窗口场景足够，代价是大 Excel 输出期间其他操作排队。
- 模块布局的保存是即时的（每次分栏/增删/调整比例/皮肤修改都会写入），没有单独的“保存布局”按钮；写入失败通过顶部 Message 提示。
- 拖放只在模块板内有效，不支持跨窗口拖放；模块数量上限为 12。模块板始终一屏，分栏越多每个模块越小，内容靠模块内部滚动；没有最小尺寸限制，必要时可使用模块放大浮层。
- 放大浮层复用同一个模块 DOM 节点而不是复制内容，因此不需要同步两份日历/编辑器状态；代价是放大期间只能操作该模块，且浮层内没有实现完整的焦点陷阱。
- 工作空间切换时本地立即重置布局、排队中的保存会被丢弃，已在途的保存由 Rust 按载荷中的工作空间路径校验拒绝，因此不会把旧布局写进新空间。
- 没有常驻的端到端浏览器测试；`tests/app.test.ts` 与 `tests/appModules.test.ts` 在 jsdom/happy-dom 中挂载界面，本次另用无头 Chromium + CDP 做过一次渲染与拖放验收，但真实鼠标拖放仍建议在桌面端人工复核。
- Rust 侧尚未接入 `rustfmt`/`clippy` 门禁。
