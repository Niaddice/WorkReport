<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import dayjs from 'dayjs'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import { isTauri } from '@tauri-apps/api/core'
import { PhTextH, PhTextB, PhListBullets, PhCheckSquare, PhFileText, PhCheck, PhCircleNotch } from '@phosphor-icons/vue'
import { api, type Document, type FileFormat } from '../lib/workspace'

const props = defineProps<{ date: string; document: Document | null; isNew: boolean; content: string; dirty: boolean; saving: boolean; disabled: boolean; error: string; savedAt: Date | null; canRestore: boolean; canUndo: boolean; canRedo: boolean; preview: boolean; storageMode?: 'files' | 'dual' | 'database' }>()
const emit = defineEmits<{ edit: [content: string]; undo: []; redo: []; save: []; reload: []; format: [value: FileFormat]; ai: []; restore: []; error: [value: string]; 'update:preview': [value: boolean] }>()
const textarea = ref<HTMLTextAreaElement>()
const editorBody = ref<HTMLElement>()
const preview = computed({ get: () => props.preview, set: (value: boolean) => emit('update:preview', value) })
const previewMarkup = ref('')
const copied = ref(false)
let dateAnimation: Animation | undefined
const newEmpty = computed(() => props.isNew && !props.content.trim())
const isMac = /Mac|iPhone|iPad|iPod/.test(navigator.platform || navigator.userAgent)
const undoTitle = `撤销（${isMac ? '⌘Z' : 'Ctrl+Z'}）`
const redoTitle = isMac ? '重做（⌘⇧Z）' : '重做（Ctrl+Y / Ctrl+Shift+Z）'
const rendered = computed(() => DOMPurify.sanitize(marked.parse(props.content, { async: false }) as string, { FORBID_TAGS: ['img', 'style', 'input', 'a'], FORBID_ATTR: ['style'] }))
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
watch(() => props.content, () => { copied.value = false })
watch(() => props.document?.format, () => { preview.value = false })
watch([preview, () => props.content], () => {
  if (preview.value) previewMarkup.value = rendered.value
}, { immediate: true })
watch(() => props.date, () => {
  dateAnimation?.cancel()
  const body = editorBody.value
  if (!body || typeof body.animate !== 'function' || window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) return
  dateAnimation = body.animate([
    { opacity: 0.35, transform: 'translateY(4px)' },
    { opacity: 1, transform: 'translateY(0)' },
  ], { duration: 160, easing: 'cubic-bezier(0.2, 0.8, 0.2, 1)' })
}, { flush: 'post' })
onBeforeUnmount(() => dateAnimation?.cancel())
async function insert(before: string, after = '') {
  preview.value = false; await nextTick()
  const input = textarea.value; if (!input) return
  const start = input.selectionStart; const end = input.selectionEnd
  const text = input.value.slice(start, end)
  emit('edit', input.value.slice(0, start) + before + text + after + input.value.slice(end))
  await nextTick(); input.focus(); input.setSelectionRange(start + before.length, start + before.length + text.length)
}
async function copyDraft() {
  try { if (isTauri()) await api.copy(props.content); else await navigator.clipboard.writeText(props.content); copied.value = true }
  catch { emit('error', '复制失败，请重试或选中文本后手动复制。') }
}
function focus() { preview.value = false; void nextTick(() => textarea.value?.focus()) }
defineExpose({ focus })
</script>
<template>
  <aside class="editor-panel" aria-label="日报编辑区" :aria-busy="disabled">
    <header class="editor-heading"><Transition name="inline-fade" mode="out-in"><div :key="date" class="editor-date-heading"><div class="editor-eyebrow">{{ dayjs(date).format('dddd') }}</div><h2>{{ dayjs(date).format('M 月 D 日') }}<span>的日报</span></h2></div></Transition><div class="editor-file-row"><span class="file-label"><PhFileText :size="14" />{{ document?.filename ?? `${date}.md` }}</span><div v-if="document" class="format-switch" role="group" aria-label="日报文件格式"><button :disabled="disabled" :aria-pressed="document.format === 'md'" @click="document.format !== 'md' && emit('format', 'md')">Markdown</button><button :disabled="disabled" :aria-pressed="document.format === 'txt'" @click="document.format !== 'txt' && emit('format', 'txt')">TXT</button></div></div></header>
    <div class="editor-state-stage">
    <Transition name="editor-state">
    <div v-if="document" key="document" class="editor-content">
      <div class="editor-text-actions"><button class="text-action history-action" :disabled="disabled || !canUndo" :title="undoTitle" @click="emit('undo')">撤销</button><button class="text-action history-action" :disabled="disabled || !canRedo" :title="redoTitle" @click="emit('redo')">重做</button><button class="text-action" :disabled="disabled || newEmpty" @click="copyDraft"><Transition name="button-label" mode="out-in"><span :key="String(copied)">{{ copied ? '已复制' : '复制' }}</span></Transition></button><button class="text-action" :disabled="disabled || newEmpty" @click="emit('ai')">AI 润色</button><button v-if="isNew || canRestore" class="text-action muted-action" :disabled="disabled || !canRestore" @click="emit('restore')">恢复原文</button></div>
      <div class="editor-toolbar"><template v-if="document.format === 'md'"><div class="format-actions" aria-label="Markdown 格式工具"><md-icon-button aria-label="插入标题" title="标题" :disabled="disabled" @click="insert('## ')"><PhTextH :size="18" /></md-icon-button><md-icon-button aria-label="加粗" title="加粗" :disabled="disabled" @click="insert('**', '**')"><PhTextB :size="18" /></md-icon-button><md-icon-button aria-label="插入列表" title="列表" :disabled="disabled" @click="insert('- ')"><PhListBullets :size="18" /></md-icon-button><md-icon-button aria-label="插入待办" title="待办" :disabled="disabled" @click="insert('- [ ] ')"><PhCheckSquare :size="18" /></md-icon-button></div><button class="preview-toggle" :aria-pressed="preview" @click="preview = !preview"><Transition name="button-label" mode="out-in"><span :key="String(preview)">{{ preview ? '编辑' : '预览' }}</span></Transition></button></template><template v-else><span class="editor-context-label">纯文本编辑</span><span class="editor-context-label">TXT</span></template></div>
      <div ref="editorBody" class="editor-body">
        <Transition name="editor-mode"><div v-show="preview && document.format === 'md'" class="markdown-preview journal-prose" :inert="!preview || document.format !== 'md'" v-html="previewMarkup"></div></Transition>
        <Transition name="editor-mode"><textarea v-show="!preview || document.format !== 'md'" ref="textarea" class="markdown-editor" :value="content" :disabled="disabled" :inert="preview && document.format === 'md'" spellcheck="false" aria-label="日报文本内容" placeholder="写下今天的工作…" @input="emit('edit', ($event.target as HTMLTextAreaElement).value)" @keydown.meta.s.prevent="emit('save')" @keydown.ctrl.s.prevent="emit('save')"></textarea></Transition>
      </div>
      <Transition name="inline-feedback"><div v-if="error && dirty" class="save-error-actions"><button @click="copyDraft">复制草稿</button><button @click="emit('reload')">重新载入</button></div></Transition>
      <footer class="editor-footer"><span :class="{ 'error-text': error && dirty }" role="status"><PhCircleNotch v-if="saving" class="spin" :size="14" /><PhCheck v-else-if="!dirty && !newEmpty" :size="14" />{{ status }}</span><span>{{ content.replace(/\s/g, '').length }} 字</span></footer>
    </div>
    <div v-else key="empty" class="editor-empty"><div class="empty-document"><PhFileText :size="38" weight="light" /></div><h3>正在载入日报…</h3><p>选择一个日期开始记录。</p></div>
    </Transition>
    </div>
  </aside>
</template>
