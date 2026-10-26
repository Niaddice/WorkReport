<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import dayjs from 'dayjs'
import DOMPurify from 'dompurify'
import { useModuleContext } from '../modules/context'
import { markdownPreviewScrollTop, markdownWithSourceLines, type MarkdownScrollAnchor } from '../lib/markdownScroll'

const { documents, isRange, sourceKey, label, loading, error, scrollPosition } = useModuleContext().journalPreview
const viewport = ref<HTMLElement>()
const previewDocument = ref<HTMLElement>()
let anchors: MarkdownScrollAnchor[] = []
const resizeObserver = typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(refreshAnchors)
const rendered = computed(() =>
  documents.value.map((document) => ({
    ...document,
    html:
      document.format === 'md'
        ? DOMPurify.sanitize(markdownWithSourceLines(document.content), {
            FORBID_TAGS: ['img', 'style', 'input', 'a'],
            FORBID_ATTR: ['style'],
          })
        : '',
  })),
)
function syncScroll() {
  const element = viewport.value
  if (!element) return
  const position = scrollPosition.value
  element.scrollTop =
    position && position.sourceKey === sourceKey.value
      ? markdownPreviewScrollTop(position, anchors, element.scrollHeight - element.clientHeight)
      : 0
}
function refreshAnchors() {
  const element = previewDocument.value
  anchors = []
  if (element && !isRange.value && documents.value.length === 1 && documents.value[0].format === 'md') {
    const bounds = element.getBoundingClientRect()
    const totalLines = documents.value[0].content.replace(/\r\n?/g, '\n').split('\n').length
    const points = new Map<number, number>([[0, 0]])
    for (const block of Array.from(element.querySelector('.journal-prose')?.children ?? [])) {
      if (!(block instanceof HTMLElement) || !block.classList.contains('markdown-source-block')) continue
      const line = Number(block.dataset.sourceLine)
      const end = Number(block.dataset.sourceEnd)
      if (!Number.isFinite(line) || !Number.isFinite(end) || line < 0 || end <= line || end > totalLines) continue
      const rect = block.getBoundingClientRect()
      points.set(line, Math.max(0, rect.top - bounds.top))
      points.set(end, Math.max(0, rect.bottom - bounds.top))
    }
    points.set(totalLines, Math.max(element.scrollHeight, bounds.height, ...points.values()))
    anchors = Array.from(points, ([line, top]) => ({ line, top })).sort((left, right) => left.line - right.line)
  }
  syncScroll()
}
watch(
  [viewport, previewDocument],
  ([container, body]) => {
    resizeObserver?.disconnect()
    if (container) resizeObserver?.observe(container)
    if (body) resizeObserver?.observe(body)
    refreshAnchors()
  },
  { flush: 'post' },
)
watch([rendered, sourceKey], refreshAnchors, { flush: 'post' })
watch(scrollPosition, syncScroll, { flush: 'post' })
onBeforeUnmount(() => resizeObserver?.disconnect())
</script>

<template>
  <!-- eslint-disable vue/no-v-html -- 日报预览先经 DOMPurify 清洗再渲染 -->
  <section class="markdown-preview-module" aria-label="日报实时预览" :aria-busy="loading">
    <div v-if="documents.length" class="markdown-preview-module-heading">
      <span :title="label">{{ label }}</span
      ><span>实时预览</span>
    </div>
    <p v-if="loading" class="markdown-preview-module-empty">正在载入所选日报…</p>
    <p v-else-if="error" class="markdown-preview-module-empty error-text" role="alert">{{ error }}</p>
    <p v-else-if="!documents.length" class="markdown-preview-module-empty">当前没有可预览的日报。</p>
    <p v-else-if="!isRange && !documents[0].content.trim()" class="markdown-preview-module-empty">
      编辑内容后会在这里实时预览。
    </p>
    <div v-else ref="viewport" class="markdown-preview-module-content" tabindex="0" aria-label="日报预览内容">
      <div ref="previewDocument" class="markdown-preview-module-document">
        <article v-for="document in rendered" :key="document.date" class="journal-preview-entry">
          <header v-if="isRange && documents.length > 1">
            <h3>{{ dayjs(document.date).format('YYYY 年 M 月 D 日 dddd') }}</h3>
            <span>{{ document.filename }}</span>
          </header>
          <div v-if="document.format === 'md'" class="journal-prose" v-html="document.html"></div>
          <pre v-else class="range-text">{{ document.content }}</pre>
        </article>
      </div>
    </div>
  </section>
</template>
