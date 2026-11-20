/**
 * 全局设置页保留的分类。
 *
 * 模块自己的设置（目录、提示词、Excel、Git、放假信息等）不再占用全局设置分类，
 * 改由模块标题栏的设置入口打开各自的模块设置弹窗。
 */
export type GlobalSettingsSection = 'workspace' | 'appearance' | 'database' | 'proxy' | 'updates' | 'ai'

export const GLOBAL_SETTINGS_SECTIONS: readonly GlobalSettingsSection[] = [
  'workspace',
  'appearance',
  'database',
  'proxy',
  'updates',
  'ai',
]
