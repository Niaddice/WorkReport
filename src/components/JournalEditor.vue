<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import dayjs from 'dayjs'
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
  PhClockCounterClockwise,
} from '@phosphor-icons/vue'
import { api, type Document, type FileFormat } from '../lib/workspace'
import { historyApi, type HistoryVersion } from '../lib/history'

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
  storageMode?: 'files' | 'dual' | 'database'
}>()
const emit = defineEmits<{
  edit: [content: string]
  undo: []
  redo: []
  save: []
  reload: []
  format: [value: FileFormat]
  ai: []
  restore: []
  error: [value: string]
  'update:preview': [value: boolean]
}>()
const textarea = ref<HTMLTextAreaElement>()
const editorBody = ref<HTMLElement>()
const preview = computed({ get: () => props.preview, set: (value: boolean) => emit('update:preview', value) })
const previewMarkup = ref('')
const copied = ref(false)
let copiedTimer: ReturnType<typeof setTimeout> | undefined
let dateAnimation: Animation | undefined
const newEmpty = computed(() => props.isNew && !props.content.trim())
const hasContent = computed(() => props.content.trim().length > 0)
const historyOpen = ref(false)
const historyLoading = ref(false)
const historyError = ref('')
const historyList = ref<HistoryVersion[]>([])
const hoveredVersion = ref<HistoryVersion | null>(null)
const versionContent = ref('')
const versionLoading = ref(false)
const versionCache = new Map<string, string>()
let historyRequest = 0
let versionRequest = 0
const formatVersionTime = (millis: number) => dayjs(millis).format('HH:mm:ss')
const formatHistoryDate = (date: string) => `${date.slice(0, 4)}-${date.slice(4, 6)}-${date.slice(6, 8)}`
const historyGroups = computed(() => {
  const groups: { date: string; versions: HistoryVersion[] }[] = []
  for (const version of historyList.value) {
    const last = groups[groups.length - 1]
    if (last && last.date === version.date) last.versions.push(version)
    else groups.push({ date: version.date, versions: [version] })
  }
  return groups
})
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
    if (historyOpen.value) closeHistory()
  },
)
watch(
  () => props.document?.format,
  () => {
    preview.value = false
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
    closeHistory()
    versionCache.clear()
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
  },
  { flush: 'post' },
)
onBeforeUnmount(() => {
  dateAnimation?.cancel()
  clearTimeout(copiedTimer)
  window.removeEventListener('keydown', onHistoryKey)
})
async function insert(before: string, after = '') {
  preview.value = false
  await nextTick()
  const input = textarea.value
  if (!input) return
  const start = input.selectionStart
  const end = input.selectionEnd
  const text = input.value.slice(start, end)
  emit('edit', input.value.slice(0, start) + before + text + after + input.value.slice(end))
  await nextTick()
  input.focus()
  input.setSelectionRange(start + before.length, start + before.length + text.length)
}
async function copyDraft() {
  try {
    if (isTauri()) await api.copy(props.content)
    else await navigator.clipboard.writeText(props.content)
    copied.value = true
    clearTimeout(copiedTimer)
    copiedTimer = setTimeout(() => {
      copied.value = false
    }, 1600)
  } catch {
    emit('error', '复制失败，请重试或选中文本后手动复制。')
  }
}
function closeHistory() {
  historyOpen.value = false
  hoveredVersion.value = null
  versionContent.value = ''
}
async function openHistory() {
  historyOpen.value = true
  hoveredVersion.value = null
  versionContent.value = ''
  historyList.value = []
  historyError.value = ''
  const id = ++historyRequest
  historyLoading.value = true
  try {
    const versions = await historyApi.list()
    if (id === historyRequest) historyList.value = versions
  } catch (cause) {
    if (id === historyRequest) historyError.value = String(cause)
  } finally {
    if (id === historyRequest) historyLoading.value = false
  }
}
function toggleHistory() {
  if (historyOpen.value) closeHistory()
  else void openHistory()
}
async function previewVersion(version: HistoryVersion) {
  hoveredVersion.value = version
  const key = `${version.date}/${version.id}`
  const cached = versionCache.get(key)
  if (cached !== undefined) {
    versionContent.value = cached
    versionLoading.value = false
    return
  }
  const id = ++versionRequest
  versionLoading.value = true
  try {
    const content = await historyApi.read(version.date, version.id)
    versionCache.set(key, content)
    if (id === versionRequest && hoveredVersion.value?.id === version.id) versionContent.value = content
  } catch (cause) {
    if (id === versionRequest) versionContent.value = `读取失败：${String(cause)}`
  } finally {
    if (id === versionRequest) versionLoading.value = false
  }
}
function onHistoryKey(event: KeyboardEvent) {
  if (event.key === 'Escape' && historyOpen.value) closeHistory()
}
watch(historyOpen, (open) => {
  if (open) window.addEventListener('keydown', onHistoryKey)
  else window.removeEventListener('keydown', onHistoryKey)
})
function focus() {
  preview.value = false
  void nextTick(() => textarea.value?.focus())
}
defineExpose({ focus, openHistory })
</script>
<template>
  <!-- eslint-disable vue/no-v-html -- Markdown 预览先经 DOMPurify 清洗再渲染 -->
  <aside class="editor-panel" aria-label="日报编辑区" :aria-busy="disabled">
    <header class="editor-heading">
      <Transition name="inline-fade" mode="out-in"
        ><div :key="date" class="editor-date-heading">
          <div class="editor-eyebrow">{{ dayjs(date).format('dddd') }}</div>
          <h2>{{ dayjs(date).format('M 月 D 日') }}<span>的日报</span></h2>
        </div></Transition
      >
      <div class="editor-file-row">
        <span class="file-label"><PhFileText :size="14" />{{ document?.filename ?? `${date}.md` }}</span>
        <div v-if="document" class="format-switch" role="group" aria-label="日报文件格式">
          <button
            :disabled="disabled"
            :aria-pressed="document.format === 'md'"
            @click="document.format !== 'md' && emit('format', 'md')"
          >
            Markdown</button
          ><button
            :disabled="disabled"
            :aria-pressed="document.format === 'txt'"
            @click="document.format !== 'txt' && emit('format', 'txt')"
          >
            TXT
          </button>
        </div>
      </div>
    </header>
    <div class="editor-state-stage">
      <Transition name="editor-state">
        <div v-if="document" key="document" class="editor-content">
          <div class="editor-toolbar">
            <div v-if="document.format === 'md'" class="format-actions" aria-label="Markdown 格式工具">
              <md-icon-button aria-label="插入标题" title="标题" :disabled="disabled" @click="insert('## ')"
                ><PhTextH :size="18" /></md-icon-button
              ><md-icon-button aria-label="加粗" title="加粗" :disabled="disabled" @click="insert('**', '**')"
                ><PhTextB :size="18" /></md-icon-button
              ><md-icon-button aria-label="插入列表" title="列表" :disabled="disabled" @click="insert('- ')"
                ><PhListBullets :size="18" /></md-icon-button
              ><md-icon-button aria-label="插入待办" title="待办" :disabled="disabled" @click="insert('- [ ] ')"
                ><PhCheckSquare :size="18"
              /></md-icon-button>
            </div>
            <span v-else class="editor-context-label">纯文本编辑</span>
            <div class="editor-toolbar-actions">
              <button
                v-if="document.format === 'md'"
                class="preview-toggle"
                :aria-pressed="preview"
                @click="preview = !preview"
              >
                <Transition name="button-label" mode="out-in"
                  ><span :key="String(preview)">{{ preview ? '编辑' : '预览' }}</span></Transition
                >
              </button>
              <button class="ai-toggle" :disabled="disabled || !hasContent" @click="emit('ai')">AI</button>
            </div>
          </div>
          <div ref="editorBody" class="editor-body">
            <Transition name="editor-mode"
              ><div
                v-show="preview && document.format === 'md'"
                class="markdown-preview journal-prose"
                :inert="!preview || document.format !== 'md'"
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
                @keydown.meta.s.prevent="emit('save')"
                @keydown.ctrl.s.prevent="emit('save')"
              ></textarea>
            </Transition>
            <button
              v-if="hasContent"
              type="button"
              class="history-icon"
              :class="{ active: historyOpen }"
              :aria-expanded="historyOpen"
              aria-haspopup="dialog"
              title="历史记录"
              aria-label="历史记录"
              :disabled="disabled"
              @click="toggleHistory"
            >
              <PhClockCounterClockwise :size="15" />
            </button>
            <button
              v-if="hasContent"
              type="button"
              class="copy-icon"
              :title="copied ? '已复制' : '复制日报'"
              :aria-label="copied ? '已复制' : '复制日报'"
              :disabled="disabled"
              @click="copyDraft"
            >
              <Transition name="button-label" mode="out-in"
                ><PhCheck v-if="copied" key="copied" :size="14" /><PhCopy v-else key="copy" :size="14"
              /></Transition>
            </button>
            <div v-if="historyOpen" class="history-backdrop" @pointerdown="closeHistory"></div>
            <aside v-if="historyOpen" class="history-panel" role="dialog" aria-label="日报历史记录" @pointerdown.stop>
              <header>
                <span>历史记录</span><span>{{ historyLoading ? '' : `${historyList.length} 个版本` }}</span>
              </header>
              <p v-if="historyLoading" class="history-note">正在加载…</p>
              <p v-else-if="historyError" class="history-note history-error">{{ historyError }}</p>
              <p v-else-if="!historyList.length" class="history-note">还没有保存过版本</p>
              <div v-else class="history-groups">
                <section v-for="group in historyGroups" :key="group.date" class="history-group">
                  <p class="history-group-title">{{ formatHistoryDate(group.date) }}</p>
                  <ul>
                    <li v-for="version in group.versions" :key="version.id">
                      <button
                        type="button"
                        class="history-item"
                        :class="{ active: hoveredVersion?.id === version.id }"
                        @mouseenter="previewVersion(version)"
                        @focus="previewVersion(version)"
                      >
                        <span class="history-time">{{ formatVersionTime(version.savedAt) }}</span
                        ><span class="history-meta">{{ version.format === 'md' ? 'MD' : 'TXT' }}</span>
                      </button>
                    </li>
                  </ul>
                </section>
              </div>
            </aside>
            <aside
              v-if="historyOpen && hoveredVersion"
              class="history-detail"
              role="dialog"
              aria-label="历史版本内容"
              @pointerdown.stop
            >
              <header>
                <span>{{ formatHistoryDate(hoveredVersion.date) }} {{ formatVersionTime(hoveredVersion.savedAt) }}</span
                ><span>{{ hoveredVersion.format === 'md' ? 'Markdown' : '纯文本' }}</span>
              </header>
              <p v-if="versionLoading" class="history-note">正在读取…</p>
              <pre v-else>{{ versionContent }}</pre>
            </aside>
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
</template>
