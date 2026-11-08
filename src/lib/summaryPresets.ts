import { DEFAULT_SUMMARY_PROMPT } from './ai'

export interface SummaryPreset {
  id: string
  label: string
  badge?: string
  description: string
  prompt: string
}

export const SUMMARY_PRESETS: readonly SummaryPreset[] = [
  {
    id: 'default',
    label: '综合归纳',
    description: '按项目/主题归纳成果、进展、问题与计划',
    prompt: DEFAULT_SUMMARY_PROMPT,
  },
  {
    id: 'weekly',
    label: '项目周报',
    badge: '周报',
    description: '提炼本周交付产出、攻坚进展与下周工作排期',
    prompt:
      '请根据所选日期范围内的日报内容，生成一份结构清晰、条理分明的项目周报：\n1. 本周核心进展与已交付成果（按项目/模块梳理）；\n2. 攻坚克难与重难点问题（包含当前状态与对策）；\n3. 下周工作计划与预期里程碑；\n4. 需要协调协助或存在风险的事项。\n要求事实准确、语言精炼，使用标准 Markdown 格式排版。',
  },
  {
    id: 'monthly',
    label: '月度复盘',
    badge: '月报',
    description: '总结月度里程碑、量化成果与反思改进',
    prompt:
      '请根据所选日期范围内的日报内容，生成一份月度工作复盘总结：\n1. 本月核心目标达成情况与重点里程碑；\n2. 关键产出与量化成果（包括功能上线、稳定性优化、技术改进等）；\n3. 过程反思与经验沉淀（遇到哪些阻塞，如何解决与防范）；\n4. 下月核心规划与预期目标。\n重点突出阶段性变化与价值产出，使用清晰的 Markdown 层次结构。',
  },
  {
    id: 'standup',
    label: '站会简报',
    badge: '晨会',
    description: '快速提炼 3 点发言：已完成、下一步、阻塞风险',
    prompt:
      '请根据所选日报内容，提炼一份精炼高效的日常站会/晨会发言要点（控制在 3-5 条关键要点）：\n- 【昨日/近期完成】：关键事项与产出进展；\n- 【今日/下一步计划】：当前正在进行或即将开展的重点；\n- 【阻塞与风险（Blockers）】：是否有需要他人协同或等待确认的事项（如无则标“无阻塞”）。\n尽量言简意赅，适合口头快速汇报。',
  },
  {
    id: 'okr',
    label: '绩效/OKR',
    badge: '绩效',
    description: '梳理关键结果支撑、技术沉淀与业务赋能',
    prompt:
      '请从所选日报中提取关键价值与突出贡献，梳理为适合绩效回顾与 OKR 对齐的成果清单：\n1. 核心关键结果（KR）达成支撑与项目里程碑；\n2. 架构演进、质量保障与技术沉淀（如性能提升、重构、组件抽取）；\n3. 跨团队协同与业务赋能。\n严格忠实于日报记录，量化指标优先展示，格式规范。',
  },
] as const

export function matchSummaryPreset(prompt: string): SummaryPreset | undefined {
  const trimmed = prompt.trim()
  if (!trimmed) return undefined
  return SUMMARY_PRESETS.find((preset) => preset.prompt.trim() === trimmed)
}

export function getSummaryPreset(id: string): SummaryPreset | undefined {
  return SUMMARY_PRESETS.find((preset) => preset.id === id)
}
