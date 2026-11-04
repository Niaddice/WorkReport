import {
  PhCalendarDots,
  PhChartLine,
  PhClockCounterClockwise,
  PhFileXls,
  PhFolderOpen,
  PhGitBranch,
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
import GitModule from '../components/modules/GitModule.vue'
import { moduleType, type ModuleDefinition, type ModuleId } from './types'

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
    title: '编辑',
    description: 'Markdown / 纯文本编辑、预览与历史版本。',
    icon: PhNotePencil,
    component: EditorModule,
    titleExtra: EditorModuleSubtitle,
    help: [
      '直接在正文区输入内容，停止输入后会自动保存；底部显示保存状态和字数。',
      '右上角「MD / TXT」切换文件格式；「复制」复制全文。',
      '点击正文右上角的分栏图标，在当前模块内展开左侧编辑、右侧实时预览；两栏双向同步滚动，再次点击可收起预览。',
      'Markdown 工具栏宽度充足时完整显示，宽度不足时从右侧依次收起到「⋯」中，变宽后自动恢复。',
      '按 ⌘S / Ctrl+S 可立即保存。',
      '目录中的文本文件会在此模块查看和编辑，Excel / CSV 以只读表格预览；通过「返回日报」回到当前日期。',
    ],
    defaultEnabled: true,
  },
  {
    id: 'overview',
    title: '数据概览',
    description: '日报统计、连续记录天数与每日热力图。',
    icon: PhChartLine,
    component: OverviewModule,
    help: [
      '统计本月、本年、本周的日报数量与连续记录天数；四张指标卡随模块宽高调整大小和排列。',
      '顶部显示今天是否已经写过日报，方便快速确认状态。',
      '每日热力图随模块宽高展示近一年、三个月、一个月或一周的记录，当前日始终保持可见；颜色越深表示日报字数越多，悬停查看日期和字数。',
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
      '自动保存的历史快照每分钟最多生成一份；手动保存、切换日报或关闭应用时补存最终版本，相同内容与格式不重复记录。',
      '鼠标指向或键盘聚焦某个版本可预览全文，右侧复制图标可复制该版本原文。',
      '右键历史版本可预览、复制内容、查看当日日报或刷新；空白处右键可刷新，也支持 Shift + F10 打开菜单。',
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
      '首次使用需在「设置 → Excel报告」配置模板和提示词；保存目录和文件名可在「设置 → 目录设置」调整。',
    ],
    defaultEnabled: false,
  },
  {
    id: 'directory',
    title: '工作空间目录',
    description: '浏览和管理工作空间内的文件与文件夹。',
    icon: PhFolderOpen,
    component: DirectoryModule,
    help: [
      '点击文件，在「编辑」模块打开。Markdown、TXT、SQL、Java、Python、XML、YAML 等文本可查看和编辑；Excel / CSV 表格只读预览。Word、PPT 等复杂格式会提示不支持。',
      '点击日报文件时，日历、编辑、历史记录、总结和 Excel报告会切换到对应日期。普通文件的编辑不会创建日报。',
      '如果之前框选了多天，点击目录中的单篇会先解除框选，避免总结仍按旧范围统计。',
      '右键文件、文件夹或空白处，可新建文件或文件夹、重命名、删除、复制相对路径及打开所在文件夹。',
      '工具栏的新建文件、新建文件夹按钮按当前所选目录操作；选中文件时在其所在目录新建，点击空白处后在根目录新建。',
      '新节点使用空白名称输入框，Enter 创建、Esc 取消；有效名称失焦后自动创建，空名称或无效名称取消。名称校验实时显示，新文件创建后自动打开。',
      'F2 在原节点重命名，文件默认选中名称主体；Enter 确认、Esc 取消，重复按 F2 可切换名称、全名和扩展名的选择范围。',
      'Delete 在目录模块内展开删除确认；回收站也在模块内展开，可直接恢复文件，相关操作不弹出窗口。',
      '重命名保留日报日期关联；调整功能保存目录的名称时同步更新设置。目录在操作后自动刷新。',
    ],
    defaultEnabled: false,
  },
  {
    id: 'git',
    title: 'Git',
    description: '多仓库 Git 工作台，管理更改、分支与同步，跨仓库多选提交并获取信息。',
    icon: PhGitBranch,
    component: GitModule,
    help: [
      '默认打开多仓库 Git 工作台。在仓库列表或窄屏下拉框中切换项目，点击「添加或管理仓库」打开 Git 配置。',
      '点击「提交说明」可收起或展开提交区，较矮面板默认收起。窄面板中选择文件后展开 Diff，点击「返回文件列表」返回，提交草稿会保留。',
      '「提交历史」可勾选多条提交，支持全选当前列表、⌘ / Ctrl 单击切换选择、Shift 单击连续选择；可跨仓库选择，搜索和翻页会保留已选项。',
      '点击「获取提交信息」按仓库分组查看完整哈希、作者、时间、引用、父提交和原始说明；可复制完整信息，或编辑工作记录后追加到日历所选单日的日报。最多同时选择 500 条提交。',
      '「本地更改」查看工作区和暂存区 Diff，暂存或取消暂存文件后填写说明，只提交暂存区内容；切换仓库会保留各仓库的提交说明草稿。',
      '提交说明旁的 AI 图标可根据已暂存更改生成说明，沿用默认 AI；在「设置 → Git 配置」自定义提示词，生成后可修改再提交。',
      '工作台支持搜索和分页浏览历史、查看提交 Diff、创建或切换分支、快进合并，以及保存和恢复 Stash。',
      'Fetch 更新远程引用，Pull 仅允许快进，Push 使用当前分支的上游；丢弃修改、删除分支和删除 Stash 需要确认。',
      'Fetch / Pull / Push 遇到 HTTP / HTTPS 认证错误时，在工作台填写用户名和密码或访问令牌后重试。可勾选加密保存到本机，后续优先自动尝试、失效后再提示认证，也可在 Git 设置中清除；不勾选则仅用于本次操作。提交后推送失败会保留本地提交。',
      '右键点击工作台中的文件、提交、分支或 Stash，可打开对应的操作菜单；也可聚焦项目后按 Shift + F10。',
    ],
    defaultEnabled: false,
  },
]

const registry = new Map(MODULE_REGISTRY.map((definition) => [definition.id, definition]))

export function moduleById(id: ModuleId): ModuleDefinition {
  const definition = registry.get(moduleType(id))
  if (!definition) throw new Error(`未知模块：${id}`)
  return definition
}
