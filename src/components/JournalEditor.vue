<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useId, watch, type CSSProperties } from 'vue'
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
  PhColumns,
  PhArrowCounterClockwise,
  PhArrowClockwise,
  PhTextIndent,
  PhTextOutdent,
  PhEraser,
  PhCaretDown,
} from '@phosphor-icons/vue'
import { api, type Document, type FileFormat } from '../lib/workspace'
import { pushMessage } from '../lib/messages'
import type {
  HeadingLevel,
  MarkdownTableAction,
  MarkdownToolbarAction,
  MarkdownToolbarState,
} from '../lib/markdownToolbar'
import { registerContextEditor } from '../lib/contextMenus'
import { selectMenuPosition } from '../lib/select'
import { useSplitPreviewScroll } from '../lib/useSplitPreviewScroll'
import { markdownWithSourceLines } from '../lib/markdownScroll'
import RichMarkdownEditor from './RichMarkdownEditor.vue'

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
  scrollSourceKey?: string
  filePath?: string
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
}>()
const textarea = ref<HTMLTextAreaElement>()
const richEditor = ref<InstanceType<typeof RichMarkdownEditor>>()
const editorViewport = ref<HTMLElement>()
const scrollKey = computed(() => props.scrollSourceKey ?? props.date)
watch(
  [textarea, richEditor],
  () => {
    editorViewport.value = richEditor.value?.getScrollElement() ?? textarea.value
  },
  { flush: 'post' },
)
watch(
  richEditor,
  (editor, _previous, onCleanup) => {
    const element = editor?.getEditableElement()
    if (!editor || !element) return
    const replaceSelection = (text: string) => {
      if (!props.disabled) editor.replaceSelection(text)
    }
    const unregister = registerContextEditor(element, {
      scope: () => {
        const selection = editor.getSelection()
        return `${scrollKey.value}/${editor.transactionVersion()}/${selection.from}/${selection.to}`
      },
      canUndo: () => !props.disabled && props.canUndo,
      canRedo: () => !props.disabled && props.canRedo,
      undo: () => {
        if (!props.disabled && props.canUndo) emit('undo')
      },
      redo: () => {
        if (!props.disabled && props.canRedo) emit('redo')
      },
      replace: replaceSelection,
      replaceSelection,
      selectedText: () => editor.getSelectedMarkdown(),
    })
    onCleanup(unregister)
  },
  { flush: 'post' },
)
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
const previewDocument = ref<HTMLElement>()
const scrollSync = useSplitPreviewScroll({
  editor: editorViewport,
  preview: inlinePreview,
  enabled: () => props.preview,
  sourceKey: () => scrollKey.value,
})
const previewId = useId()
const scrollObserver = typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(refreshScrollGeometry)
const preview = computed({ get: () => props.preview, set: (value: boolean) => emit('update:preview', value) })
const previewMarkup = ref('')
const copied = ref(false)
const moreOpened = ref(false)
const moreTrigger = ref<HTMLButtonElement>()
const moreMenu = ref<HTMLDivElement>()
const moreMenuId = useId()
const moreMenuStyle = ref<CSSProperties>({ position: 'fixed', visibility: 'hidden' })
type ToolbarAction = MarkdownToolbarAction | 'undo' | 'redo'
const toolbarState = ref<MarkdownToolbarState>({
  active: {},
  enabled: {},
  headingLevel: null,
  inTable: false,
  tableEnabled: {},
  tableAlignment: null,
})
const actionPopover = ref<'heading' | 'table' | null>(null)
const actionMenu = ref<HTMLElement>()
const actionAnchor = ref<HTMLElement>()
const actionMenuId = useId()
const actionMenuStyle = ref<CSSProperties>({ position: 'fixed', visibility: 'hidden' })
watch(scrollKey, closePopovers)
const tableRows = ref(2)
const tableColumns = ref(2)
const tableError = ref('')
const headingOptions = [
  { level: 0, label: '正文' },
  { level: 1, label: '一级标题' },
  { level: 2, label: '二级标题' },
  { level: 3, label: '三级标题' },
  { level: 4, label: '四级标题' },
  { level: 5, label: '五级标题' },
  { level: 6, label: '六级标题' },
] satisfies { level: HeadingLevel | 0; label: string }[]
const tableGroups = [
  {
    label: '行',
    actions: [
      { action: 'addRowBefore', label: '上方插入行' },
      { action: 'addRowAfter', label: '下方插入行' },
      { action: 'deleteRow', label: '删除当前行' },
    ],
  },
  {
    label: '列',
    actions: [
      { action: 'addColumnBefore', label: '左侧插入列' },
      { action: 'addColumnAfter', label: '右侧插入列' },
      { action: 'deleteColumn', label: '删除当前列' },
    ],
  },
] satisfies { label: string; actions: { action: MarkdownTableAction; label: string }[] }[]
const tableAlignments = [
  { action: 'alignLeft', value: 'left', label: '列左对齐' },
  { action: 'alignCenter', value: 'center', label: '列居中' },
  { action: 'alignRight', value: 'right', label: '列右对齐' },
] satisfies { action: MarkdownTableAction; value: 'left' | 'center' | 'right'; label: string }[]
const markdownTools = [
  { action: 'undo', label: '撤销', icon: PhArrowCounterClockwise },
  { action: 'redo', label: '重做', icon: PhArrowClockwise },
  { action: 'heading', label: '标题与正文', icon: PhTextH },
  { action: 'bold', label: '加粗', icon: PhTextB },
  { action: 'bulletList', label: '插入列表', icon: PhListBullets },
  { action: 'taskList', label: '插入待办', icon: PhCheckSquare },
  { action: 'italic', label: '斜体', icon: PhTextItalic },
  { action: 'strike', label: '删除线', icon: PhTextStrikethrough },
  { action: 'orderedList', label: '有序列表', icon: PhListNumbers },
  { action: 'quote', label: '引用', icon: PhQuotes },
  { action: 'inlineCode', label: '行内代码', icon: PhCode },
  { action: 'codeBlock', label: '代码块', icon: PhCodeBlock },
  { action: 'link', label: '链接', icon: PhLink },
  { action: 'divider', label: '分隔线', icon: PhMinus },
  { action: 'table', label: '表格', icon: PhTable },
  { action: 'indent', label: '增加缩进', icon: PhTextIndent },
  { action: 'outdent', label: '减少缩进', icon: PhTextOutdent },
  { action: 'clearFormat', label: '清除格式', icon: PhEraser },
] satisfies { action: ToolbarAction; label: string; icon: unknown }[]
const formatActions = ref<HTMLElement>()
const visibleToolCount = ref(markdownTools.length)
const visibleTools = computed(() => markdownTools.slice(0, visibleToolCount.value))
const moreTools = computed(() => markdownTools.slice(visibleToolCount.value))
const toolbarObserver = typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(measureToolbar)
let toolbarWidth = 0
watch(
  formatActions,
  (element) => {
    toolbarObserver?.disconnect()
    if (element) {
      toolbarObserver?.observe(element)
      measureToolbar()
    } else {
      closePopovers()
      visibleToolCount.value = markdownTools.length
    }
  },
  { flush: 'post' },
)

