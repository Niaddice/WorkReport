<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useId, watch, type CSSProperties } from 'vue'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import { isTauri } from '@tauri-apps/api/core'
import {
  PhTextH,
  PhTextB,
  PhListBullets,
  PhCheckSquare,
  PhFileText,
  PhCopy,
  PhCheck,
  PhCircleNotch,
  PhDotsThree,
  PhTextItalic,
  PhTextStrikethrough,
  PhListNumbers,
  PhQuotes,
  PhCode,
  PhCodeBlock,
  PhLink,
  PhMinus,
  PhTable,
  PhEye,
  PhPencilSimple,
  PhBrowsers,
} from '@phosphor-icons/vue'
import { api, type Document, type FileFormat } from '../lib/workspace'
import { pushMessage } from '../lib/messages'
import { markdownEdit, type MarkdownAction } from '../lib/markdownEditing'
import { registerContextEditor } from '../lib/contextMenus'
import { selectMenuPosition } from '../lib/select'
import { createTextareaScrollReader, elementScrollPosition, type MarkdownScrollPosition } from '../lib/markdownScroll'

const props = defineProps<{
  date: string
  document: Document | null
  isNew: boolean
  content: string
  dirty: boolean
  saving: boolean
  disabled: boolean
  error: string
  savedAt: Date | null
  canRestore: boolean
  canUndo: boolean
  canRedo: boolean
  preview: boolean
  journalPreviewAvailable?: boolean
  journalPreviewVisible?: boolean
  storageMode?: 'files' | 'dual' | 'database'
}>()
const emit = defineEmits<{
  edit: [content: string, forceHistory?: boolean]
  undo: []
  redo: []
  save: []
  reload: []
  format: [value: FileFormat]
  restore: []
  error: [value: string]
  'update:preview': [value: boolean]
  'scroll-position': [position: MarkdownScrollPosition]
  'toggle-journal-preview': []
}>()
const textarea = ref<HTMLTextAreaElement>()
watch(
  textarea,
  (element, _previous, onCleanup) => {
    if (!element) return
    const unregister = registerContextEditor(element, {
      scope: () => `${props.date}/${props.document?.filename}`,
      canUndo: () => !props.disabled && props.canUndo,
      canRedo: () => !props.disabled && props.canRedo,
      undo: () => {
        if (!props.disabled && props.canUndo) emit('undo')
      },
      redo: () => {
        if (!props.disabled && props.canRedo) emit('redo')
      },
      replace: (text, start, end) => {
        if (props.disabled) return
        emit('edit', props.content.slice(0, start) + text + props.content.slice(end), true)
        void nextTick(() => {
          if (element === textarea.value) element.setSelectionRange(start + text.length, start + text.length)
        })
      },
    })
    onCleanup(unregister)
  },
  { flush: 'post' },
)
const editorBody = ref<HTMLElement>()
const inlinePreview = ref<HTMLElement>()
const scrollReader = createTextareaScrollReader()
const scrollObserver = typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(publishScrollPosition)
const preview = computed({ get: () => props.preview, set: (value: boolean) => emit('update:preview', value) })
const previewMarkup = ref('')
const copied = ref(false)
const moreOpened = ref(false)
const moreTrigger = ref<HTMLButtonElement>()
const moreMenu = ref<HTMLDivElement>()
const moreMenuId = useId()
const moreMenuStyle = ref<CSSProperties>({ position: 'fixed', visibility: 'hidden' })
const moreTools = [
  { action: 'italic', label: '斜体', icon: PhTextItalic },
  { action: 'strike', label: '删除线', icon: PhTextStrikethrough },
  { action: 'orderedList', label: '有序列表', icon: PhListNumbers },
  { action: 'quote', label: '引用', icon: PhQuotes },
  { action: 'inlineCode', label: '行内代码', icon: PhCode },
  { action: 'codeBlock', label: '代码块', icon: PhCodeBlock },
  { action: 'link', label: '链接', icon: PhLink },
  { action: 'divider', label: '分隔线', icon: PhMinus },
  { action: 'table', label: '表格', icon: PhTable },
] satisfies { action: MarkdownAction; label: string; icon: unknown }[]
let rememberedSelection = { start: 0, end: 0 }
let copiedTimer: ReturnType<typeof setTimeout> | undefined
let dateAnimation: Animation | undefined
const newEmpty = computed(() => props.isNew && !props.content.trim())
const hasContent = computed(() => props.content.trim().length > 0)
const rendered = computed(() =>
  DOMPurify.sanitize(marked.parse(props.content, { async: false }) as string, {
    FORBID_TAGS: ['img', 'style', 'input', 'a'],
    FORBID_ATTR: ['style'],
  }),
)
const status = computed(() => {
  if (newEmpty.value) return '编辑后自动保存'
  if (props.saving) return '正在保存…'
  if (props.error && props.dirty) return '保存失败，草稿已保留'
  if (props.dirty) return '等待保存…'
  if (props.storageMode === 'database') return '已保存到数据库'
  if (props.document?.backupWarning) return '文件已保存，数据库备份待重试'
  if (props.storageMode === 'dual') return '已保存到文件与数据库'
  return '已保存到工作空间'
})
watch(
  () => props.content,
  () => {
    copied.value = false
    clearTimeout(copiedTimer)
  },
)
watch(
  () => props.document?.format,
  () => {
    preview.value = false
    closeMore()
  },
)
watch(
  [preview, () => props.content],
  () => {
    if (preview.value) previewMarkup.value = rendered.value
  },
  { immediate: true },
)
watch(
  () => props.date,
  () => {
    if (textarea.value) textarea.value.scrollTop = 0
    if (inlinePreview.value) inlinePreview.value.scrollTop = 0
    rememberedSelection = { start: 0, end: 0 }
    closeMore()
    dateAnimation?.cancel()
    const body = editorBody.value
    if (!body || typeof body.animate !== 'function' || window.matchMedia?.('(prefers-reduced-motion: reduce)').matches)
      return
    dateAnimation = body.animate(
      [
        { opacity: 0.35, transform: 'translateY(4px)' },
        { opacity: 1, transform: 'translateY(0)' },
      ],
      { duration: 160, easing: 'cubic-bezier(0.2, 0.8, 0.2, 1)' },
    )
    // 切换日期或进入框选预览时会取消动画，处理 finished 的 AbortError。
    void dateAnimation.finished.catch(() => {})
  },
  { flush: 'post' },
)
onBeforeUnmount(() => {
  scrollReader.dispose()
  scrollObserver?.disconnect()
  dateAnimation?.cancel()
  clearTimeout(copiedTimer)
  document.removeEventListener('pointerdown', onOutsidePointer)
  document.removeEventListener('focusin', onOutsideFocus)
  window.removeEventListener('resize', closeMore)
  window.removeEventListener('scroll', onScroll, true)
})
onMounted(() => {
  publishScrollPosition()
  document.addEventListener('pointerdown', onOutsidePointer)
  document.addEventListener('focusin', onOutsideFocus)
  window.addEventListener('resize', closeMore)
  window.addEventListener('scroll', onScroll, true)
})
watch(
  [textarea, inlinePreview],
  ([input, rendered]) => {
    scrollObserver?.disconnect()
    if (input) scrollObserver?.observe(input)
    if (rendered) scrollObserver?.observe(rendered)
    publishScrollPosition()
  },
  { flush: 'post' },
)
watch([() => props.content, () => props.date, () => props.document?.format, preview], publishScrollPosition, {
  flush: 'post',
})
function publishScrollPosition() {
  if (props.document?.format !== 'md') return
  const element = preview.value ? inlinePreview.value : textarea.value
  if (!element?.clientHeight) return
  emit(
    'scroll-position',
    preview.value
      ? elementScrollPosition(element, props.date)
      : scrollReader.read(element as HTMLTextAreaElement, props.date),
  )
}
watch(
  () => props.disabled,
  (disabled) => {
    if (disabled) closeMore()
  },
)
function rememberSelection() {
  if (preview.value || !textarea.value) return
  rememberedSelection = { start: textarea.value.selectionStart, end: textarea.value.selectionEnd }
}
async function applyMarkdown(action: MarkdownAction) {
  if (props.disabled || props.document?.format !== 'md') return
  rememberSelection()
  const result = markdownEdit(props.content, rememberedSelection.start, rememberedSelection.end, action)
  preview.value = false
  closeMore()
  emit('edit', result.content, true)
  await nextTick()
  const input = textarea.value
  if (!input) return
  input.focus({ preventScroll: true })
  input.setSelectionRange(result.selectionStart, result.selectionEnd)
  rememberedSelection = { start: result.selectionStart, end: result.selectionEnd }
}
function closeMore() {
  moreOpened.value = false
}
async function openMore(last = false) {
  if (props.disabled || props.document?.format !== 'md') return
  rememberSelection()
  moreMenuStyle.value = { position: 'fixed', visibility: 'hidden' }
  moreOpened.value = true
  await nextTick()
  if (!moreOpened.value || !moreTrigger.value || !moreMenu.value) return
  const anchor = moreTrigger.value.getBoundingClientRect()
  const viewport = { width: document.documentElement.clientWidth, height: window.innerHeight }
  const desiredAnchor = { ...anchor, top: anchor.top, bottom: anchor.bottom, left: anchor.left, width: 220 }
  const width = selectMenuPosition(desiredAnchor, viewport, 0).width
  moreMenu.value.style.width = `${width}px`
  const position = selectMenuPosition(desiredAnchor, viewport, moreMenu.value.scrollHeight)
  moreMenuStyle.value = {
    position: 'fixed',
    visibility: 'visible',
    left: `${position.left}px`,
    top: `${position.top}px`,
    width: `${position.width}px`,
    maxHeight: `${position.maxHeight}px`,
  }
  const options = moreMenu.value.querySelectorAll<HTMLButtonElement>('button')
  options[last ? options.length - 1 : 0]?.focus({ preventScroll: true })
}
function toggleMore() {
  if (moreOpened.value) closeMore()
  else void openMore()
}
function onMoreKeydown(event: KeyboardEvent) {
  if (event.isComposing || event.altKey || event.ctrlKey || event.metaKey) return
  if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    closeMore()
    moreTrigger.value?.focus({ preventScroll: true })
    return
  }
  if (event.key === 'Tab') {
    closeMore()
    moreTrigger.value?.focus({ preventScroll: true })
    return
  }
  const keys = ['ArrowDown', 'ArrowUp', 'Home', 'End']
  if (!keys.includes(event.key)) return
  event.preventDefault()
  if (!moreOpened.value) {
    void openMore(event.key === 'ArrowUp' || event.key === 'End')
    return
  }
  const options = Array.from(moreMenu.value?.querySelectorAll<HTMLButtonElement>('button') ?? [])
  const current = options.indexOf(document.activeElement as HTMLButtonElement)
  const next =
    event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? options.length - 1
        : (current + (event.key === 'ArrowUp' ? -1 : 1) + options.length) % options.length
  options[next]?.focus({ preventScroll: true })
}
function onOutsidePointer(event: PointerEvent) {
  const target = event.target as Node | null
  if (target && !moreTrigger.value?.contains(target) && !moreMenu.value?.contains(target)) closeMore()
}
function onOutsideFocus(event: FocusEvent) {
  const target = event.target as Node | null
  if (target && !moreTrigger.value?.contains(target) && !moreMenu.value?.contains(target)) closeMore()
}
function onScroll(event: Event) {
  if (moreOpened.value && !(event.target instanceof Node && moreMenu.value?.contains(event.target))) closeMore()
}
async function copyDraft() {
  try {
    if (isTauri()) await api.copy(props.content)
    else await navigator.clipboard.writeText(props.content)
    copied.value = true
    pushMessage({ tone: 'success', text: '日报正文已复制到剪贴板' })
    clearTimeout(copiedTimer)
    copiedTimer = setTimeout(() => {
      copied.value = false
    }, 1600)
  } catch {
    emit('error', '复制失败，请重试或选中文本后手动复制。')
  }
}
function focus() {
  preview.value = false
  void nextTick(() => textarea.value?.focus())
}
defineExpose({ focus })
</script>
<template>
  <!-- eslint-disable vue/no-v-html -- Markdown 预览先经 DOMPurify 清洗再渲染 -->
  <aside class="editor-panel" aria-label="日报编辑区" :aria-busy="disabled">
    <div class="editor-state-stage">
      <Transition name="editor-state">
        <div v-if="document" key="document" class="editor-content">
          <div class="editor-toolbar">
            <div v-if="document.format === 'md'" class="format-actions" aria-label="Markdown 格式工具">
              <md-icon-button aria-label="插入标题" title="标题" :disabled="disabled" @click="applyMarkdown('heading')"
                ><PhTextH :size="18" /></md-icon-button
              ><md-icon-button aria-label="加粗" title="加粗" :disabled="disabled" @click="applyMarkdown('bold')"
                ><PhTextB :size="18" /></md-icon-button
              ><md-icon-button
                aria-label="插入列表"
                title="无序列表"
                :disabled="disabled"
                @click="applyMarkdown('bulletList')"
                ><PhListBullets :size="18" /></md-icon-button
              ><md-icon-button
                aria-label="插入待办"
                title="待办"
                :disabled="disabled"
                @click="applyMarkdown('taskList')"
                ><PhCheckSquare :size="18"
              /></md-icon-button>
              <button
                ref="moreTrigger"
                type="button"
                class="markdown-tools-trigger"
                aria-label="更多 Markdown 工具"
                title="更多 Markdown 工具"
                aria-haspopup="menu"
                :aria-expanded="moreOpened"
                :aria-controls="moreOpened ? moreMenuId : undefined"
                :disabled="disabled"
                @click="toggleMore"
                @keydown="onMoreKeydown"
              >
                <PhDotsThree :size="18" aria-hidden="true" />
              </button>
            </div>
            <span v-else class="editor-context-label">纯文本编辑</span>
          </div>
          <div ref="editorBody" class="editor-body">
            <Transition name="editor-mode"
              ><div
                v-show="preview && document.format === 'md'"
                ref="inlinePreview"
                class="markdown-preview journal-prose"
                :inert="!preview || document.format !== 'md'"
                @scroll.passive="publishScrollPosition"
                v-html="previewMarkup"
              ></div
            ></Transition>
            <Transition name="editor-mode">
              <textarea
                v-show="!preview || document.format !== 'md'"
                ref="textarea"
                class="markdown-editor"
                :value="content"
                :disabled="disabled"
                :inert="preview && document.format === 'md'"
                spellcheck="false"
                aria-label="日报文本内容"
                placeholder="写下今天的工作…"
                @input="emit('edit', ($event.target as HTMLTextAreaElement).value)"
                @select="rememberSelection"
                @keyup="rememberSelection"
                @click="rememberSelection"
                @blur="rememberSelection"
                @scroll.passive="publishScrollPosition"
                @keydown.meta.s.prevent="emit('save')"
                @keydown.ctrl.s.prevent="emit('save')"
              ></textarea>
            </Transition>
            <span class="editor-watermark" aria-hidden="true"
              ><PhFileText :size="14" />{{ document?.filename ?? `${date}.md` }}</span
            >
            <div class="editor-body-tools">
              <button
                v-if="document"
                type="button"
                class="editor-tool-button format-toggle"
                :aria-label="`当前格式 ${document.format === 'md' ? 'Markdown' : '纯文本'}，点击切换`"
                :title="`切换为 ${document.format === 'md' ? 'TXT' : 'Markdown'}`"
                :disabled="disabled"
                @click="emit('format', document.format === 'md' ? 'txt' : 'md')"
              >
                {{ document.format === 'md' ? 'MD' : 'TXT' }}
              </button>
              <button
                v-if="document.format === 'md'"
                type="button"
                class="editor-tool-button preview-toggle"
                :aria-label="preview ? '编辑' : '预览'"
                :title="preview ? '编辑' : '预览'"
                :aria-pressed="preview"
                @click="preview = !preview"
              >
                <PhPencilSimple v-if="preview" :size="15" aria-hidden="true" />
                <PhEye v-else :size="15" aria-hidden="true" />
              </button>
              <button
                type="button"
                class="editor-tool-button journal-preview-toggle"
                aria-label="日报预览"
                :title="journalPreviewVisible ? '关闭日报预览' : '日报预览'"
                :aria-pressed="!!journalPreviewVisible"
                :disabled="disabled || (!journalPreviewAvailable && !journalPreviewVisible)"
                @click="emit('toggle-journal-preview')"
              >
                <PhBrowsers :size="15" aria-hidden="true" />
              </button>
              <button
                v-if="hasContent"
                type="button"
                class="editor-tool-button copy-icon"
                :title="copied ? '已复制' : '复制日报'"
                :aria-label="copied ? '已复制' : '复制日报'"
                :disabled="disabled"
                @click="copyDraft"
              >
                <Transition name="button-label" mode="out-in"
                  ><PhCheck v-if="copied" key="copied" :size="14" /><PhCopy v-else key="copy" :size="14"
                /></Transition>
              </button>
            </div>
          </div>
          <Transition name="inline-feedback"
            ><div v-if="error && dirty" class="save-error-actions">
              <button @click="copyDraft">复制草稿</button><button @click="emit('reload')">重新载入</button>
            </div></Transition
          >
          <footer class="editor-footer">
            <span :class="{ 'error-text': error && dirty }" role="status"
              ><PhCircleNotch v-if="saving" class="spin" :size="14" /><PhCheck
                v-else-if="!dirty && !newEmpty"
                :size="14"
              />{{ status }}</span
            ><span>{{ content.replace(/\s/g, '').length }} 字</span>
          </footer>
        </div>
        <div v-else key="empty" class="editor-empty">
          <div class="empty-document"><PhFileText :size="38" weight="light" /></div>
          <h3>正在载入日报…</h3>
          <p>选择一个日期开始记录。</p>
        </div>
      </Transition>
    </div>
  </aside>
  <Teleport to="body">
    <Transition name="select-popover">
      <div
        v-if="moreOpened"
        :id="moreMenuId"
        ref="moreMenu"
        class="markdown-tools-menu"
        role="menu"
        aria-label="更多 Markdown 工具"
        :style="moreMenuStyle"
        @keydown="onMoreKeydown"
      >
        <button
          v-for="tool in moreTools"
          :key="tool.action"
          type="button"
          class="markdown-tool-option"
          role="menuitem"
          :disabled="disabled"
          @click="applyMarkdown(tool.action)"
        >
          <component :is="tool.icon" :size="16" aria-hidden="true" /><span>{{ tool.label }}</span>
        </button>
      </div>
    </Transition>
  </Teleport>
</template>
