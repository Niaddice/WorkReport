<script setup lang="ts">
import { computed, onBeforeUnmount, ref, useId, watch } from 'vue'
import dayjs from 'dayjs'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import { PhCircleNotch, PhColumns, PhFileText } from '@phosphor-icons/vue'
import type { Document } from '../lib/workspace'
import { useSplitPreviewScroll } from '../lib/useSplitPreviewScroll'

const props = defineProps<{
  dates: string[]
  documents: Document[]
  loading: boolean
  error: string
  preview: boolean
  scrollSourceKey?: string
}>()
const emit = defineEmits<{ 'update:preview': [value: boolean] }>()
type RangeRun = { start: string; end: string }
const sortedDocuments = computed(() => [...props.documents].sort((left, right) => right.date.localeCompare(left.date)))
const viewport = ref<HTMLElement>()
const previewViewport = ref<HTMLElement>()
const previewId = useId()
const scrollSync = useSplitPreviewScroll({
  editor: viewport,
  preview: previewViewport,
  enabled: () => props.preview,
  sourceKey: () => props.scrollSourceKey ?? [...props.dates].sort().join(','),
})
const resizeObserver = typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(scrollSync.refresh)
function setViewport(pane: string, element: unknown) {
  const target = pane === 'editor' ? viewport : previewViewport
  target.value = element instanceof HTMLElement ? element : undefined
}
watch(
  [viewport, previewViewport],
  (elements) => {
    resizeObserver?.disconnect()
    for (const element of elements) {
      if (!element) continue
      resizeObserver?.observe(element)
      if (element.firstElementChild) resizeObserver?.observe(element.firstElementChild)
    }
    scrollSync.refresh()
  },
  { flush: 'post' },
)
watch([() => props.documents, () => props.loading], scrollSync.refresh, { flush: 'post' })
onBeforeUnmount(() => resizeObserver?.disconnect())
const characterCount = computed(() =>
  props.documents.reduce((total, document) => total + document.content.replace(/\s/g, '').length, 0),
)
const rangeRuns = computed(() => {
  const runs: RangeRun[] = []
  for (const date of [...props.dates].sort()) {
    const last = runs[runs.length - 1]
    if (last && dayjs(date).diff(dayjs(last.end), 'day') === 1) last.end = date
    else runs.push({ start: date, end: date })
  }
  return runs
})
const crossesYears = computed(() => new Set(props.dates.map((date) => date.slice(0, 4))).size > 1)
function simpleRun(run: RangeRun) {
  const start = dayjs(run.start)
  const end = dayjs(run.end)
  if (run.start === run.end) return start.format('M 月 D 日')
  if (start.isSame(end, 'year') && start.isSame(end, 'month'))
    return `${start.format('M 月 D 日')} – ${end.format('D 日')}`
  if (start.isSame(end, 'year')) return `${start.format('M 月 D 日')} – ${end.format('M 月 D 日')}`
  return `${start.format('M 月 D 日')} – ${end.format('M 月 D 日')}`
}
function fullRun(run: RangeRun) {
  const start = dayjs(run.start)
  const end = dayjs(run.end)
  if (run.start === run.end) return start.format('YYYY 年 M 月 D 日')
  return `${start.format('YYYY 年 M 月 D 日')} – ${end.format('YYYY 年 M 月 D 日')}`
}
const rangeLabel = computed(() => {
  if (!rangeRuns.value.length) return ''
  const firstYear = dayjs(rangeRuns.value[0].start).year()
  const lastYear = dayjs(rangeRuns.value[rangeRuns.value.length - 1].end).year()
  const yearPrefix = crossesYears.value ? `${firstYear}–${lastYear} · ` : ''
  return `${yearPrefix}${rangeRuns.value.map(simpleRun).join('、')}`
})
const rangeTooltip = computed(() => {
  if (!rangeRuns.value.length) return ''
  const full = rangeRuns.value.map(fullRun).join('、')
  return props.dates.length > 1 ? `${full}（共 ${props.dates.length} 天）` : full
})
function dateLabel(date: string) {
  return dayjs(date).format('YYYY 年 M 月 D 日 dddd')
}
function render(content: string) {
  return DOMPurify.sanitize(marked.parse(content, { async: false }) as string, {
    FORBID_TAGS: ['img', 'style', 'input', 'a'],
    FORBID_ATTR: ['style'],
  })
}
</script>

<template>
  <!-- eslint-disable vue/no-v-html -- Markdown 预览先经 DOMPurify 清洗再渲染 -->
  <aside class="editor-panel range-overview" aria-label="框选日报合并预览" :aria-busy="loading">
    <header class="editor-heading range-heading">
      <h2 :title="rangeTooltip">{{ rangeLabel }}</h2>
      <div class="editor-body-tools">
        <button
          type="button"
          class="editor-tool-button journal-split-toggle"
          :aria-label="preview ? '收起日报预览' : '左右分栏预览'"
          :title="preview ? '收起日报预览' : '左右分栏预览'"
          :aria-pressed="preview"
          :aria-expanded="preview"
          :aria-controls="preview ? previewId : undefined"
          :disabled="loading || !sortedDocuments.length"
          @click="emit('update:preview', !preview)"
        >
          <PhColumns :size="15" aria-hidden="true" />
        </button>
      </div>
    </header>
    <div class="editor-content">
      <div class="range-panes" :class="{ 'is-split-preview': preview }">
        <div
          v-for="pane in preview ? ['editor', 'preview'] : ['editor']"
          :id="pane === 'preview' ? previewId : undefined"
          :key="pane"
          :ref="(element) => setViewport(pane, element)"
          class="range-overview-body"
          :class="{ 'range-preview-pane': pane === 'preview' }"
          role="region"
          :aria-label="pane === 'editor' ? '框选日报原文' : '框选日报预览'"
          tabindex="0"
          @scroll.passive="scrollSync.onScroll"
        >
          <div class="range-content">
            <div v-if="loading" class="range-state"><PhCircleNotch class="spin" :size="17" />正在载入所选日报…</div>
            <div v-else-if="error" class="range-state range-error" role="alert">{{ error }}</div>
            <div v-else-if="!sortedDocuments.length" class="range-state">所选日期中没有可显示的日报。</div>
            <article v-for="document in sortedDocuments" :key="document.date" class="range-entry">
              <header v-if="sortedDocuments.length > 1">
                <h3>{{ dateLabel(document.date) }}</h3>
                <span>{{ document.filename }}</span>
              </header>
              <div
                v-if="pane === 'preview' && document.format === 'md'"
                class="journal-prose"
                v-html="render(document.content)"
              ></div>
              <pre v-else class="range-text">{{ document.content }}</pre>
            </article>
          </div>
        </div>
      </div>
      <footer class="editor-footer">
        <span
          ><PhFileText :size="14" aria-hidden="true" />{{
            loading ? '正在载入所选日报…' : `已选择 ${dates.length} 天 · ${documents.length} 篇日报`
          }}</span
        >
        <span>{{ characterCount }} 字</span>
      </footer>
    </div>
  </aside>
</template>
