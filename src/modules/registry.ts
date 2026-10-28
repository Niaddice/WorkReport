import {
  PhCalendarDots,
  PhChartLine,
  PhClockCounterClockwise,
  PhFileXls,
  PhFolderOpen,
  PhNotePencil,
  PhSparkle,
} from '@phosphor-icons/vue'
import CalendarModule from '../components/modules/CalendarModule.vue'
import EditorModule from '../components/modules/EditorModule.vue'
import EditorModuleSubtitle from '../components/modules/EditorModuleSubtitle.vue'
import DirectoryModule from '../components/modules/DirectoryModule.vue'
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
    help: [
      '点击任意日期，切换日报编辑区显示的当天内容。',
      '在日期上按下并拖到另一天，可框选一段连续日期，用于批量查看或生成总结。',
      '顶部可切换周 / 月 / 年视图；日期上的圆点表示当天已有日报。',
    ],
    defaultEnabled: true,
  },
  {
    id: 'editor',
    title: '日报编辑',
    description: 'Markdown / 纯文本编辑、预览与历史版本。',
    icon: PhNotePencil,
    component: EditorModule,
    titleExtra: EditorModuleSubtitle,
    help: [
      '直接在正文区输入内容，停止输入后会自动保存；底部显示保存状态和字数。',
      '右上角「MD / TXT」切换文件格式；「眼睛」图标在编辑与预览之间切换；「复制」复制全文。',
      '右上角「日报预览」开关独立悬浮窗口，可拖动、调整大小、关闭或最大化；单日 TXT 不可开启。',
      'Markdown 模式下，工具栏提供标题、加粗、列表、待办等快捷插入，「⋯」里还有更多格式。',
      '按 ⌘S / Ctrl+S 可立即保存。',
    ],
    defaultEnabled: true,
  },
  {
    id: 'overview',
    title: '数据概览',
    description: '本月、本年与连续记录统计，今日记录状态一目了然。',
    icon: PhChartLine,
    component: OverviewModule,
    help: [
      '统计本月与本年已记录的日报数量、字数与连续记录天数。',
      '顶部显示今天是否已经写过日报，方便快速确认状态。',
      '统计数据随日报保存自动更新，无需手动刷新。',
    ],
    defaultEnabled: false,
  },
  {
    id: 'history',
    title: '历史记录',
    description: '查看当前日报的历史版本，并预览任意版本全文。',
    icon: PhClockCounterClockwise,
    component: HistoryModule,
    help: [
      '每次保存都会为当天日报生成一个历史版本，按日期分组展示。',
      '点击某个版本可查看全文，确认后能把内容恢复到编辑区。',
      '恢复前会先保存当前内容，避免误覆盖。',
    ],
    defaultEnabled: false,
  },
  {
    id: 'summary',
    title: '总结',
    description: '对当前选中日期或当天日报生成 AI 总结或直接合并总结。',
    icon: PhSparkle,
    component: SummaryModule,
    help: [
      '打开模块后会自动合并所选日期的日报，直接显示为可复制的文本。',
      '点右上角「总结」直接打开生成窗口，核对日期、临时修改提示词后确认生成，在同一窗口查看进度与结果。',
      '临时提示词只对本次生成有效，关闭后恢复设置中的内容；可修改提示词后重试。',
      '还可以复制结果，或导出为 Markdown / TXT 到设置的总结目录。',
      '未配置默认 AI 时，点「总结」会引导前往设置。',
    ],
    defaultEnabled: false,
  },
  {
    id: 'excel',
    title: 'Excel报告',
    description: '把所选日期范围的日报整理进 Excel 模板，预览确认后输出。',
    icon: PhFileXls,
    component: ExcelModule,
    help: [
      '读取当前选中的单日或日历框选范围，支持跨周、跨月和跨年。',
      '点「Xls」直接打开生成窗口，核对日期并临时编辑提示词，确认后在同一窗口查看 AI 进度和预览，再确认输出。',
      '窗口中的提示词只对本次生成有效，关闭后恢复设置中的内容。',
      '已导出列表自动显示与日历选择有日期交集的报告，点击文件即可预览。',
      '首次使用需在「设置 → Excel报告」配置模板和提示词；报告固定保存在工作空间的 excel 目录中。',
    ],
    defaultEnabled: false,
  },
  {
    id: 'directory',
    title: '工作空间目录',
    description: '按日期浏览工作空间内的全部日报，点击即可切换。',
    icon: PhFolderOpen,
    component: DirectoryModule,
    help: [
      '列出当前工作空间内的所有日报，最新的排在最上面，并标注当前选中的日期与文件格式。',
      '点击任意一篇，日历、日报编辑、历史记录、总结和 Excel报告都会切换到这一天。',
      '如果之前框选了多天，点击目录中的单篇会先解除框选，避免总结仍按旧范围统计。',
      '目录随日报的新增与切换自动更新，无需手动刷新。',
    ],
    defaultEnabled: false,
  },
]

const registry = new Map(MODULE_REGISTRY.map((definition) => [definition.id, definition]))

export function moduleById(id: ModuleId): ModuleDefinition {
  const definition = registry.get(id)
  if (!definition) throw new Error(`未知模块：${id}`)
  return definition
}
