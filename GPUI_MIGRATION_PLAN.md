# WorkReport Rust + GPUI 迁移计划

以 `feature-gpui` 的起始提交 `b39ffee`（WorkReport `0.1.8`）为基准。建议先在现有开发机验证 macOS Apple Silicon，随后补齐计划发布的平台与架构。最终交付原生 GPUI 桌面应用，覆盖现有发行版的业务流程、数据兼容和自动更新。迁移期间保留 Vue/Tauri 版本用于开发、对照和回退。 [现有功能说明](README.md) · [当前架构](ARCHITECTURE.md)

## 当前检查点

本次保存位于 `feature-gpui`，阶段 2 仍在推进。共享核心抽离、GPUI 工作空间恢复、新建/编辑/自动保存、范围预览、历史浏览、周/月/年日历、拖动选择、农历、节假日与摘要已实现。当前默认发行入口仍为 Vue/Tauri。

- 最近验证通过 75 项 GPUI 测试、303 项共享核心测试、50 项 Tauri 测试及 1,086 项前端测试。macOS 调试原型包构建和本地签名校验通过。
- 下一步先完成基础分栏和布局状态，再接历史 Diff/还原与仅数据库模式，随后按阶段 3 迁移其余工作台模块。
- 真实年度联网下载、原生窗口视觉复核、系统输入法及屏幕阅读器实机验收仍待完成。Windows/Linux 编译工作流已配置，等待托管 CI 执行。

## 技术路线

