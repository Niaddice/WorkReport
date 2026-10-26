// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest'
import { computed, nextTick, ref, type Ref } from 'vue'
import { mount } from '@vue/test-utils'
import JournalPreviewContent from '../src/components/JournalPreviewContent.vue'
import { MODULE_CONTEXT } from '../src/modules/context'
import type { MarkdownScrollPosition } from '../src/lib/markdownScroll'

function previewState(
  document: Ref<{ date?: string; filename: string; format: string; content?: string }>,
  draft: Ref<string>,
  scrollPosition = ref<MarkdownScrollPosition | null>(null),
) {
  return {
    documents: computed(() => [{ ...document.value, content: draft.value }]),
    isRange: ref(false),
    sourceKey: computed(() => document.value.date ?? document.value.filename),
    label: computed(() => document.value.filename),
    loading: ref(false),
    error: ref(''),
    scrollPosition,
  }
}

describe('Markdown live preview rendering', () => {
  it('renders the merged editor documents and keeps TXT contents as literal text', () => {
    const documents = ref([
      { date: '20261026', filename: '20261026.txt', format: 'txt', content: '# TXT 标记 <script>alert(1)</script>' },
      { date: '20261025', filename: '20261025.md', format: 'md', content: '**Markdown 内容**' },
    ])
    const wrapper = mount(JournalPreviewContent, {
      global: {
        provide: {
          [MODULE_CONTEXT as symbol]: {
            journalPreview: {
              documents,
              isRange: ref(true),
              sourceKey: ref('range'),
              label: ref('2 天 · 2 篇日报'),
              loading: ref(false),
              error: ref(''),
              scrollPosition: ref(null),
            },
          },
        },
      },
    })
    try {
      expect(wrapper.findAll('.journal-preview-entry > header span').map((entry) => entry.text())).toEqual([
        '20261026.txt',
        '20261025.md',
      ])
      expect(wrapper.get('.range-text').text()).toBe('# TXT 标记 <script>alert(1)</script>')
      expect(wrapper.find('script').exists()).toBe(false)
      expect(wrapper.get('.journal-prose strong').text()).toBe('Markdown 内容')
    } finally {
      wrapper.unmount()
    }
  })

  it('tracks source paragraphs, remeasures after resize, and resets when the date changes', async () => {
    const document = ref({ date: '20261026', filename: '20261026.md', format: 'md' })
    const draft = ref('# 标题\n\n第一段\n\n第二段\n\n第三段')
    const editorScrollPosition = ref<MarkdownScrollPosition | null>(null)
    let resize!: () => void
    let scale = 1
    vi.stubGlobal(
      'ResizeObserver',
      class {
        constructor(callback: () => void) {
          resize = callback
        }
        observe() {}
        disconnect() {}
      },
    )
    const rect = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (
      this: HTMLElement,
    ) {
      const positions: Record<string, [number, number]> = { 0: [0, 100], 2: [300, 200], 4: [650, 150], 6: [900, 200] }
      const [top, height] = this.classList.contains('markdown-source-block')
        ? (positions[this.dataset.sourceLine!] ?? [0, 0])
        : [0, 1100]
      return new DOMRect(0, top * scale, 300, height * scale)
    })
    const wrapper = mount(JournalPreviewContent, {
      global: {
        provide: {
          [MODULE_CONTEXT as symbol]: {
            journalPreview: previewState(document, draft, editorScrollPosition),
          },
        },
      },
    })
    try {
      await nextTick()
      const viewport = wrapper.get<HTMLElement>('.markdown-preview-module-content').element
      Object.defineProperties(viewport, { scrollHeight: { value: 1200 }, clientHeight: { value: 200 } })
      editorScrollPosition.value = { sourceKey: '20261026', line: 4, ratio: 0.2, atBottom: false }
      await nextTick()
      expect(viewport.scrollTop).toBe(650)
      scale = 1.2
      resize()
      expect(viewport.scrollTop).toBe(780)
      editorScrollPosition.value = { ...editorScrollPosition.value, atBottom: true }
      await nextTick()
      expect(viewport.scrollTop).toBe(1000)
      document.value = { ...document.value, date: '20261027', filename: '20261027.md' }
      await nextTick()
      expect(viewport.scrollTop).toBe(0)
    } finally {
      wrapper.unmount()
      rect.mockRestore()
      vi.unstubAllGlobals()
    }
  })

  it('renders Markdown structure safely and follows the draft before it has been saved', async () => {
    const document = ref({ filename: '20261026.md', format: 'md', content: '已保存原文' })
    const draft = ref(
      '# 日报标题\n\n**重点**\n\n- 完成工作\n\n| 项目 | 状态 |\n| --- | --- |\n| 预览 | 完成 |\n\n' +
        '```ts\nconst ready = true\n```\n\n' +
        '<img src="https://example.test/image.png" onerror="alert(1)"><script>alert(1)</script>' +
        '<p onclick="alert(1)" style="color:red">安全文本</p>',
    )
    const wrapper = mount(JournalPreviewContent, {
      global: {
        provide: {
          [MODULE_CONTEXT as symbol]: {
            journalPreview: previewState(document, draft),
          },
        },
      },
    })
    try {
      const preview = wrapper.get('.journal-prose')
      expect(preview.get('h1').text()).toBe('日报标题')
      expect(preview.get('strong').text()).toBe('重点')
      expect(preview.get('li').text()).toBe('完成工作')
      expect(preview.findAll('tbody td').map((cell) => cell.text())).toEqual(['预览', '完成'])
      expect(preview.get('pre code').text()).toContain('const ready = true')
      expect(preview.find('img, script, [onerror], [onclick], [style]').exists()).toBe(false)
      expect(preview.text()).toContain('安全文本')

      draft.value = '## 新的草稿\n\n**实时更新**'
      await nextTick()
      expect(preview.find('h1').exists()).toBe(false)
      expect(preview.get('h2').text()).toBe('新的草稿')
      expect(preview.get('strong').text()).toBe('实时更新')
      expect(document.value.content).toBe('已保存原文')
    } finally {
      wrapper.unmount()
    }
  })
})
