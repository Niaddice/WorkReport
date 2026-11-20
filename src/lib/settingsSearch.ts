import type { GlobalSettingsSection } from './settingsSections'
import type { ModuleType } from '../modules/types'

export type SettingSearchTarget =
  | {
      type: 'global'
      section: GlobalSettingsSection
      targetId?: string
    }
  | {
      type: 'module'
      module: ModuleType
      sectionId?: string
    }

export interface SettingSearchEntry {
  id: string
  title: string
  category: string
  desc: string
  keywords: string[]
  target: SettingSearchTarget
}

export interface SettingSearchResult {
  entry: SettingSearchEntry
  score: number
  matchCount: number
}

export const SETTING_SEARCH_ENTRIES: readonly SettingSearchEntry[] = [
  // 全局设置
  {
    id: 'global-workspace',
    title: '工作空间',
    category: '全局设置 · 基础设置',
    desc: '工作空间目录保存位置与路径管理',
    keywords: ['工作空间', '目录', '文件夹', '保存路径', 'workspace', 'folder', 'path'],
    target: { type: 'global', section: 'workspace' },
  },
  {
    id: 'global-appearance',
    title: '外观与字体',
    category: '全局设置 · 基础设置',
    desc: '界面字号缩放、功能栏停顿与长按关闭时间',
    keywords: [
      '外观',
      '字体',
      '字号',
      '缩放',
      '功能栏',
      '长按关闭',
      '侧边栏',
      '主题',
      '皮肤',
      'appearance',
      'font',
      'scale',
      'rail',
    ],
    target: { type: 'global', section: 'appearance' },
  },
  {
    id: 'global-database',
    title: '数据存储',
    category: '全局设置 · 基础设置',
    desc: '工作空间备份迁移、恢复与 SQLite 数据库存储模式',
    keywords: ['数据存储', '数据库', '备份', '恢复', '迁移', 'sqlite', 'database', 'backup', 'restore', 'migration'],
    target: { type: 'global', section: 'database' },
  },
  {
    id: 'global-proxy',
    title: '网络代理',
    category: '全局设置 · 基础设置',
    desc: 'HTTP 与 SOCKS5 网络代理服务器及端口',
    keywords: ['网络代理', '代理', 'proxy', 'http', 'socks5', '网络', '端口', '服务器', 'network'],
    target: { type: 'global', section: 'proxy' },
  },
  {
    id: 'global-updates',
    title: '应用更新',
    category: '全局设置 · 基础设置',
    desc: '查看当前版本、检查与下载最新应用更新',
    keywords: ['应用更新', '版本', '更新', '升级', '检查更新', 'update', 'version', 'upgrade'],
    target: { type: 'global', section: 'updates' },
  },
  {
    id: 'global-ai',
    title: '模型配置',
    category: '全局设置 · AI 助手',
    desc: '管理 AI 模型、API Key、Base URL 与默认 AI 提供商',
    keywords: [
      '模型配置',
      'AI助手',
      'AI',
      '大模型',
      'OpenAI',
      'DeepSeek',
      'Gemini',
      'Anthropic',
      'Claude',
      'Ollama',
      'LM Studio',
      'Cursor',
      'Grok',
      'API Key',
      'Token',
      'Base URL',
      '默认AI',
    ],
    target: { type: 'global', section: 'ai' },
  },

  // 模块设置 - 日报编辑
  {
    id: 'module-editor-format',
    title: '默认格式',
    category: '模块设置 · 日报编辑',
    desc: 'Markdown 与纯文本文件格式设置',
    keywords: ['编辑器', '格式', 'markdown', '纯文本', '默认格式', 'md', 'txt'],
    target: { type: 'module', module: 'editor', sectionId: 'editor-format' },
  },
  {
    id: 'module-editor-files',
    title: '保存位置',
    category: '模块设置 · 日报编辑',
    desc: '日报保存目录与文件命名规则',
    keywords: ['编辑器', '保存位置', '保存目录', '命名规则', '文件名', '日报路径'],
    target: { type: 'module', module: 'editor', sectionId: 'editor-files' },
  },
  {
    id: 'module-editor-skeleton',
    title: '每日骨架',
    category: '模块设置 · 日报编辑',
    desc: '空白日报初始模板与骨架结构',
    keywords: ['编辑器', '每日骨架', '骨架', '模板', '空白日报', 'template', 'skeleton'],
    target: { type: 'module', module: 'editor', sectionId: 'editor-skeleton' },
  },
  {
    id: 'module-editor-prompt',
    title: '日报润色',
    category: '模块设置 · 日报编辑',
    desc: 'AI 润色提示词及模型配置',
    keywords: ['编辑器', '日报润色', '润色', '提示词', 'AI润色', 'polish', 'prompt'],
    target: { type: 'module', module: 'editor', sectionId: 'editor-prompt' },
  },

  // 模块设置 - Git
  {
    id: 'module-git-repositories',
    title: '本地仓库',
    category: '模块设置 · Git 工作台',
    desc: '添加与管理关联的代码仓库',
    keywords: ['git', '代码仓库', '本地仓库', '项目仓库', 'repo', 'repository', '仓库'],
    target: { type: 'module', module: 'git', sectionId: 'git-repositories' },
  },
  {
    id: 'module-git-prompt',
    title: '提交说明',
    category: '模块设置 · Git 工作台',
    desc: 'AI 提交说明提示词与风格预设',
    keywords: ['git', '提交说明', 'commit', '提交信息', '风格预设', '提示词', 'prompt'],
    target: { type: 'module', module: 'git', sectionId: 'git-prompt' },
  },
  {
    id: 'module-git-credentials',
    title: '认证凭据',
    category: '模块设置 · Git 工作台',
    desc: 'GitHub 账号与本机 Git 凭据',
    keywords: ['git', '认证凭据', 'github', 'token', '凭据', '账号', 'credentials', 'auth'],
    target: { type: 'module', module: 'git', sectionId: 'git-credentials' },
  },
  {
    id: 'module-git-advanced',
    title: '高级设置',
    category: '模块设置 · Git 工作台',
    desc: 'Git 可执行程序执行路径',
    keywords: ['git', '高级设置', 'git路径', '可执行文件', 'path', 'binary'],
    target: { type: 'module', module: 'git', sectionId: 'git-advanced' },
  },

  // 模块设置 - Excel
  {
    id: 'module-excel-export',
    title: 'Excel 输出',
    category: '模块设置 · Excel 导出',
    desc: 'Excel 生成配置、表格模板与提示词',
    keywords: ['excel', '表格', '导出', '模板', '提示词', 'xlsx', 'excel输出'],
    target: { type: 'module', module: 'excel', sectionId: 'excel-export' },
  },
  {
    id: 'module-excel-files',
    title: '保存位置',
    category: '模块设置 · Excel 导出',
    desc: 'Excel 导出保存目录与命名规则',
    keywords: ['excel', '导出目录', '保存位置', '命名规则', '路径', '文件名'],
    target: { type: 'module', module: 'excel', sectionId: 'excel-files' },
  },

  // 模块设置 - Summary
  {
    id: 'module-summary-ai',
    title: '默认 AI',
    category: '模块设置 · 工作总结',
    desc: '工作总结使用的全局模型与服务状态',
    keywords: ['工作总结', '总结', 'ai', '默认ai', '模型', '服务状态'],
    target: { type: 'module', module: 'summary', sectionId: 'summary-ai' },
  },
  {
    id: 'module-summary-prompt',
    title: '总结提示词',
    category: '模块设置 · 工作总结',
    desc: '多方案工作总结提示词与系统设定',
    keywords: ['工作总结', '总结', '提示词', '系统设定', '方案', 'prompt'],
    target: { type: 'module', module: 'summary', sectionId: 'summary-prompt' },
  },
  {
    id: 'module-summary-files',
    title: '保存位置',
    category: '模块设置 · 工作总结',
    desc: '工作总结保存目录与命名规则',
    keywords: ['工作总结', '保存目录', '保存位置', '命名规则', '路径'],
    target: { type: 'module', module: 'summary', sectionId: 'summary-files' },
  },

  // 模块设置 - Calendar
  {
    id: 'module-calendar-sync',
    title: '放假与调休',
    category: '模块设置 · 日历视图',
    desc: '法定节假日与调休安排同步与管理',
    keywords: ['日历', '节假日', '放假', '调休', '法定节假日', 'holiday', 'calendar'],
    target: { type: 'module', module: 'calendar', sectionId: 'calendar-sync' },
  },

  // 模块设置 - History
  {
    id: 'module-history-limit',
    title: '存储上限',
    category: '模块设置 · 历史记录',
    desc: '历史版本存储空间上限与清理',
    keywords: ['历史记录', '历史版本', '存储上限', '版本限制', 'history', 'limit'],
    target: { type: 'module', module: 'history', sectionId: 'history-limit' },
  },
]

