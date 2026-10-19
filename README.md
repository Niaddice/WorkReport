# WorkReport

Rust + Tauri 2 本地工作日报应用。前端使用 Vue 3、TypeScript 与官方 Material Web 组件，界面参考 Google Pixel / Material 3。

首页包含周／月／年日历和日报编辑区。日历保留周一开头、相邻月份日期、今天定位、农历及原项目节假日标记。右上角“设置”打开工作空间和 AI 设置。

桌面窗口最小为 960 × 680。顶部栏固定，首页日历与编辑区随窗口宽高伸缩；较矮窗口用圆点标记已有日报。正文、预览与设置内容在各自区域内滚动，首页不会因固定面板高度产生整页滚动条。

## 日报与工作空间

1. 每次启动后选择文件夹作为工作空间，或确认使用上次的工作空间。
2. 单击日期查看；双击日期（键盘回车也可）确认新增，默认 Markdown，也可选 TXT。
3. 文件保存在工作空间根目录，以 `yyyymmdd.md` 或 `yyyymmdd.txt` 命名。已有文件会直接打开。
4. 停止输入 650ms 后自动保存，也支持 `⌘S` / `Ctrl+S`。切换日期、工作空间或关闭窗口前会先保存。
5. 编辑区的 Markdown / TXT 开关会先请求确认，再更换文件扩展名。正文逐字保留，不删除 Markdown 标记，不覆盖已经存在的目标文件。
6. “复制”文字操作把当前编辑器的全部正文复制到系统剪贴板，包括尚未自动保存的编辑。
7. 在“设置 → 工作空间”切换文件夹。点击首页左下角路径旁的文件夹图标，可在系统文件管理器中打开当前工作空间。首页“本地工作空间”旁的“刷新”重新读取磁盘文件；返回应用时也会自动刷新无修改的日报。

```text
你的工作空间/
├── 20261018.md
├── 20261019.txt
└── ...
```

日报正文全部保存在工作空间，没有数据库或浏览器存储。应用配置目录保存最近工作空间路径和 AI 配置。其他非日期文件不会被修改。若其他编辑器改动了同一文件，保存会报告冲突并保留草稿，可复制草稿后重新载入。

## AI 润色

在“设置 → AI 润色”配置；选择默认 AI 后立即保存，其他修改可点击“保存 AI 设置”，返回首页或关闭窗口时也会先保存。保存失败会保留编辑并显示错误：

- **本机 CLI**：“检测本机 CLI”检测 Codex、Claude Code、OpenCode；支持常见 PATH、Homebrew、用户安装目录、NVM，以及 macOS Codex / ChatGPT 客户端内置的 Codex CLI。检测只查询版本，不调用模型。添加后可编辑 CLI 路径及可选模型 ID。需先在终端完成该 CLI 的登录。
- **模型接口**：支持 Chat Completions 兼容 API，包括 DeepSeek、Gemini 及自定义服务。填写接口地址、API Key（本地免密接口可留空）后，点击“获取模型”读取模型列表并选择，也可手动填写模型 ID。可填基础地址或完整 `/chat/completions` 地址；获取列表使用相同路径下的 `/models`，沿用当前填写的密钥，无需先保存。重新获取不会自动替换已选模型；更换地址或密钥会清空旧列表。所选模型随 AI 设置保存，获取失败或列表为空时仍可手动填写。
- **默认 AI**：由用户手动选择一个；添加和检测不会自动选择。也可使用“暂不设置默认 AI”。
- **AI 生成倾向**：填写希望的语气、篇幅、结构和表达侧重点（最多 2000 字），例如“简洁客观，突出已完成事项和下一步计划，保留原有条目结构”。对所有 CLI 和模型接口生效，包括重新生成；留空时保持原有润色方式。沿用同一套 AI 设置保存逻辑，旧配置可直接使用。

常用基础地址：

| 服务 | 基础地址 |
| --- | --- |
| DeepSeek | `https://api.deepseek.com/v1` |
| Gemini 兼容接口 | `https://generativelanguage.googleapis.com/v1beta/openai` |
| 自定义服务 | 服务商提供的兼容接口地址 |

编辑日报时点击“AI 润色”。未配置默认 AI 时打开设置；已配置时只将当前日报文本交给选中的 AI，CLI 在独立临时目录中运行，不把工作空间作为工作目录。

结果通过弹窗与原文并排展示，可修改结果、重新生成、恢复本次原文或取消。只有“确认使用”才会回填并保存。“重新生成”始终使用本次原文。回填后编辑区出现“恢复原文”，可恢复当前会话内保留的原始文本；退出应用后该临时撤销记录不会保留。关闭润色弹窗会取消尚未完成的请求。

API Key 保存在本机应用配置 `ai.json` 中；Unix 文件权限为 `0600`，未写入日报或日志，未加密。HTTP 请求在 Rust 中发出，拒绝自动跳转，错误信息不会回显服务商响应中的敏感内容。CLI 使用其已有账号与计费配置。Claude Code 和 OpenCode 的适配已实现，实际可用性取决于本机安装、版本及登录状态。

