import { invoke, isTauri } from '@tauri-apps/api/core'
import { ref } from 'vue'

export interface GitRepository {
  id: string
  name: string
  path: string
  author: string
  branch: string
}

export interface GitSettings {
  executable: string
  repositories: GitRepository[]
  includeMerges: boolean
  commitPrompt?: string
}

export const DEFAULT_GIT_COMMIT_PROMPT =
  '根据暂存区的更改生成简洁准确的中文 Git 提交说明。首行使用 Conventional Commits 格式（如 feat:、fix:、refactor:），概括主要改动；必要时空一行后列出关键变化。只描述实际更改，不编造测试结果或工作成果，只输出提交说明正文。'

export interface CommitPromptPreset {
  id: string
  label: string
  prompt: string
}

export const COMMIT_PROMPT_PRESETS: CommitPromptPreset[] = [
  {
    id: 'conventional',
    label: 'Conventional (默认)',
    prompt: DEFAULT_GIT_COMMIT_PROMPT,
  },
  {
    id: 'gitmoji',
    label: 'Gitmoji 风格',
    prompt:
      '首行使用 Gitmoji 与语义化提交格式（如 ✨ feat:、🐛 fix:、💄 style:、♻️ refactor:、📝 docs:），概括主要改动；必要时空一行列出具体变更清单。只描述实际更改，不编造测试结果，只输出提交说明正文。',
  },
  {
    id: 'chinese',
    label: '中文动词简短',
    prompt:
      '使用简明中文（如“新增...”、“修复...”、“优化...”、“重构...”），首行不超过50字概括改动；必要时空一行分条说明核心修改。只输出提交说明正文，不使用英文标签。',
  },
  {
    id: 'angular-scope',
    label: 'Angular (含 Scope)',
    prompt:
      '首行严格遵循 Angular 规范格式：<type>(<scope>): <subject>，例如 feat(auth): 支持扫码登录，50字以内；必要时空一行书写具体改动点。只输出提交说明正文。',
  },
]

export const DEFAULT_GIT_SETTINGS: GitSettings = {
  executable: 'git',
  repositories: [],
  includeMerges: false,
  commitPrompt: DEFAULT_GIT_COMMIT_PROMPT,
}

export function cloneGitSettings(value: GitSettings = DEFAULT_GIT_SETTINGS): GitSettings {
  return {
    executable: value.executable,
    repositories: value.repositories.map((repository) => ({ ...repository })),
    includeMerges: value.includeMerges,
    commitPrompt: value.commitPrompt ?? DEFAULT_GIT_COMMIT_PROMPT,
  }
}

export function validateGitSettings(settings: GitSettings): string {
  if ((settings.commitPrompt?.length ?? 0) > 2000 || settings.commitPrompt?.includes('\0'))
    return 'Git 提交说明提示词不能超过 2000 字，且不能包含空字符。'
  const absolute = (path: string) => /^(?:\/|[a-z]:[\\/]|\\\\)/i.test(path)
  if (
    !settings.executable.trim() ||
    /[\x00-\x1f\x7f]/.test(settings.executable) ||
    (settings.executable.trim() !== 'git' && !absolute(settings.executable.trim()))
  )
    return 'Git 程序请填写 git 或可执行文件的绝对路径。'
  if (settings.repositories.length > 20) return 'Git 仓库最多配置 20 个。'
  const paths = new Set<string>()
  for (const repository of settings.repositories) {
    if (!repository.name.trim() || repository.name.length > 100) return '请填写仓库名称，最多 100 字。'
    if (!absolute(repository.path.trim()) || /[\x00-\x1f\x7f]/.test(repository.path))
      return `${repository.name}：请填写仓库目录的绝对路径。`
    const path = repository.path.trim().replace(/[\\/]+$/, '')
    if (paths.has(path)) return '同一 Git 仓库目录不能重复添加。'
    paths.add(path)
    if (
      repository.author.length > 500 ||
      repository.branch.length > 500 ||
      /[\x00-\x1f\x7f]/.test(repository.author + repository.branch) ||
      repository.branch.trim().startsWith('-')
    )
      return `${repository.name}：作者或分支配置无效。`
  }
  return ''
}

export function useGit() {
  const settings = ref(cloneGitSettings())
  const loading = ref(false)
  const error = ref('')
  let settingsRevision = 0

  function acceptSaved(value: GitSettings) {
    settingsRevision++
    settings.value = cloneGitSettings(value)
    loading.value = false
    error.value = ''
  }

  async function load(): Promise<boolean> {
    const revision = ++settingsRevision
    loading.value = true
    error.value = ''
    settings.value = cloneGitSettings()
    try {
      const value = isTauri() ? await invoke<GitSettings>('load_git_settings') : cloneGitSettings()
      if (revision !== settingsRevision) return false
      settings.value = cloneGitSettings(value)
      return true
    } catch (cause) {
      if (revision === settingsRevision) error.value = `读取 Git 设置失败：${String(cause)}`
      return false
    } finally {
      if (revision === settingsRevision) loading.value = false
    }
  }

  return { settings, loading, error, load, acceptSaved }
}
