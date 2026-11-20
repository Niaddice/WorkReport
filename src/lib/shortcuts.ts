export type HistoryAction = 'undo' | 'redo'

export type HistoryKeyEvent = Pick<KeyboardEvent, 'key' | 'altKey' | 'ctrlKey' | 'metaKey' | 'shiftKey'>

export function isMacPlatform(): boolean {
  if (typeof navigator === 'undefined') return false
  return /Mac|iPhone|iPad|iPod/i.test(navigator.platform || navigator.userAgent)
}

export function historyShortcut(event: HistoryKeyEvent, isMac: boolean): HistoryAction | null {
  if (event.altKey) return null
  const modifier = isMac ? event.metaKey : event.ctrlKey
  if (!modifier) return null
  const key = event.key.toLowerCase()
  if (key === 'z') return event.shiftKey ? 'redo' : 'undo'
  if (!isMac && key === 'y') return 'redo'
  return null
}

export function searchShortcut(event: HistoryKeyEvent, isMac: boolean): boolean {
  if (event.altKey || event.shiftKey) return false
  const modifier = isMac ? event.metaKey : event.ctrlKey
  if (!modifier) return false
  return event.key.toLowerCase() === 'k'
}

export function saveShortcut(event: HistoryKeyEvent, isMac: boolean): boolean {
  if (event.altKey || event.shiftKey) return false
  const modifier = isMac ? event.metaKey : event.ctrlKey
  if (!modifier) return false
  return event.key.toLowerCase() === 's'
}

export function cheatsheetShortcut(event: HistoryKeyEvent, isMac: boolean): boolean {
  if (event.altKey) return false
  const modifier = isMac ? event.metaKey : event.ctrlKey
  if (!modifier) return false
  return event.key === '/' || event.key === '?'
}

export interface ShortcutItem {
  id: string
  category: 'global' | 'editor' | 'ai' | 'git'
  categoryLabel: string
  title: string
  description: string
  keys: string[]
  macKeys?: string[]
}

export const SHORTCUT_DEFINITIONS: ShortcutItem[] = [
  {
    id: 'search',
    category: 'global',
    categoryLabel: '通用全局',
    title: '全局全文检索',
    description: '快速检索历史日报、工作空间文档与 Git 提交记录',
    keys: ['Ctrl', 'K'],
    macKeys: ['⌘', 'K'],
  },
  {
    id: 'cheatsheet',
    category: 'global',
    categoryLabel: '通用全局',
    title: '快捷键速查表',
    description: '打开/关闭当前快捷键与操作指南卡片',
    keys: ['Ctrl', '/'],
    macKeys: ['⌘', '/'],
  },
  {
    id: 'escape',
    category: 'global',
    categoryLabel: '通用全局',
    title: '关闭弹窗 / 取消',
    description: '关闭当前活动弹窗、差异对比或气泡菜单',
    keys: ['Esc'],
  },
  {
    id: 'save',
    category: 'editor',
    categoryLabel: '日报编辑',
    title: '立即保存草稿',
    description: '强制刷新并将当前编辑内容立即保存到磁盘或数据库',
    keys: ['Ctrl', 'S'],
    macKeys: ['⌘', 'S'],
  },
  {
    id: 'undo',
    category: 'editor',
    categoryLabel: '日报编辑',
    title: '撤销上一步',
    description: '撤销最近一次文本编辑或格式修改',
    keys: ['Ctrl', 'Z'],
    macKeys: ['⌘', 'Z'],
  },
  {
    id: 'redo',
    category: 'editor',
    categoryLabel: '日报编辑',
    title: '重做已撤销操作',
    description: '恢复上一次被撤销的编辑动作',
    keys: ['Ctrl', 'Y'],
    macKeys: ['⌘', '⇧', 'Z'],
  },
  {
    id: 'bold',
    category: 'editor',
    categoryLabel: '日报编辑',
    title: '粗体',
    description: '将当前选中文本切换为粗体标记',
    keys: ['Ctrl', 'B'],
    macKeys: ['⌘', 'B'],
  },
  {
    id: 'italic',
    category: 'editor',
    categoryLabel: '日报编辑',
    title: '斜体',
    description: '将当前选中文本切换为斜体标记',
    keys: ['Ctrl', 'I'],
    macKeys: ['⌘', 'I'],
  },
  {
    id: 'link',
    category: 'editor',
    categoryLabel: '日报编辑',
    title: '插入超链接',
    description: '在富文本或 Markdown 中为选中文本创建链接',
    keys: ['Ctrl', 'K'],
    macKeys: ['⌘', 'K'],
  },
  {
    id: 'indent',
    category: 'editor',
    categoryLabel: '日报编辑',
    title: '列表缩进与减少缩进',
    description: '在项目列表或待办项中调整嵌套层级',
    keys: ['Tab', '/', 'Shift+Tab'],
  },
  {
    id: 'ai-accept',
    category: 'ai',
    categoryLabel: 'AI 智能辅助',
    title: '采纳润色效果',
    description: '在润色效果对比弹窗中采纳改动并替换编辑区内容',
    keys: ['Enter'],
    macKeys: ['↩ Enter'],
  },
  {
    id: 'ai-discard',
    category: 'ai',
    categoryLabel: 'AI 智能辅助',
    title: '放弃润色效果',
    description: '关闭对比弹窗，丢弃 AI 改动并保持原有内容',
    keys: ['Esc'],
  },
  {
    id: 'git-fetch',
    category: 'git',
    categoryLabel: 'Git 工作台',
    title: '多仓库一键 Fetch',
    description: '同步并检查所有关联 Git 仓库的远程最新分支状态',
    keys: ['Git 工作台', '一键 Fetch'],
  },
  {
    id: 'git-discard-hunk',
    category: 'git',
    categoryLabel: 'Git 工作台',
    title: '放弃单块改动',
    description: '在未暂存差异对比中，一键精准放弃指定代码块的改动',
    keys: ['Diff 视图', '放弃改动'],
  },
]
