<script setup lang="ts">
import { computed } from 'vue'
import dayjs from 'dayjs'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import { PhCircleNotch, PhFileText, PhTextAlignLeft } from '@phosphor-icons/vue'
import type { Document } from '../lib/workspace'

const props = defineProps<{ dates: string[]; documents: Document[]; loading: boolean; error: string; summarizing: boolean; preview: boolean }>()
const emit = defineEmits<{ close: []; summarize: []; 'update:preview': [value: boolean] }>()
type RangeRun = { start: string; end: string }
const sortedDocuments = computed(() => [...props.documents].sort((left, right) => right.date.localeCompare(left.date)))
const characterCount = computed(() => props.documents.reduce((total, document) => total + document.content.replace(/\s/g, '').length, 0))
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
  if (start.isSame(end, 'year') && start.isSame(end, 'month')) return `${start.format('M 月 D 日')} – ${end.format('D 日')}`
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
  if (!rangeRuns.value.length) return '框选日报'
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
function dateLabel(date: string) { return dayjs(date).format('YYYY 年 M 月 D 日 dddd') }
function render(content: string) {
  return DOMPurify.sanitize(marked.parse(content, { async: false }) as string, { FORBID_TAGS: ['img', 'style', 'input', 'a'], FORBID_ATTR: ['style'] })
}
</script>

<template>
  <aside class="editor-panel range-overview" aria-label="框选日报合并预览" :aria-busy="loading">
    <header class="editor-heading">
      <div class="editor-date-heading">
        <div class="editor-eyebrow">框选日报</div>
        <h2 :title="rangeTooltip">{{ rangeLabel }}</h2>
      </div>
      <div class="editor-file-row">
        <span class="file-label"><PhFileText :size="14" aria-hidden="true" />{{ sortedDocuments.length === 1 ? sortedDocuments[0].filename : `${documents.length} 篇日报 · ${dates.length} 天` }}</span>
        <span class="editor-readonly-label">只读</span>
      </div>
    </header>
    <div class="editor-content">
      <div class="editor-text-actions">
        <button class="text-action" @click="emit('close')">取消框选</button>
        <button class="text-action" :disabled="loading || !documents.length || summarizing" title="打开所选日报的总结弹窗" @click="emit('summarize')"><PhTextAlignLeft :size="14" aria-hidden="true" /><span>总结</span></button>
      </div>
      <div class="editor-toolbar range-toolbar">
        <span class="editor-context-label">按日期降序</span>
        <button class="preview-toggle" :aria-pressed="preview" @click="emit('update:preview', !preview)"><Transition name="button-label" mode="out-in"><span :key="String(preview)">{{ preview ? '原文' : '预览' }}</span></Transition></button>
      </div>
    <div class="range-overview-body">
      <Transition name="content-fade" mode="out-in">
      <div :key="loading ? 'loading' : error ? 'error' : 'documents'" class="range-content">
      <div v-if="loading" class="range-state"><PhCircleNotch class="spin" :size="17" />正在载入所选日报…</div>
      <div v-else-if="error" class="range-state range-error" role="alert">{{ error }}</div>
      <div v-else-if="!sortedDocuments.length" class="range-state">所选日期中没有可显示的日报。</div>
      <article v-for="document in sortedDocuments" :key="document.date" class="range-entry">
        <header v-if="sortedDocuments.length > 1"><h3>{{ dateLabel(document.date) }}</h3><span>{{ document.filename }}</span></header>
        <Transition name="inline-fade" mode="out-in">
          <div v-if="preview && document.format === 'md'" class="journal-prose" v-html="render(document.content)"></div>
          <pre v-else class="range-text">{{ document.content }}</pre>
        </Transition>
      </article>
      </div>
      </Transition>
    </div>
      <footer class="editor-footer"><span><PhFileText :size="14" aria-hidden="true" />{{ loading ? '正在载入所选日报…' : `已选择 ${dates.length} 天 · ${documents.length} 篇日报` }}</span><span>{{ characterCount }} 字</span></footer>
    </div>
  </aside>
</template>
