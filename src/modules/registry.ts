import {
  PhCalendarDots,
  PhChartLine,
  PhClockCounterClockwise,
  PhFileXls,
  PhNotePencil,
  PhSparkle,
} from '@phosphor-icons/vue'
import CalendarModule from '../components/modules/CalendarModule.vue'
import EditorModule from '../components/modules/EditorModule.vue'
import ExcelModule from '../components/modules/ExcelModule.vue'
import HistoryModule from '../components/modules/HistoryModule.vue'
import SummaryModule from '../components/modules/SummaryModule.vue'
import OverviewModule from '../components/modules/OverviewModule.vue'
import type { ModuleDefinition, ModuleId } from './types'

/**
 * 内置模块注册表。新增模块时：
 * 1. 在 types.ts 的 ModuleId 与 MODULE_IDS 中登记 id；
 * 2. 新建独立的模块内容组件（通过 useModuleContext 注入状态）；
 * 3. 在本表追加元数据，即可从右侧功能栏加入模块板、拖放和持久化。
 */
export const MODULE_REGISTRY: readonly ModuleDefinition[] = [
  {
    id: 'calendar',
    title: '日历',
    description: '周 / 月 / 年视图、农历与节假日、框选日报。',
    icon: PhCalendarDots,
    component: CalendarModule,
    defaultEnabled: true,
  },
  {
    id: 'editor',
    title: '日报编辑',
    description: 'Markdown / 纯文本编辑、预览、AI 润色与历史版本。',
    icon: PhNotePencil,
    component: EditorModule,
    defaultEnabled: true,
  },
  {
    id: 'overview',
    title: '数据概览',
    description: '本月、本年与连续记录统计，今日记录状态一目了然。',
    icon: PhChartLine,
    component: OverviewModule,
    defaultEnabled: false,
  },
  {
    id: 'history',
    title: '历史记录',
    description: '查看当前日报的历史版本，并预览任意版本全文。',
    icon: PhClockCounterClockwise,
    component: HistoryModule,
    defaultEnabled: false,
  },
  {
    id: 'summary',
    title: 'AI 总结',
    description: '对当前选中日期或当天日报生成 AI 总结或直接合并总结。',
    icon: PhSparkle,
    component: SummaryModule,
    defaultEnabled: false,
  },
  {
    id: 'excel',
    title: 'Excel 周报',
    description: '把同一周的日报整理进 Excel 模板，预览确认后输出。',
    icon: PhFileXls,
    component: ExcelModule,
    defaultEnabled: false,
  },
]

const registry = new Map(MODULE_REGISTRY.map((definition) => [definition.id, definition]))

export function moduleById(id: ModuleId): ModuleDefinition {
  const definition = registry.get(id)
  if (!definition) throw new Error(`未知模块：${id}`)
  return definition
}