function measureToolbar() {
  const element = formatActions.value
  if (!element) return
  const width = element.clientWidth
  const gap = Number.parseFloat(getComputedStyle(element).columnGap) || 0
  const button = element.querySelector<HTMLElement>('md-icon-button')
  // Module layout animations scale the painted rectangle, not the toolbar's
  // layout width. Compare layout measurements so a transient FLIP transform
  // cannot leave tools hidden after the animation finishes.
  const toolWidth = button?.offsetWidth || 26
  const triggerWidth = moreTrigger.value?.offsetWidth || 26
  const fullWidth = markdownTools.length * toolWidth + (markdownTools.length - 1) * gap
  const count =
    width >= fullWidth
      ? markdownTools.length
      : Math.max(0, Math.min(markdownTools.length - 1, Math.floor((width - triggerWidth) / (toolWidth + gap))))
  const resized = Math.abs(width - toolbarWidth) > 0.5
  toolbarWidth = width
  if (count === visibleToolCount.value && !resized) return
  const focused = document.activeElement
  const focusedAction =
    focused instanceof HTMLElement && element.contains(focused)
      ? focused.closest('[data-markdown-action]')?.getAttribute('data-markdown-action')
      : null
  const hiddenFocusedTool = focusedAction && markdownTools.findIndex((tool) => tool.action === focusedAction) >= count
  const focusedMenu =
    focused instanceof Node && (moreMenu.value?.contains(focused) || actionMenu.value?.contains(focused))
  const removedTrigger = focused === moreTrigger.value && count === markdownTools.length
  const previousAnchor = actionAnchor.value
  closePopovers()
  visibleToolCount.value = count
  if (hiddenFocusedTool || focusedMenu || removedTrigger) {
    void nextTick(() => {
      const target =
        previousAnchor?.isConnected &&
        (!previousAnchor.dataset.markdownAction || toolEnabled(previousAnchor.dataset.markdownAction as ToolbarAction))
          ? previousAnchor
          : (moreTrigger.value ?? lastEnabledTool())
      target?.focus({ preventScroll: true })
    })
  }
}
function onWindowResize() {
  const focusedMenu =
    document.activeElement instanceof Node &&
    (moreMenu.value?.contains(document.activeElement) || actionMenu.value?.contains(document.activeElement))
  const previousAnchor = actionAnchor.value
  closePopovers()
  measureToolbar()
  if (focusedMenu)
    void nextTick(() =>
      (previousAnchor?.isConnected ? previousAnchor : (moreTrigger.value ?? lastEnabledTool()))?.focus({
        preventScroll: true,
      }),
    )
}
function lastEnabledTool() {
  const tool = [...visibleTools.value].reverse().find((item) => toolEnabled(item.action))
  return tool ? formatActions.value?.querySelector<HTMLElement>(`[data-markdown-action="${tool.action}"]`) : undefined
}
let copiedTimer: ReturnType<typeof setTimeout> | undefined
let dateAnimation: Animation | undefined
const newEmpty = computed(() => props.isNew && !props.content.trim())
const hasContent = computed(() => props.content.trim().length > 0)
const rendered = computed(() =>
  DOMPurify.sanitize(markdownWithSourceLines(props.content), {
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
    closePopovers()
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
    closePopovers()
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
  scrollObserver?.disconnect()
  toolbarObserver?.disconnect()
  dateAnimation?.cancel()
  clearTimeout(copiedTimer)
  document.removeEventListener('pointerdown', onOutsidePointer)
  document.removeEventListener('focusin', onOutsideFocus)
  window.removeEventListener('resize', onWindowResize)
  window.removeEventListener('scroll', onScroll, true)
})
onMounted(() => {
  refreshScrollGeometry()
  document.addEventListener('pointerdown', onOutsidePointer)
  document.addEventListener('focusin', onOutsideFocus)
  window.addEventListener('resize', onWindowResize)
  window.addEventListener('scroll', onScroll, true)
})
watch(
  [editorViewport, inlinePreview, previewDocument],
  ([input, rendered, body]) => {
    scrollObserver?.disconnect()
    if (input) scrollObserver?.observe(input)
    if (rendered) scrollObserver?.observe(rendered)
    if (body) scrollObserver?.observe(body)
    refreshScrollGeometry()
  },
  { flush: 'post' },
)
watch([previewMarkup, () => props.content], refreshScrollGeometry, { flush: 'post' })
watch(
  [preview, () => props.document?.format],
  () => {
    refreshScrollGeometry()
  },
  { flush: 'post' },
)
function refreshScrollGeometry() {
  scrollSync.refresh()
}
function onInput(event: Event) {
  emit('edit', (event.target as HTMLTextAreaElement).value)
  scrollSync.captureEditor()
}
function onRichEdit(content: string, forceHistory = false) {
  emit('edit', content, forceHistory)
  scrollSync.captureEditor()
}
watch(
  () => props.disabled,
  (disabled) => {
    if (disabled) closePopovers()
  },
)
function toolEnabled(action: ToolbarAction) {
  if (props.disabled) return false
  if (action === 'undo') return props.canUndo
  if (action === 'redo') return props.canRedo
  return toolbarState.value.enabled[action] === true
}
function toolActive(action: ToolbarAction) {
  return action !== 'undo' && action !== 'redo' && toolbarState.value.active[action] === true
}
function isToggleAction(action: ToolbarAction) {
  return [
    'bold',
    'italic',
    'strike',
    'bulletList',
    'taskList',
    'orderedList',
    'quote',
    'inlineCode',
    'codeBlock',
  ].includes(action)
}
function onToolbarState(state: MarkdownToolbarState) {
  toolbarState.value = state
}
function applyMarkdown(action: ToolbarAction) {
  if (!toolEnabled(action) || props.document?.format !== 'md') return
  if (action === 'heading' || action === 'table') {
    void openActionPopover(action)
    return
  }
  closePopovers()
  if (action === 'undo' || action === 'redo') {
    if (action === 'undo') emit('undo')
    else emit('redo')
    focus()
    return
  }
  richEditor.value?.apply(action)
  scrollSync.captureEditor()
}
function closeMore() {
  moreOpened.value = false
}
async function openMore(last = false) {
  if (props.disabled || props.document?.format !== 'md' || !moreTools.value.length) return
  closeActionPopover()
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
  await nextTick()
  if (!moreOpened.value || !moreMenu.value) return
  const options = moreMenu.value.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')
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
  const options = Array.from(moreMenu.value?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])
  if (!options.length) return
  const current = options.indexOf(document.activeElement as HTMLButtonElement)
  const next =
    event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? options.length - 1
        : (current + (event.key === 'ArrowUp' ? -1 : 1) + options.length) % options.length
  options[next]?.focus({ preventScroll: true })
}
function closeActionPopover(restoreFocus = false) {
  actionPopover.value = null
  if (restoreFocus && actionAnchor.value?.isConnected) actionAnchor.value.focus({ preventScroll: true })
}
function closePopovers() {
  closeMore()
  closeActionPopover()
}
async function openActionPopover(kind: 'heading' | 'table', last = false) {
  if (actionPopover.value === kind) return closeActionPopover(true)
  const anchor =
    formatActions.value?.querySelector<HTMLElement>(`[data-markdown-action="${kind}"]`) ?? moreTrigger.value
  if (!anchor) return
  closeMore()
  actionAnchor.value = anchor
  actionPopover.value = kind
  tableError.value = ''
  actionMenuStyle.value = { position: 'fixed', visibility: 'hidden' }
  await nextTick()
  if (actionPopover.value !== kind || !actionMenu.value || !anchor.isConnected) return
  const rect = anchor.getBoundingClientRect()
  const viewport = { width: document.documentElement.clientWidth, height: window.innerHeight }
  const desiredAnchor = {
    ...rect,
    top: rect.top,
    bottom: rect.bottom,
    left: rect.left,
    width: kind === 'table' ? 260 : 230,
  }
  const width = selectMenuPosition(desiredAnchor, viewport, 0).width
  actionMenu.value.style.width = `${width}px`
  const position = selectMenuPosition(desiredAnchor, viewport, actionMenu.value.scrollHeight)
  actionMenuStyle.value = {
    position: 'fixed',
    visibility: 'visible',
    left: `${position.left}px`,
    top: `${position.top}px`,
    width: `${position.width}px`,
    maxHeight: `${position.maxHeight}px`,
  }
  await nextTick()
  if (actionPopover.value !== kind || !actionMenu.value) return
  const choices = actionMenu.value.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled)')
  const selected = kind === 'heading' ? actionMenu.value.querySelector<HTMLElement>('[aria-checked="true"]') : null
  const choice = last ? choices[choices.length - 1] : (selected ?? choices[0])
  choice?.focus({ preventScroll: true })
  if (choice instanceof HTMLInputElement) choice.select()
}
function onToolKeydown(event: KeyboardEvent, action: ToolbarAction) {
  if (
    (action !== 'heading' && action !== 'table') ||
    !toolEnabled(action) ||
    event.isComposing ||
    event.altKey ||
    event.ctrlKey ||
    event.metaKey
  )
    return
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault()
    void openActionPopover(action, event.key === 'ArrowUp')
  }
}
function onActionKeydown(event: KeyboardEvent) {
  if (event.isComposing || event.altKey || event.ctrlKey || event.metaKey) return
  if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    closeActionPopover(true)
    return
  }
  if (actionPopover.value !== 'heading') return
  if (event.key === 'Tab') {
    closeActionPopover(true)
    return
  }
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return
  event.preventDefault()
  const choices = Array.from(actionMenu.value?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])
  if (!choices.length) return
  const current = choices.indexOf(document.activeElement as HTMLButtonElement)
  const next =
    event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? choices.length - 1
        : (current + (event.key === 'ArrowUp' ? -1 : 1) + choices.length) % choices.length
  choices[next]?.focus({ preventScroll: true })
}
function chooseHeading(level: HeadingLevel | 0) {
  if (!toolEnabled('heading')) return
  closePopovers()
  richEditor.value?.setHeading(level)
  scrollSync.captureEditor()
}
function insertTable() {
  if (!toolEnabled('table')) return
  if (
    !Number.isInteger(tableRows.value) ||
    tableRows.value < 1 ||
    tableRows.value > 20 ||
    !Number.isInteger(tableColumns.value) ||
    tableColumns.value < 1 ||
    tableColumns.value > 10
  ) {
    tableError.value = '行数需为 1–20，列数需为 1–10 的整数'
    return
  }
  closePopovers()
  richEditor.value?.insertTable(tableRows.value, tableColumns.value)
  scrollSync.captureEditor()
}
function runTableAction(action: MarkdownTableAction) {
  if (props.disabled || !toolbarState.value.tableEnabled[action]) return
  closePopovers()
  richEditor.value?.tableAction(action)
  scrollSync.captureEditor()
}
function onOutsidePointer(event: PointerEvent) {
  const target = event.target as Node | null
  if (target && !moreTrigger.value?.contains(target) && !moreMenu.value?.contains(target)) closeMore()
  if (target && !actionAnchor.value?.contains(target) && !actionMenu.value?.contains(target)) closeActionPopover()
}
function onOutsideFocus(event: FocusEvent) {
  const target = event.target as Node | null
  if (target && !moreTrigger.value?.contains(target) && !moreMenu.value?.contains(target)) closeMore()
  if (target && !actionAnchor.value?.contains(target) && !actionMenu.value?.contains(target)) closeActionPopover()
}
function onScroll(event: Event) {
  if (moreOpened.value && !(event.target instanceof Node && moreMenu.value?.contains(event.target))) closeMore()
  if (actionPopover.value && !(event.target instanceof Node && actionMenu.value?.contains(event.target)))
    closeActionPopover()
}
async function copyDraft() {
  try {
    if (isTauri()) await api.copy(props.content)
    else await navigator.clipboard.writeText(props.content)
    copied.value = true
    pushMessage({ tone: 'success', text: props.filePath ? '文件内容已复制到剪贴板' : '日报正文已复制到剪贴板' })
    clearTimeout(copiedTimer)
    copiedTimer = setTimeout(() => {
      copied.value = false
    }, 1600)
  } catch {
    emit('error', '复制失败，请重试或选中文本后手动复制。')
  }
}
function focus() {
  void nextTick(() => {
    if (richEditor.value) richEditor.value.focus()
    else textarea.value?.focus()
  })
}
defineExpose({ focus })
</script>
<template>
  <!-- eslint-disable vue/no-v-html -- Markdown 预览先经 DOMPurify 清洗再渲染 -->
  <aside class="editor-panel" :aria-label="filePath ? '文件编辑区' : '日报编辑区'" :aria-busy="disabled">
    <div class="editor-state-stage">
      <Transition name="editor-state">
        <div v-if="document" key="document" class="editor-content">
          <div class="editor-toolbar">
            <div
              v-if="document.format === 'md'"
              ref="formatActions"
              class="format-actions"
              role="toolbar"
              aria-label="Markdown 格式工具"
            >
              <md-icon-button
                v-for="tool in visibleTools"
                :key="tool.action"
                :data-markdown-action="tool.action"
                :aria-label="tool.label"
                :title="tool.label"
                :class="{ 'is-active': toolActive(tool.action) }"
                :toggle="isToggleAction(tool.action)"
                :selected="toolActive(tool.action)"
                :aria-pressed="isToggleAction(tool.action) ? toolActive(tool.action) : undefined"
                :aria-haspopup="tool.action === 'heading' ? 'menu' : tool.action === 'table' ? 'dialog' : undefined"
                :aria-expanded="
                  tool.action === 'heading' || tool.action === 'table' ? actionPopover === tool.action : undefined
                "
                :aria-controls="actionPopover === tool.action ? actionMenuId : undefined"
                :disabled="!toolEnabled(tool.action)"
                @click="applyMarkdown(tool.action)"
                @keydown="onToolKeydown($event, tool.action)"
              >
                <component :is="tool.icon" :size="18" aria-hidden="true" />
                <span
                  v-if="tool.action === 'heading' && toolbarState.headingLevel"
                  class="markdown-heading-level"
                  aria-hidden="true"
                  >{{ toolbarState.headingLevel }}</span
                >
              </md-icon-button>
              <button
                v-if="moreTools.length"
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
          <div ref="editorBody" class="editor-body" :class="{ 'is-split-preview': preview }">
            <Transition name="editor-mode"
              ><div
                v-show="preview"
                :id="previewId"
                ref="inlinePreview"
                class="markdown-preview journal-prose"
                role="region"
                :aria-label="filePath ? '文件实时预览' : '日报实时预览'"
                tabindex="0"
                :inert="!preview"
                @scroll.passive="scrollSync.onScroll"
              >
                <div v-if="document.format === 'md'" ref="previewDocument" v-html="previewMarkup"></div>
                <pre v-else class="range-text">{{ content }}</pre>
              </div></Transition
            >
            <RichMarkdownEditor
              v-if="document.format === 'md'"
              :key="scrollKey"
              ref="richEditor"
              :content="content"
              :disabled="disabled"
              :source-key="scrollKey"
              @edit="onRichEdit"
              @undo="emit('undo')"
              @redo="emit('redo')"
              @save="emit('save')"
              @scroll="scrollSync.onScroll"
              @toolbar-state="onToolbarState"
            />
            <textarea
              v-else
              ref="textarea"
              class="markdown-editor"
              :value="content"
              :disabled="disabled"
              spellcheck="false"
              :aria-label="filePath ? '文件文本内容' : '日报文本内容'"
              :placeholder="filePath ? '编辑文件内容…' : '写下今天的工作…'"
              @input="onInput"
              @scroll.passive="scrollSync.onScroll"
              @keydown.meta.s.prevent="emit('save')"
              @keydown.ctrl.s.prevent="emit('save')"
            ></textarea>
            <span class="editor-watermark" aria-hidden="true"
              ><PhFileText :size="14" />{{ document?.filename ?? `${date}.md` }}</span
            >
            <div class="editor-body-tools">
              <button
                v-if="document && !filePath"
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
                type="button"
                class="editor-tool-button journal-split-toggle"
                :aria-label="preview ? '收起日报预览' : '左右分栏预览'"
                :title="preview ? '收起日报预览' : '左右分栏预览'"
                :aria-pressed="preview"
                :aria-expanded="preview"
                :aria-controls="preview ? previewId : undefined"
                @click="preview = !preview"
              >
                <PhColumns :size="15" aria-hidden="true" />
              </button>
              <button
                v-if="hasContent"
                type="button"
                class="editor-tool-button copy-icon"
                :title="copied ? '已复制' : filePath ? '复制文件内容' : '复制日报'"
                :aria-label="copied ? '已复制' : filePath ? '复制文件内容' : '复制日报'"
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
          :role="isToggleAction(tool.action) ? 'menuitemcheckbox' : 'menuitem'"
          :class="{ 'is-active': toolActive(tool.action) }"
          :aria-checked="isToggleAction(tool.action) ? toolActive(tool.action) : undefined"
          :aria-haspopup="tool.action === 'heading' ? 'menu' : tool.action === 'table' ? 'dialog' : undefined"
          :disabled="!toolEnabled(tool.action)"
          @click="applyMarkdown(tool.action)"
        >
          <component :is="tool.icon" :size="16" aria-hidden="true" /><span>{{ tool.label }}</span>
          <PhCheck
            v-if="isToggleAction(tool.action) && toolActive(tool.action)"
            class="markdown-option-check"
            :size="14"
            aria-hidden="true"
          />
          <PhCaretDown
            v-else-if="tool.action === 'heading' || tool.action === 'table'"
            class="markdown-option-check"
            :size="14"
            aria-hidden="true"
          />
        </button>
      </div>
    </Transition>
    <Transition name="select-popover">
      <div
        v-if="actionPopover"
        :id="actionMenuId"
        ref="actionMenu"
        class="markdown-tools-menu markdown-action-popover"
        :role="actionPopover === 'heading' ? 'menu' : 'dialog'"
        :aria-label="actionPopover === 'heading' ? '标题与正文' : '表格'"
        :style="actionMenuStyle"
        @keydown="onActionKeydown"
      >
        <template v-if="actionPopover === 'heading'">
          <button
            v-for="option in headingOptions"
            :key="option.level"
            type="button"
            class="markdown-tool-option markdown-heading-option"
            role="menuitemradio"
            :aria-label="option.level ? `H${option.level}` : '正文'"
            :aria-checked="toolbarState.headingLevel === option.level"
            :disabled="!toolEnabled('heading')"
            @click="chooseHeading(option.level)"
          >
            <span class="markdown-heading-symbol" aria-hidden="true">{{
              option.level ? `H${option.level}` : '¶'
            }}</span>
            <span>{{ option.label }}</span>
            <PhCheck
              v-if="toolbarState.headingLevel === option.level"
              class="markdown-option-check"
              :size="14"
              aria-hidden="true"
            />
          </button>
        </template>
        <template v-else-if="toolbarState.inTable">
          <div v-for="group in tableGroups" :key="group.label" class="markdown-table-group">
            <span class="markdown-table-label">{{ group.label }}</span>
            <button
              v-for="item in group.actions"
              :key="item.action"
              type="button"
              class="markdown-tool-option"
              :disabled="disabled || !toolbarState.tableEnabled[item.action]"
              @click="runTableAction(item.action)"
            >
              {{ item.label }}
            </button>
          </div>
          <div class="markdown-table-group">
            <span class="markdown-table-label">列对齐</span>
            <div class="markdown-table-alignment">
              <button
                v-for="item in tableAlignments"
                :key="item.action"
                type="button"
                :aria-label="item.label"
                :aria-pressed="toolbarState.tableAlignment === item.value"
                :disabled="disabled || !toolbarState.tableEnabled[item.action]"
                @click="runTableAction(item.action)"
              >
                {{ item.value === 'left' ? '左对齐' : item.value === 'center' ? '居中' : '右对齐' }}
              </button>
            </div>
          </div>
          <button
            type="button"
            class="markdown-tool-option markdown-table-delete"
            :disabled="disabled || !toolbarState.tableEnabled.deleteTable"
            @click="runTableAction('deleteTable')"
          >
            删除表格
          </button>
        </template>
        <form v-else class="markdown-table-form" @submit.prevent="insertTable">
          <p class="markdown-table-title">插入表格</p>
          <div class="markdown-table-dimensions">
            <label
              >行数<input
                v-model.number="tableRows"
                type="number"
                aria-label="行数"
                min="1"
                max="20"
                step="1"
                required
                @input="tableError = ''"
            /></label>
            <label
              >列数<input
                v-model.number="tableColumns"
                type="number"
                aria-label="列数"
                min="1"
                max="10"
                step="1"
                required
                @input="tableError = ''"
            /></label>
          </div>
          <p class="markdown-table-hint">第一行作为表头</p>
          <p v-if="tableError" role="alert" class="error-text">{{ tableError }}</p>
          <button type="submit" class="markdown-table-submit" :disabled="!toolEnabled('table')">插入表格</button>
        </form>
      </div>
    </Transition>
  </Teleport>
</template>
