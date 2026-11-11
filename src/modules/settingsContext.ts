import { inject, type ComputedRef, type InjectionKey, type Ref } from 'vue'
import type { Workspace } from '../lib/workspace'
import type { WorkspaceSettings } from '../lib/settingsDraft'
import type { HolidayState } from '../lib/holidays'
import type { GlobalSettingsSection } from '../lib/settingsSections'

/**
 * 模块设置内容组件的注入契约。
 *
 * 内容组件只声明「这个模块有哪些设置、如何校验」，不关心弹窗外壳、保存按钮和
 * 未保存确认；草稿、工作空间与跨模块动作统一由 ModuleSettingsDialog 提供。
 */
export interface ModuleSettingsContext {
  /** 本次打开弹窗时复制的工作空间设置草稿；组件直接编辑对应字段。 */
  draft: Ref<WorkspaceSettings>
  /** 当前工作空间；未选择工作空间时为空。 */
  workspace: ComputedRef<Workspace | null>
  /** 弹窗整体忙碌状态：读取设置、保存设置、数据库操作等。 */
  busy: ComputedRef<boolean>
  /** 默认 AI 是否已完整配置，供 Excel 等模块判断前置条件。 */
  aiReady: ComputedRef<boolean>
  /** 放假信息不在工作空间设置草稿中，需要单独的状态与同步动作。 */
  holidays: {
    state: ComputedRef<HolidayState>
    currentYear: ComputedRef<number>
    loading: ComputedRef<boolean>
    syncing: ComputedRef<boolean>
    error: ComputedRef<string>
  }
  /** 打开全局设置并跳到指定分类；弹窗会先处理未保存的修改。 */
  openGlobalSettings(section: GlobalSettingsSection): void
  /** 同步指定年份的放假与调休安排，立即写入日历。 */
  syncHolidays(year: number): void
}

export const MODULE_SETTINGS_CONTEXT: InjectionKey<ModuleSettingsContext> = Symbol('workreport-module-settings')

export function useModuleSettingsContext(): ModuleSettingsContext {
  const context = inject(MODULE_SETTINGS_CONTEXT)
  if (!context) throw new Error('模块设置组件必须在模块设置弹窗中渲染。')
  return context
}
