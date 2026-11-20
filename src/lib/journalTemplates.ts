import dayjs from 'dayjs'
import type { FileFormat } from './workspace'

export type JournalTemplateId = 'none' | 'standard' | 'agile' | 'project' | 'minimal' | 'custom'

export interface JournalTemplatePreset {
  id: JournalTemplateId
  name: string
  description: string
  markdown: string
  text: string
}

export const JOURNAL_TEMPLATE_PRESETS: JournalTemplatePreset[] = [
  {
    id: 'standard',
    name: '标准工作日报',
    description: '产出总结、明日计划与思考复盘，结构清晰，适合绝大多数日常工作场景。',
    markdown: `### 📌 今日工作产出
1. 
2. 

### 🎯 明日工作计划
1. 
2. 

### 💡 思考与问题
- 无
`,
    text: `【今日工作产出】
1、
2、

【明日工作计划】
1、
2、

【思考与问题】
- 无
`,
  },
  {
    id: 'agile',
    name: '敏捷站会三问',
    description: '昨日回顾、今日要务与阻碍风险，聚焦协同，适合敏捷开发与产研团队。',
    markdown: `### ⏪ 昨天完成了什么
1. 

### ⏩ 今天计划做什么
1. 

### ⚠️ 遇到的阻碍与风险
- 无
`,
    text: `【昨天完成了什么】
1、

【今天计划做什么】
1、

【遇到的阻碍与风险】
- 无
`,
  },
  {
    id: 'project',
    name: '项目重点推进',
    description: '核心里程碑、任务清单与技术细节，适合专项攻坚、阶段交付或重要项目。',
    markdown: `### 🚀 核心里程碑进展
- **重点项目**：

### 📋 今日任务清单
- [ ] 
- [ ] 

### 🔍 技术细节与遗留事项
- 
`,
    text: `【核心里程碑进展】
- 重点项目：

【今日任务清单】
[待办] 
[待办] 

【技术细节与遗留事项】
- 
`,
  },
  {
    id: 'minimal',
    name: '极简打卡清单',
    description: '去除冗余修饰，保留待办与备忘，适合简单记录与快速交接。',
    markdown: `### 今日事项
- [ ] 
- [ ] 

### 备注
- 
`,
    text: `【今日事项】
1、
2、

【备注】
- 
`,
  },
]

const WEEKDAY_NAMES = ['星期日', '星期一', '星期二', '星期三', '星期四', '星期五', '星期六']

/**
 * 变量插值：支持将模板中的 `{date}`, `{year}`, `{month}`, `{day}`, `{weekday}` 等替换为对应日期的实际文本。
 */
export function interpolateTemplate(content: string, dateStr?: string): string {
  if (!content) return ''
  let parsed = dateStr ? dayjs(dateStr) : dayjs()
  if (!parsed.isValid() && dateStr && dateStr.length === 8) {
    const formatted = `${dateStr.slice(0, 4)}-${dateStr.slice(4, 6)}-${dateStr.slice(6, 8)}`
    parsed = dayjs(formatted)
  }
  if (!parsed.isValid()) parsed = dayjs()

  const year = parsed.format('YYYY')
  const month = parsed.format('MM')
  const day = parsed.format('DD')
  const dateFormatted = `${year}-${month}-${day}`
  const rawDate = `${year}${month}${day}`
  const weekday = WEEKDAY_NAMES[parsed.day()] ?? ''

  return content
    .replace(/\{date\}/gi, dateFormatted)
    .replace(/\{rawdate\}/gi, rawDate)
    .replace(/\{(?:year|yyyy)\}/gi, year)
    .replace(/\{(?:month|mm)\}/gi, month)
    .replace(/\{(?:day|dd)\}/gi, day)
    .replace(/\{weekday\}/gi, weekday)
}

/**
 * 获取指定预设模板在对应格式和日期下的具体内容。
 */
export function getJournalTemplateContent(
  templateId: string,
  format: FileFormat = 'md',
  customTemplate = '',
  dateStr?: string,
): string {
  if (!templateId || templateId === 'none') return ''
  if (templateId === 'custom') {
    return interpolateTemplate(customTemplate.trim(), dateStr)
  }
  const preset = JOURNAL_TEMPLATE_PRESETS.find((item) => item.id === templateId)
  if (!preset) return ''
  const base = format === 'txt' ? preset.text : preset.markdown
  return interpolateTemplate(base.trim(), dateStr)
}
