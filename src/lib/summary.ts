import dayjs from 'dayjs'
import { marked, type Token, type Tokens } from 'marked'
import 'dayjs/locale/zh-cn'

dayjs.locale('zh-cn')

export interface ReportDocument {
  date: string
  content: string
}

const LIST_PREFIX = /^([-*+]|\d+[.)])\s+/
const DATE_HEADING = /^\d{4}\s*年\s*\d{1,2}\s*月\s*\d{1,2}\s*日(\s*日报)?$/

/** AI 输入：日期带星期，使用 Markdown 标题与分隔线，便于模型区分不同日期。 */
export function aiSummarySource(documents: ReportDocument[]) {
  return documents
    .map((document) => `### ${dayjs(document.date).format('YYYY年M月D日 dddd')}\n\n${document.content.trim()}`)
    .join('\n\n---\n\n')
}

/**
 * 把一天日报拆成若干条目：跳过 Markdown 标题与日期行，
 * 每个列表项独立成条，普通段落的多行合并为同一条。
 */
export function reportItems(content: string): string[][] {
  const items: string[][] = []
  let current: string[] = []
  const flush = () => {
    if (current.length) {
      items.push(current)
      current = []
    }
  }
  for (const raw of content.split('\n')) {
    const line = raw.trim()
    if (!line || line.startsWith('#') || DATE_HEADING.test(line)) {
      flush()
      continue
    }
    const match = line.match(LIST_PREFIX)
    if (match) {
      flush()
      current = [line.slice(match[0].length).trim()]
    } else {
      current.push(line)
    }
  }
  flush()
  return items
}

/** 直接总结输出：纯文本，日期不带星期与 Markdown 标记，条目按 1、2、3 编号，不同日期之间空一行。 */
export function plainTextSummary(documents: ReportDocument[]) {
  return documents
    .map((document) => {
      const items = reportItems(document.content)
      const blocks = items.length ? items : [[document.content.trim()]]
      const lines = [dayjs(document.date).format('YYYY年M月D日'), '']
      blocks.forEach((item, index) => {
        if (!item[0]) return
        lines.push(`${index + 1}. ${item[0]}`)
        for (const extra of item.slice(1)) lines.push(`   ${extra}`)
      })
      return lines.join('\n')
    })
    .join('\n\n')
}

function decodeEntities(text: string): string {
  const names: Record<string, string> = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ' }
  return text.replace(/&(#x[\da-f]+|#\d+|amp|lt|gt|quot|apos|nbsp);/gi, (raw, entity: string) => {
    if (!entity.startsWith('#')) return names[entity.toLowerCase()] ?? raw
    const value = entity[1]?.toLowerCase() === 'x' ? Number.parseInt(entity.slice(2), 16) : Number(entity.slice(1))
    return value > 0 && value <= 0x10ffff && !(value >= 0xd800 && value <= 0xdfff) ? String.fromCodePoint(value) : raw
  })
}

function htmlText(text: string): string {
  return decodeEntities(text.replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1\s*>/gi, '')
    .replace(/<br\s*\/?\s*>/gi, '\n').replace(/<\/(?:p|div|li|h[1-6])\s*>/gi, '\n').replace(/<[^>]*>/g, ''))
}

function inlineText(tokens: Token[]): string {
  return tokens.map(token => {
    if (token.type === 'br') return '\n'
    if (token.type === 'html') return htmlText((token as Tokens.HTML).text)
    if (token.type === 'codespan') return (token as Tokens.Codespan).text
    if (token.type === 'link' || token.type === 'image') {
      const link = token as Tokens.Link | Tokens.Image
      const label = 'tokens' in link && link.tokens ? inlineText(link.tokens) : decodeEntities(link.text)
      return label === link.href || !link.href ? label : `${label} (${link.href})`
    }
    if ('tokens' in token && Array.isArray(token.tokens)) return inlineText(token.tokens)
    return 'text' in token && typeof token.text === 'string' ? decodeEntities(token.text) : ''
  }).join('')
}

function blockText(tokens: Token[]): string {
  return tokens.map(token => {
    switch (token.type) {
      case 'space': case 'hr': case 'def': return ''
      case 'html': return htmlText((token as Tokens.HTML).text).trim()
      case 'code': return (token as Tokens.Code).text
      case 'blockquote': return blockText((token as Tokens.Blockquote).tokens)
      case 'list': {
        const list = token as Tokens.List
        return list.items.map((item, index) => {
          const marker = list.ordered ? `${Number(list.start) + index}. ` : '- '
          const checkbox = item.task ? `[${item.checked ? 'x' : ' '}] ` : ''
          const content = blockText(item.tokens)
          return `${marker}${checkbox}${content.replace(/\n/g, '\n  ')}`
        }).join('\n')
      }
      case 'table': {
        const table = token as Tokens.Table
        return [table.header, ...table.rows].map(row => row.map(cell => inlineText(cell.tokens)).join('\t')).join('\n')
      }
      default:
        if ('tokens' in token && Array.isArray(token.tokens)) return inlineText(token.tokens)
        return 'text' in token && typeof token.text === 'string' ? decodeEntities(token.text) : ''
    }
  }).filter(Boolean).join('\n\n')
}

/** Preserve code and readable links while removing Markdown presentation from AI results. */
export function markdownSummaryText(content: string): string {
  return blockText(marked.lexer(content)).trim()
}

/** Direct summaries are already plain text; their numbering and whitespace stay intact. */
export function summaryExportContent(content: string, format: 'md' | 'txt', mode: 'ai' | 'direct' | ''): string {
  return format === 'txt' && mode !== 'direct' ? markdownSummaryText(content) : content
}