export function searchSettings(rawQuery: string): SettingSearchResult[] {
  const terms = rawQuery.trim().toLowerCase().split(/\s+/).filter(Boolean)

  if (!terms.length) {
    return SETTING_SEARCH_ENTRIES.map((entry) => ({
      entry,
      score: 0,
      matchCount: 0,
    }))
  }

  const results: SettingSearchResult[] = []

  for (const entry of SETTING_SEARCH_ENTRIES) {
    const titleLower = entry.title.toLowerCase()
    const categoryLower = entry.category.toLowerCase()
    const descLower = entry.desc.toLowerCase()
    const keywordsLower = entry.keywords.map((k) => k.toLowerCase())

    let allTermsMatch = true
    let totalScore = 0
    let totalMatches = 0

    for (const term of terms) {
      let termMatched = false

      if (titleLower === term) {
        totalScore += 120
        totalMatches += 1
        termMatched = true
      } else if (titleLower.includes(term)) {
        totalScore += 80
        totalMatches += 1
        termMatched = true
      }

      if (categoryLower.includes(term)) {
        totalScore += 35
        totalMatches += 1
        termMatched = true
      }

      if (keywordsLower.some((kw) => kw === term)) {
        totalScore += 60
        totalMatches += 1
        termMatched = true
      } else if (keywordsLower.some((kw) => kw.includes(term))) {
        totalScore += 40
        totalMatches += 1
        termMatched = true
      }

      if (descLower.includes(term)) {
        totalScore += 25
        totalMatches += 1
        termMatched = true
      }

      if (!termMatched) {
        allTermsMatch = false
        break
      }
    }

    if (allTermsMatch && totalMatches > 0) {
      results.push({
        entry,
        score: totalScore,
        matchCount: totalMatches,
      })
    }
  }

  return results.sort((a, b) => b.score - a.score)
}
