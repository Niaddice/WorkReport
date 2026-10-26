import { describe, expect, it } from 'vitest'
import { effectScope, nextTick, ref } from 'vue'
import { useJournalPreview } from '../src/lib/useJournalPreview'
import type { Document } from '../src/lib/workspace'

function document(date: string, format: 'md' | 'txt', content: string): Document {
  return { date, format, content, filename: `${date}.${format}`, revision: '1' }
}
function setup() {
  const scope = effectScope()
  const journal = {
    workspace: ref({ path: '/test', name: 'test' }),
    document: ref<Document | null>(document('20261026', 'txt', '')),
    selectedDate: ref('20261026'),
    draft: ref(''),
  }
  const selection = { dates: ref<string[]>([]), documents: ref<Document[]>([]), loading: ref(false), error: ref('') }
  const preview = scope.run(() => useJournalPreview(journal, selection))!
  return { scope, journal, selection, preview }
}

describe('journal preview state', () => {
  it('blocks single TXT, follows the unsaved draft and closes after changing back to TXT', async () => {
    const { scope, journal, preview } = setup()
    try {
      preview.toggle()
      expect(preview.visible.value).toBe(false)
      journal.document.value = document('20261026', 'md', '已保存')
      journal.draft.value = '**未保存的编辑**'
      preview.toggle()
      expect(preview.visible.value).toBe(true)
      expect(preview.documents.value[0]!.content).toBe('**未保存的编辑**')
      expect(journal.document.value.content).toBe('已保存')
      journal.document.value = document('20261026', 'txt', '纯文本')
      await nextTick()
      expect(preview.available.value).toBe(false)
      expect(preview.visible.value).toBe(false)
    } finally {
      scope.stop()
    }
  })

  it('previews mixed and TXT-only date ranges in editor order and keeps the window during loading', async () => {
    const { scope, selection, preview } = setup()
    try {
      selection.dates.value = ['20261025', '20261026']
      selection.documents.value = [document('20261025', 'txt', '# 纯文本'), document('20261026', 'md', '**Markdown**')]
      expect(preview.available.value).toBe(true)
      expect(preview.documents.value.map((value) => value.date)).toEqual(['20261026', '20261025'])
      preview.toggle()
      selection.loading.value = true
      selection.documents.value = []
      await nextTick()
      expect(preview.available.value).toBe(false)
      expect(preview.visible.value).toBe(true)
      selection.documents.value = [document('20261025', 'txt', '第一天'), document('20261026', 'txt', '第二天')]
      selection.loading.value = false
      expect(preview.available.value).toBe(true)
      selection.dates.value = ['20261025']
      selection.documents.value = [document('20261025', 'txt', '单日 TXT')]
      await nextTick()
      expect(preview.available.value).toBe(false)
      expect(preview.visible.value).toBe(false)
    } finally {
      scope.stop()
    }
  })

  it('clears stale scroll positions and closes when switching workspaces', async () => {
    const { scope, journal, preview } = setup()
    try {
      journal.document.value = document('20261026', 'md', '正文')
      preview.toggle()
      preview.scrollPosition.value = { sourceKey: preview.sourceKey.value, line: 5, ratio: 0.5, atBottom: false }
      journal.selectedDate.value = '20261027'
      expect(preview.scrollPosition.value).toBeNull()
      journal.workspace.value = { path: '/other', name: 'other' }
      await nextTick()
      expect(preview.visible.value).toBe(false)
    } finally {
      scope.stop()
    }
  })
})
