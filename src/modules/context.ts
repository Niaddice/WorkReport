import { inject, type ComputedRef, type InjectionKey, type Ref } from 'vue'
import type { Dayjs } from 'dayjs'
import type { Document, FileFormat } from '../lib/workspace'
import type { useJournal } from '../lib/useJournal'
import type { useAi } from '../lib/ai'
import type { useHolidays } from '../lib/holidays'
import type { useExcel } from '../lib/useExcel'
import type { useSummaryOutput } from '../lib/useSummaryOutput'
import type { useJournalPreview } from '../lib/useJournalPreview'

export type JournalStore = ReturnType<typeof useJournal>
export type AiStore = ReturnType<typeof useAi>
export type HolidayStore = ReturnType<typeof useHolidays>
export type ExcelStore = ReturnType<typeof useExcel>
export type SummaryOutputStore = ReturnType<typeof useSummaryOutput>

/** 日历模块暴露给页面编排层的命令，避免模块内部状态泄漏到 App.vue。 */
export interface CalendarHandle {
  clearRange(): void
}

/** 编辑器模块暴露的命令：聚焦输入框。 */
export interface EditorHandle {
  focus(): void
}

/** 模块可以调用的页面级动作；所有状态写入仍由 composable 统一处理。 */
export interface ModuleActions {
  selectDate(date: string): Promise<void>
  /** 目录模块专用：先解除框选再聚焦单日，保证各模块同步到同一篇日报。 */
  selectReport(date: string): Promise<void>
  loadRange(dates: string[]): Promise<void>
  requestSummary(dates: string[]): Promise<void>
  generateSummary(): void
  directSummary(): void
  closeSummary(): void
  exportSummary(format: FileFormat): void
  /** 准备 Excel 预览（同一周日期），供 Excel 周报模块内嵌使用。 */
  prepareExcel(dates: string[]): Promise<void>
  confirmExcel(): Promise<void>
  discardExcelPreview(): void
  openExcelOutputDirectory(): Promise<void>
  requestFormat(format: FileFormat): void
  requestReload(): void
  requestRestore(): void
  openAiSettings(): void
  openExcelSettings(): void
  openHolidaySettings(): void
  syncHolidays(year: number): void
}

/**
 * 模块运行上下文。模块通过 `useModuleContext()` 注入，因此组件本身不依赖
 * App.vue 的模板绑定，新增模块也不需要修改页面编排层。
 */
export interface ModuleContext {
  journal: JournalStore
  ai: AiStore
  holidays: HolidayStore
  excel: ExcelStore
  summaryOutput: SummaryOutputStore
  summarySelectionDates: Ref<string[]>
  excelBlockedReason: ComputedRef<string>
  now: Ref<Dayjs>
  today: ComputedRef<string>
  storageMode: ComputedRef<'files' | 'dual' | 'database'>
  canRestore: ComputedRef<boolean>
  /** 首页整体忙碌状态（保存、AI、Excel、切换工作空间等）。 */
  disabled: ComputedRef<boolean>
  rangeDates: Ref<string[]>
  rangeDocuments: Ref<Document[]>
  rangeLoading: Ref<boolean>
  rangeError: Ref<string>
  editorPreview: Ref<boolean>
  journalPreview: ReturnType<typeof useJournalPreview>
  calendar: Ref<CalendarHandle | null>
  editor: Ref<EditorHandle | null>
  actions: ModuleActions
}

export const MODULE_CONTEXT: InjectionKey<ModuleContext> = Symbol('workreport-module-context')

export function useModuleContext(): ModuleContext {
  const context = inject(MODULE_CONTEXT)
  if (!context) throw new Error('模块组件必须在 WorkReport 首页的模块板中渲染。')
  return context
}