- **复用 Rust 后端。** 建议建立 Cargo workspace，将工作空间、文件规则与迁移、历史、数据库、AI、Git、节假日、总结导出和 Excel 抽入 `workreport-core`，保留现有类型、规则和测试。将 `src-tauri/src/lib.rs` 中的工作空间切换、设置事务、Excel 快照等用例编排一并抽出。Tauri command 保留为薄适配层，`workreport-gpui` 通过同一套 Rust 接口调用业务。系统配置目录、选文件、剪贴板、打开 URL/目录、应用菜单、关闭窗口与更新由桌面入口处理。 [Tauri 入口](src-tauri/src/lib.rs) · [工作空间实现](src-tauri/core/src/workspace.rs)
- **重建界面与状态编排。** 用 GPUI 的 `Entity`、`Render` 和 `Action` 管理模块状态、渲染和快捷键。将布局归一化、日期范围、草稿保存、Excel 会话等独立于 Vue 的规则移植成可单测的 Rust 逻辑。按模块 ID 复用视图实例，拖放、调整分栏、最大化时保留编辑状态。 [模块注册表](src/modules/registry.ts) · [布局算法](src/modules/layout.ts) · [GPUI API](https://docs.rs/gpui/0.2.2/gpui/)
- **沿用现有异步语义。** 保留 Tokio、SQLx 和现有后台任务机制，将文件、数据库、Git、AI 和 Excel 操作放在后台执行。将 IPC 进度换成类型化事件，按操作使用工作空间、请求 ID 和 revision 关联结果，保留取消、超时与过期结果检查。串行保存日报草稿和设置，关闭及切换流程等待相应的保存结果。 [保存与冲突处理](src/lib/useJournal.ts) · [数据库运行时](src-tauri/core/src/database.rs)

## 实施阶段

| 阶段                     | 实施内容                                                                                                                                                                                | 验收标准                                                                                                                                                            |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **0 · 基线与原型**       | 记录当前测试和安装行为。用脱敏工作空间验证 GPUI 窗口、中文输入、剪贴板、原生目录选择、快捷键和屏幕阅读器。做富文本风险原型及 macOS 安装包探针，同时在 Windows、Linux 建立最小编译检查。 | 在 macOS arm64 启动原型。中文组字、光标与选区、撤销、表格编辑和 Markdown 往返有可复现的结果。记录所选 GPUI/组件库、固定 Rust 工具链和锁文件，以及各平台的待解事项。 |
| **1 · 抽离共享核心**     | 提取业务与用例编排，保留 Tauri 切换前的调度和数据格式。补齐旧版配置、数据库和文件备份样本。                                                                                             | 继续通过原有 Rust 和 Tauri 测试。两个入口先后使用同一份样本，均可读日报、保存并重新打开，外部修改冲突时保留草稿。                                                   |
| **2 · 日报闭环**         | 实现首次选择及恢复工作空间、日历、单日编辑、Markdown/TXT 保存、范围预览、历史和基本分栏。源码加预览可用于内部原型。                                                                     | 用复制的工作空间完成“选择日期 → 中文编辑 → 自动保存 → 重启 → 查看历史”。保留 650ms 防抖和串行保存。切换日期、工作空间或关闭前保存，失败时保留草稿供继续编辑或重试。 |
| **3 · 工作台和业务补齐** | 完成所见即所得编辑、全部模块与分栏交互、目录/搜索、全局和模块设置。先迁移 AI 配置和请求状态，再迁移 Excel，Git 工作台可并行。                                                           | 八个模块与现有关键场景通过回归，Markdown 通过保真用例。Excel 依次通过正文确认、填表预览和确认输出。测试文件、双存储、仅数据库三种模式，以及备份恢复。               |
| **4 · 安装、升级和切换** | 在独立预览渠道分发签名包。跑目标平台安装、升级、系统凭据、回退和真实交互验收。通过后将 GPUI 设为默认发行版，再移除 Vue/Tauri 运行时及相关构建入口。                                     | 实测现有 Tauri 版升级到 GPUI、GPUI 自更新、归档 Tauri 版回退和数据重开。发布矩阵中每个目标均通过安装与升级验收后才切换对应正式渠道。                                |

阶段 0 的首个可评审交付是 **macOS arm64 原型、原型验证记录、依赖选型和旧版样本**。当前开发机只有 Command Line Tools，`xcrun --find metal` 失败。原型选型需验证运行时着色器能否构建并启动，正式分发单独验收签名与公证。GPUI 仍处于 1.0 之前，发行版和上游主线的启动 API 及平台说明有差异，应针对选定版本验证。 [GPUI 0.2.2 构建脚本](https://docs.rs/crate/gpui/0.2.2/source/build.rs) · [GPUI 上游平台说明](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md)

## 正式切换验收

| 范围                   | 必须验证的行为                                                                                                                                                                                                                                                                                                                                                                                                          |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **编辑和预览**         | 中文输入法组字确认与取消、中英文混排和 emoji 的光标及选区、复制粘贴、撤销重做、快捷键焦点。标题、列表、待办、链接、引用、代码和可编辑表格保留。打开未编辑的文件不触发重写，未支持的 Markdown/HTML 保留原文，远程图片沿用现有处理。编辑/预览分栏保持滚动同步。 [编辑器回归](tests/richMarkdownEditor.test.ts) · [Markdown 保真测试](tests/richMarkdown.test.ts)                                                          |
| **工作台和设置**       | 日历周/月/年及日期框选、数据概览及热力图、历史 Diff/还原、工作空间目录和全局搜索。分栏拖放、间隙插入、调整比例、全关、最大化、命名布局与旧布局迁移。保留功能栏停靠、字体缩放和已保存皮肤。验证全局及模块设置草稿、校验、离开确认、代理和节假日同步。真实系统验证 Tab 焦点、屏幕阅读器、系统菜单和减少动态效果。 [模块注册表](src/modules/registry.ts) · [现有布局测试](tests/moduleBoard.test.ts)                       |
| **AI、Git 和文件操作** | AI 的 API/CLI 选择、流式结果、取消与超时，日报润色 Diff、直接及 AI 总结、Markdown/TXT 导出。Git 多仓库、状态/Diff、暂存/提交、仅快进拉取、分支/Stash、GitHub CLI 授权和提交生成日报。目录操作的预览与确认，普通文本编辑及 CSV、`.xls`/`.xlsx` 只读预览。切换工作空间、日期或仓库后，旧任务结果不覆盖当前界面。 [Git 日报与工作台](README.md#git-日报与工作台) · [工作空间文档测试](tests/appWorkspaceDocuments.test.ts) |
| **Excel 输出**         | 保持“生成可编辑正文 → 用户确认后填表 → 工作簿预览 → 确认输出”。填表失败保留确认正文，取消或快照过期不发布文件，确认输出复用已预览工作簿。验证跨周/月/年范围、多工作表、单元格样式、合并区域、公式及已有文件不被覆盖。 [两阶段工作流测试](tests/excelWorkflowDialog.test.ts) · [Excel 实现](src-tauri/core/src/excel.rs)                                                                                                 |

富文本选型在阶段 0 比较直接使用 GPUI 输入能力与采用 GPUI Kit。Kit 的源码编辑器和块渲染接口可用于原型，结构化编辑命令、选区、撤销、语义和原文保留仍需验证。统一锁定一套兼容依赖，避免同时引入不同版本的 GPUI 类型。若富文本原型未达到保真要求，继续以内测验证日报闭环，正式渠道继续发行 Tauri 版，直至原生编辑通过验收。 [GPUI 输入示例](https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/input.rs) · [GPUI Kit 编辑器文档](https://github.com/longbridge/gpui-kit/blob/main/website/component/editor.md) · [GPUI Kit 架构](https://github.com/longbridge/gpui-kit/blob/main/docs/ARCHITECTURE.md#text-rendering-for-external-components)

## 数据、发布与回退

- **原样接续工作空间。** 首次打开沿用 Markdown/TXT、历史快照、`.workreport-files.json`、`workreport.json` 的字段和 `ui.layout`，保留原子保存、revision 检查、自定义目录及文件名、旧布局兼容和目录安全边界。继续读取 SQLite、MySQL、PostgreSQL 的 `workspaceId` 和原表结构，双存储沿用文件为权威的规则。用脱敏样本覆盖默认和自定义路径、数据库断线、迁移中途失败、重试和跨版本双向读写。预览使用独立应用配置目录与工作空间副本，避免两个版本同时写入同一目录。 [设置格式](src-tauri/core/src/settings.rs) · [文件规则](src-tauri/core/src/file_rules.rs) · [数据库实现](src-tauri/core/src/database.rs)
- **迁移本机状态。** 正式应用兼容 `com.workreport.desktop` 的现有配置目录、最近工作空间、旧 AI/外观配置回退、网络代理、节假日和加密 Git 凭据。保留系统密钥存储标识和加密格式，在签名安装包上验证权限。更新渠道目前存在 WebView `localStorage`，由过渡版将其写入本机配置，验证 `main`、`dev` 选择和经确认切回较低版本。直接跨版本安装时，在首次检查更新前让用户确认渠道。 [应用标识](src-tauri/tauri.conf.json) · [Git 凭据](src-tauri/src/git_credentials.rs) · [更新渠道偏好](src/lib/useUpdater.ts)
- **验证安装和更新合同。** 用 `cargo-packager` 验证 `.app`/DMG、NSIS、AppImage 等相应产物，以既有 Tauri `latest.json`、签名公钥、平台包格式和渠道行为做升级试验。当前清单的平台条目只有 `signature` 与 `url`，`cargo-packager-updater` 文档的条目另需 `format`，预留兼容清单或过渡包方案。验证篡改包被拒绝、代理、下载失败重试、手动下载、保存失败或后台任务时暂缓安装、安装后重启。macOS 签名和公证按既有分发要求验证。 [当前清单脚本](scripts/updater-manifest.mjs) · [Tauri 更新文档](https://v2.tauri.app/plugin/updater/) · [Cargo Packager](https://github.com/crabnebula-dev/cargo-packager) · [候选更新清单格式](https://docs.rs/crate/cargo-packager-updater/0.2.3#endpoint-response) · [Apple 分发文档](https://developer.apple.com/documentation/xcode/packaging-mac-software-for-distribution)
- **保留可执行的回退路径。** 归档最后一版签名 Tauri 安装包。发布遇到数据读写、凭据或更新问题时，暂停该渠道继续推送。用户先退出 GPUI，再通过归档安装包回到旧版，用同一份测试样本验证旧版可读取 GPUI 保存后的日报、设置和数据库。兼容期间将任何新增数据格式变更独立评审。 [现有发布流程](README.md#应用自动更新)

## 测试与交付顺序

1. 在基线提交运行 `npm run verify`、`cargo test --manifest-path src-tauri/Cargo.toml --lib` 和 `npm run test:release`，记录结果与失败。现有行为以运行中的应用、源代码和可复现的测试为准。 [现有 CI](.github/workflows/ci.yml) · [发布脚本测试](tests/release_test.py)
2. 按“原型 → 共享核心 → 日报闭环 → 模块迁移 → 安装升级”拆成可独立审查的变更。双入口运行期间继续跑旧版 CI，新增核心 Cargo 测试和 GPUI 状态、焦点与快捷键测试。将关键 Vitest 场景改写为 Rust/GPUI 行为测试。 [GPUI 测试示例](https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/testing.rs)
3. 在目标机器用安装包验证系统交互，包括 macOS arm64、Windows 和 Linux 中实际发布的架构及后端。记录旧版与 GPUI 在同一脱敏工作空间上的冷启动、目录/搜索响应和长文编辑体验，用基线排查明显退化。阶段 0 完成后，依据富文本与更新链路的原型结果估算剩余工作量。

## 阶段 0 执行记录

- `b39ffee` 基线通过。`npm run verify` 的 87 个测试文件、1,086 项前端测试通过。Rust 在允许测试监听本机回环端口后，330 项测试通过。18 项模拟发布测试通过。
- 原型固定 GPUI Kit `0.10.0` 和依赖锁文件，使用 Rust 1.99.1 验证。该版本的 macOS 平台依赖已启用运行时 Metal 着色器，阶段 0 可使用现有 Command Line Tools。 [原型启动与验证说明](src-gpui/README.md)
- macOS arm64 已完成 debug 与 release 构建、本地 `.app` 签名校验及 release 窗口启动。4 项原型单元格测试通过，覆盖 Unicode、CRLF、原文字节保留和结构性输入拒绝。中文输入法组字、VoiceOver、系统选择框交互与签名升级回退仍待实机验收。
- 已加入脱敏旧版工作空间，包含自定义路径、文件索引、旧布局、Markdown/TXT 和历史快照。现有 Rust 后端通过新增样本回归，验证日报身份、历史快照和只读打开的字节保真。新增测试后完整 Rust 测试为 331 项通过。 [样本及期望行为](fixtures/gpui-migration/README.md)
- 已配置 Windows 与 Linux 原型编译检查，待 GitHub 托管环境执行。 [原型工作流](.github/workflows/gpui-prototype.yml)

## 阶段 1 执行记录

- Tauri 和 GPUI 均引用无 Tauri 依赖的 `workreport-core`。现有工作空间、历史、数据库、文件迁移、AI、Git 和 Excel 实现及测试已迁入共享核心。现有 `src-tauri` 构建命令默认覆盖核心和 Tauri 适配层。 [共享核心](src-tauri/core/Cargo.toml) · [Tauri 入口](src-tauri/src/lib.rs)
- GPUI 通过后台任务读取本地日报和历史快照，内置样本草稿在视图切换期间保留。只读入口覆盖文件和双存储的本地文件，跳过数据库连接及写权限探测。仅数据库模式暂由 Tauri 处理。 [只读入口](src-tauri/core/src/workspace_preview.rs) · [原型运行方式](src-gpui/README.md)
- 锁定依赖验证通过 286 项共享核心测试、50 项 Tauri 测试和 18 项模拟发布测试。macOS 的 Tauri 调试应用包仍从原路径生成，应用标识与版本分别为 `com.workreport.desktop` 和 `0.1.8`。 [验证命令](README.md#开发与构建)
- macOS arm64 的 GPUI 应用包使用 `--workspace` 打开脱敏副本，窗口显示三篇日报、最新 Markdown 正文及两份历史快照。浏览后副本与样本目录逐字节一致。
- GPUI 现有 Markdown/TXT 日报已接入显式保存。共享编辑会话在每次保存校验实际路径、存储配置和 revision，沿用原子写入、历史快照及双存储备份。保存冲突保留编辑草稿，后续输入独立于正在保存的内容。返回浏览、窗口关闭和应用菜单退出会检查是否有未保存日报。 [共享编辑会话](src-tauri/core/src/journal_edit.rs) · [GPUI 原型验证方式](src-gpui/README.md)
- GPUI 集成测试已通过编辑按钮、保存操作、重开、撤销关闭确认和草稿复制的完整流程，并模拟了输入法组字确认前后的保存。系统强制退出后的草稿恢复与仅数据库模式仍待接入。

## 阶段 2 执行记录

- 已有 Markdown/TXT 日报接入 650 毫秒防抖自动保存。只提交已确认的输入，单个日报的写入串行进行，保存结果只更新已持久化的版本。文件冲突或写入错误时保留草稿并暂停自动保存，手动保存成功后恢复。
- 返回浏览、关闭窗口或通过应用菜单退出时，先等待进行中的写入，再保存最新草稿。自动快照沿用原有限频规则，显式保存或离开时补记最后已保存的版本。离开时遇到保存失败可继续编辑或放弃草稿。 [验证步骤](src-gpui/README.md)
- 已接入按日期新建 TXT/Markdown 日报，格式沿用工作空间设置。空白草稿不生成文件，首次非空输入由自动保存按自定义路径发布，并更新日期列表和历史。已有日期打开原格式，外部创建同日期日报时保留草稿并拒绝覆盖。 [新建与保存会话](src-tauri/core/src/journal_edit.rs)
- 已记录最近成功打开的工作空间，启动时自动恢复，命令行路径可覆盖本次选择。打开失败保留先前记录，记录损坏或目录失效时可重新选择。记录保存在 GPUI 原型自己的本机配置目录。 [本机记录](src-gpui/src/recent_workspace.rs)
- 共享核心的两个入口在新建日报时持有跨进程创建锁，覆盖日期检查和文件发布。锁文件 `.workreport-create.lock` 保留在工作空间内，文件句柄关闭或进程退出时释放锁。 [共享工作空间](src-tauri/core/src/workspace.rs)
- 新建日报与启动恢复已通过 25 项 GPUI 测试。共享核心的 303 项测试和 Tauri 的 50 项测试通过，包含跨进程锁占用、进程退出后释放及不同格式竞争创建。Vue 的规范检查、构建及 1,086 项前端测试通过。macOS 调试包已重建并校验本地签名。
- 已加入原生月历，覆盖周一起始、已有日报标记、跨年翻月、今天定位与空白日期新建。列表和表单会定位到所选月份，邻月日期点击及进入编辑保留正在查看的月份。读取失败保留先前选区和正文，关闭空白草稿显示所选日期的空状态。 [日历状态](src-gpui/src/calendar.rs) · [日历界面](src-gpui/src/calendar_view.rs)
- Shift 点击可选择包含两端的连续日期范围，在后台读取实际存在的日报并倒序预览。Markdown 和 TXT 分别按原格式显示，可切换合并原文或复制，保留正文中的 Unicode、CRLF、HTML 与 YAML。缺失日期不创建文件，范围内任一读取失败保留先前预览。Esc 或「退出范围」直接清除范围，空范围退出不会新建日报。 [范围读取](src-gpui/src/workspace_range.rs)
- 日历和范围功能的回归扩展至 38 项 GPUI 测试，覆盖跨年、原文保真、自动保存、读取失败、空范围退出与工作空间切换。共享核心 303 项测试及 Tauri 50 项测试继续通过。
- macOS 调试包已构建、校验本地签名，并启动到脱敏工作空间副本。启动后副本文件哈希与启动前一致。当前环境的窗口截图未成功，真实窗口的视觉布局仍待复核。
- 已补齐周、月、年视图，按当前周期翻页。年视图可滚动浏览十二个月，自动定位当前月份，点击月份标题进入月视图。视图切换保留选择，外部日期选择定位对应时段，周与年导航覆盖跨年及闰日边界。 [日历状态](src-gpui/src/calendar.rs)
- 三种视图均支持正反方向鼠标拖动范围。拖动只更新临时高亮，松开后读取；日历外释放可结束，Esc 和窗口失焦可取消。尾部鼠标点击及拖回起点不会误开编辑器，下次单击和键盘激活仍可用。月视图邻月日期沿用可点击、不可参与拖动的规则。退出范围恢复之前查看的单日，并保留当前视图和显示时段。 [日历交互](src-gpui/src/calendar_view.rs)
- 回归扩展至 53 项 GPUI 测试，其中新增 5 项状态测试及 10 项原生事件测试，验证全年滚动、十一月首帧定位、跨视图日期同步、范围替换、取消和原文保真。
- 已接入农历、休息/补班标记、周末样式及已有日报摘要。农历月初显示简体月名并保留闰月，支持范围外安全省略。周/月可见日期的摘要在后台使用共享核心的 16 KiB/80 Unicode 字符规则，年图不读取全年文件；保存及窗口重新激活后刷新，使用工作空间实例、请求编号和保存版本拒绝过期结果。当天标记在跨日和窗口激活时刷新，不移动原有选区。 [农历](src-gpui/src/lunar.rs) · [只读摘要](src-tauri/core/src/workspace_preview.rs) · [后台状态](src-gpui/src/calendar_details.rs)
- 放假信息沿用共享核心的校验、手动同步与失败保留规则。启动只读优先原型缓存、缺失时回退旧版；首次显式同步前原子导入全部有效旧年度，写入仅限原型配置目录。新版年度通知优先覆盖前一年十二月，同步年度为权威，不复活旧离线日期。坏缓存与网络错误独立显示，不覆盖日报状态或选区。Vue 与 GPUI 现共用同一份离线安排，GPUI 沿用旧版代理配置。 [放假信息适配](src-gpui/src/calendar_holidays.rs) · [共享离线数据](shared/calendar/holidays.json)
- 本轮 75 项 GPUI 测试全部通过，覆盖农历边界、旧年度导入、缓存错误、同步去重与失败重试、过期加载、快速切换和保存后的摘要刷新。共享核心 303 项及 Tauri 50 项测试继续通过，Vue 的规范检查、构建及 1,086 项测试通过。联网下载使用离线响应验证，尚未执行真实年度下载；分栏拖放、历史 Diff/还原及仅数据库模式仍待迁移。