参考：[Codex 非交互调用](https://learn.chatgpt.com/docs/non-interactive-mode)、[Claude Code CLI](https://code.claude.com/docs/en/cli-reference)、[OpenCode CLI](https://opencode.ai/docs/cli/)、[DeepSeek 模型列表](https://api-docs.deepseek.com/api/list-models/)、[Gemini 兼容接口](https://ai.google.dev/gemini-api/docs/openai#list-models)。

## 开发与构建

需要 Node.js 22.12+、npm、Rust 1.90+ stable，以及 [Tauri 平台依赖](https://v2.tauri.app/start/prerequisites/)。macOS 需要 Xcode Command Line Tools。

```sh
npm ci
npm run app:dev
```

```sh
npm run build          # TypeScript 检查及前端构建
npm test               # 日历、保存、格式切换及 AI 预览生命周期
npm run test:rust      # 文件边界、格式切换、AI 配置与本地 HTTP 模拟接口
npm run app:build      # 桌面安装包
```

macOS `.app` 输出到 `src-tauri/target/release/bundle/macos/`。`npm run dev` 仅提供浏览器界面预览；工作空间、剪贴板、CLI 和 AI 设置需要 Tauri 桌面环境。

## 一键提交与发布

仓库固定为 [Niaddice/WorkReport](https://github.com/Niaddice/WorkReport)。安装 Git、GitHub CLI（`gh`）及上面的构建依赖，首次使用时登录，并确认已配置 Git 提交身份：

```sh
gh auth login --hostname github.com
git config --global user.name "你的姓名"
git config --global user.email "你的 GitHub 邮箱"
```

在项目中执行一个命令；macOS 也可以在 Finder 中双击 `release.command`：

```sh
./release.sh
```

脚本会连接已有仓库、同步版本号、安装依赖、提交更改、运行前端和 Rust 测试、构建安装包、生成 SHA-256 校验和，再推送当前分支及标签并发布 GitHub Release。首次连接时沿用远端默认分支和提交历史，保留本地现有文件，并恢复远端独有文件。已有 Git 仓库使用当前分支；远端有尚未合并的提交时会停止，需先自行同步。

所有未被 `.gitignore` 忽略的更改都会提交，包括新增文件和删除。版本会同步到 `package.json`、`package-lock.json`、Tauri 配置、`Cargo.toml` 和 `Cargo.lock`。默认沿用项目版本；标签或 Release 已存在时自动递增补丁版本。指定的版本已存在时会报错，不覆盖已有发布。

```sh
./release.sh --dry-run                     # 离线预览，不修改任何文件
./release.sh 0.1.1                         # 指定版本
./release.sh 0.1.1 -m "优化模型选择"         # 自定义提交说明
./release.sh --notes-file release-notes.md  # 从文件读取发布说明，默认自动生成
./release.sh --draft                       # 上传草稿，留待手动发布
./release.sh --resume v0.1.1                # 推送/上传失败后，使用原产物重试
```

产物保存在 `.release/v版本号/`，不会提交到仓库。仅构建当前电脑的平台和架构：macOS 上传包含完整 `.app` 的 ZIP；Linux 上传 DEB 和 AppImage；Windows 在 Git Bash 中执行，上传 MSI 和 EXE。安装包沿用项目及本机的 Tauri 签名配置。

测试或构建失败时不会推送，但本地版本修改和已完成的提交会保留。推送、上传失败时，按提示使用 `--resume`：脚本会核对提交、分支和安装包校验和；已存在的草稿可继续上传，正式 Release 的附件保持原样。重试沿用原来的草稿模式，不需要重新构建。脚本不强制推送；多个发布进程通过 `.release/.publish.lock` 互斥。

脚本自身的集成测试使用临时 Git 仓库和模拟 GitHub/构建命令，全程禁用网络传输；运行它需要 Python 3：

```sh
npm run test:release
```

## 源码

- `src/components/CalendarPane.vue`：日历；`JournalEditor.vue`：两种格式编辑和文字操作。
- `src/components/SettingsView.vue`：工作空间和 AI 配置；`AiPolishDialog.vue`：原文与结果对照。
- `src/lib/useJournal.ts`：串行自动保存、文件格式切换和草稿保护；`src/lib/ai.ts`：AI 设置、请求及取消。
- `src-tauri/src/workspace.rs`：日期文件索引、创建、读取、原子保存和格式切换，拒绝路径穿越和符号链接。
- `src-tauri/src/ai.rs`：CLI 检测与调用、配置持久化、自定义 HTTP 接口及超时取消。

Excel、邮件配置、报告风格、模板管理、生成配置和报告生成功能均未包含。历史 SQLite 数据不会自动迁移。节假日沿用原项目内置表，更新年度安排时请维护 `src/lib/chinaHolidays.ts`。
