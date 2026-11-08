<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import dayjs from 'dayjs'
import {
  PhArrowUUpLeft,
  PhCaretDown,
  PhCheck,
  PhCircleNotch,
  PhColumns,
  PhCopy,
  PhGitDiff,
  PhListDashes,
  PhX,
} from '@phosphor-icons/vue'
import { createDialogMotion } from '../lib/dialogMotion'
import { computeLineDiff, type DiffResult } from '../lib/lineDiff'
import type { HistoryVersion } from '../lib/history'

const props = defineProps<{
  baseVersion: HistoryVersion | null
  availableVersions: HistoryVersion[]
  currentDraftContent: string
  readContent: (version: HistoryVersion) => Promise<string>
  isMac?: boolean
}>()

const emit = defineEmits<{
  close: []
  restore: [version: HistoryVersion, content: string]
  copy: [content: string]
}>()

const dialog = ref<HTMLDialogElement>()
const motion = createDialogMotion(() => dialog.value)

type ViewMode = 'split' | 'unified'
const viewMode = ref<ViewMode>('split')
const targetId = ref<string>('current')

const baseContent = ref<string>('')
const targetContent = ref<string>('')
const loading = ref<boolean>(false)
const error = ref<string>('')
const confirmingRestore = ref<boolean>(false)
const copied = ref<boolean>(false)
let copiedTimer: ReturnType<typeof setTimeout> | undefined

let loadRevision = 0

function formatTime(timestamp: number) {
  return dayjs(timestamp).format('HH:mm:ss')
}

function formatDate(dateStr: string) {
  if (dateStr.length === 8) {
    return `${dateStr.slice(0, 4)}年${dateStr.slice(4, 6)}月${dateStr.slice(6, 8)}日`
  }
  return dateStr
}

function badgeOf(version: HistoryVersion) {
  return version.format === 'md' ? 'MD' : 'TXT'
}

const targetVersion = computed<HistoryVersion | null>(() => {
  if (targetId.value === 'current') return null
  return props.availableVersions.find((v) => v.id === targetId.value) ?? null
})

const diff = computed<DiffResult>(() => {
  return computeLineDiff(baseContent.value, targetContent.value)
})

const isTargetCurrent = computed(() => targetId.value === 'current')

async function loadContents() {
  const revision = ++loadRevision
  if (!props.baseVersion) return

  loading.value = true
  error.value = ''
  confirmingRestore.value = false

  try {
    const basePromise = props.readContent(props.baseVersion)
    const targetPromise = isTargetCurrent.value
      ? Promise.resolve(props.currentDraftContent)
      : targetVersion.value
        ? props.readContent(targetVersion.value)
        : Promise.resolve('')

    const [baseText, targetText] = await Promise.all([basePromise, targetPromise])
    if (revision !== loadRevision) return

    baseContent.value = baseText
    targetContent.value = targetText
  } catch (err) {
    if (revision === loadRevision) {
      error.value = `读取对比内容失败：${String(err)}`
    }
  } finally {
    if (revision === loadRevision) {
      loading.value = false
    }
  }
}

watch(
  [() => props.baseVersion, targetId, () => props.currentDraftContent],
  () => {
    if (dialog.value?.open && props.baseVersion) {
      void loadContents()
    }
  },
  { deep: false },
)

function open() {
  targetId.value = 'current'
  confirmingRestore.value = false
  copied.value = false
  motion.open()
  void nextTick(() => {
    void loadContents()
  })
}

function close() {
  confirmingRestore.value = false
  motion.close()
  emit('close')
}

function handleRestore() {
  if (!props.baseVersion) return
  confirmingRestore.value = false
  emit('restore', props.baseVersion, baseContent.value)
  close()
}

function handleCopy() {
  emit('copy', baseContent.value)
  copied.value = true
  clearTimeout(copiedTimer)
  copiedTimer = setTimeout(() => {
    copied.value = false
  }, 1600)
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    if (confirmingRestore.value) {
      confirmingRestore.value = false
    } else {
      close()
    }
  }
}

defineExpose({
  open,
  close,
  isOpen: () => !!dialog.value?.open,
})

onBeforeUnmount(() => {
  clearTimeout(copiedTimer)
  motion.dispose()
})
</script>

<template>
  <dialog
    ref="dialog"
    tabindex="-1"
    role="dialog"
    aria-modal="true"
    class="history-diff-dialog confirmation-dialog"
    aria-labelledby="history-diff-title"
    @cancel.prevent="close"
    @click.self="close"
    @keydown="onKeydown"
  >
    <div class="history-diff-window">
      <!-- 弹窗顶栏 -->
      <header class="history-diff-header">
        <div class="history-diff-title-wrap">
          <div class="history-diff-icon" aria-hidden="true">
            <PhGitDiff :size="18" />
          </div>
          <div>
            <h3 id="history-diff-title" class="history-diff-title">历史版本差异对比</h3>
            <p v-if="baseVersion" class="history-diff-subtitle">
              {{ formatDate(baseVersion.date) }} · 快照时间 {{ formatTime(baseVersion.savedAt) }} ({{
                badgeOf(baseVersion)
              }})
            </p>
          </div>
        </div>

        <!-- 差异统计徽章 -->
        <div class="history-diff-toolbar">
          <div v-if="!loading && !error" class="history-diff-stats">
            <span v-if="diff.identical" class="stats-identical">内容完全一致</span>
            <template v-else>
              <span class="stats-add" title="新增行数">+{{ diff.additions }}</span>
              <span class="stats-remove" title="删除行数">−{{ diff.deletions }}</span>
            </template>
          </div>

          <!-- 视图模式切换 -->
          <div class="history-diff-view-mode" role="radiogroup" aria-label="差异视图模式">
            <button
              type="button"
              class="mode-btn"
              :class="{ 'is-active': viewMode === 'split' }"
              role="radio"
              :aria-checked="viewMode === 'split'"
              title="双栏并排对比"
              @click="viewMode = 'split'"
            >
              <PhColumns :size="14" aria-hidden="true" />
              <span>并排</span>
            </button>
            <button
              type="button"
              class="mode-btn"
              :class="{ 'is-active': viewMode === 'unified' }"
              role="radio"
              :aria-checked="viewMode === 'unified'"
              title="单栏逐行对比"
              @click="viewMode = 'unified'"
            >
              <PhListDashes :size="14" aria-hidden="true" />
              <span>逐行</span>
            </button>
          </div>

          <button
            type="button"
            class="history-diff-close-btn"
            aria-label="关闭对比窗口"
            title="关闭 (Esc)"
            @click="close"
          >
            <PhX :size="16" aria-hidden="true" />
          </button>
        </div>
      </header>

      <!-- 对比目标选择器条 -->
      <div class="history-diff-target-bar">
        <div class="target-col-label">
          <span class="label-tag base">基准</span>
          <strong>历史快照 ({{ baseVersion ? formatTime(baseVersion.savedAt) : '' }})</strong>
        </div>
        <div class="target-col-arrow" aria-hidden="true">➔</div>
        <div class="target-col-selector">
          <span class="label-tag target">目标</span>
          <label for="history-diff-target-select" class="sr-only">选择对比目标</label>
          <div class="select-wrapper">
            <select id="history-diff-target-select" v-model="targetId" class="target-select">
              <option value="current">当前编辑区最新草稿</option>
              <template v-for="ver in availableVersions" :key="ver.id">
                <option v-if="ver.id !== baseVersion?.id" :value="ver.id">
                  历史快照 · {{ formatTime(ver.savedAt) }} ({{ badgeOf(ver) }})
                </option>
              </template>
            </select>
            <PhCaretDown class="select-caret" :size="12" aria-hidden="true" />
          </div>
        </div>
      </div>

      <!-- 差异内容主区 -->
      <main class="history-diff-body" tabindex="0">
        <div v-if="loading" class="history-diff-placeholder">
          <PhCircleNotch class="spin" :size="24" aria-hidden="true" />
          <span>正在计算版本差异…</span>
        </div>

        <div v-else-if="error" class="history-diff-placeholder error">
          <p>{{ error }}</p>
        </div>

        <div v-else-if="diff.identical" class="history-diff-placeholder identical">
          <p>两个版本内容完全一致，没有文本修改。</p>
        </div>

        <!-- 并排双栏模式 -->
        <div v-else-if="viewMode === 'split'" class="history-diff-split-view">
          <div class="split-view-header">
            <div class="split-col-header">
              <span>基准历史版本 (旧)</span>
            </div>
            <div class="split-col-header">
              <span>{{ isTargetCurrent ? '当前编辑区草稿 (新)' : '对比历史快照 (新)' }}</span>
            </div>
          </div>
          <div class="split-view-content">
            <div v-for="(row, idx) in diff.splitRows" :key="idx" class="split-diff-row">
              <!-- 左侧 (基准/旧) -->
              <div class="split-side left" :class="`diff-${row.left.kind}`">
                <span class="line-num" aria-hidden="true">{{ row.left.line ?? '' }}</span>
                <span class="line-marker" aria-hidden="true">{{ row.left.kind === 'remove' ? '−' : ' ' }}</span>
                <span class="line-text">{{ row.left.text || ' ' }}</span>
              </div>
              <!-- 右侧 (目标/新) -->
              <div class="split-side right" :class="`diff-${row.right.kind}`">
                <span class="line-num" aria-hidden="true">{{ row.right.line ?? '' }}</span>
                <span class="line-marker" aria-hidden="true">{{ row.right.kind === 'add' ? '+' : ' ' }}</span>
                <span class="line-text">{{ row.right.text || ' ' }}</span>
              </div>
            </div>
          </div>
        </div>

        <!-- 统一逐行模式 -->
        <div v-else class="history-diff-unified-view">
          <div class="unified-view-content">
            <div v-for="(line, idx) in diff.lines" :key="idx" class="unified-diff-row" :class="`diff-${line.kind}`">
              <span class="line-num old" aria-hidden="true">{{ line.oldLine ?? '' }}</span>
              <span class="line-num new" aria-hidden="true">{{ line.newLine ?? '' }}</span>
              <span class="line-marker" aria-hidden="true">
                {{ line.kind === 'add' ? '+' : line.kind === 'remove' ? '−' : ' ' }}
              </span>
              <span class="line-text">{{ line.text || ' ' }}</span>
            </div>
          </div>
        </div>
      </main>

      <!-- 弹窗底栏 -->
      <footer class="history-diff-footer">
        <div class="footer-meta">
          <span>提示：按 Esc 键可随时关闭</span>
        </div>

        <div class="footer-actions">
          <button type="button" class="btn secondary" title="复制所选历史版本的全文" @click="handleCopy">
            <PhCheck v-if="copied" :size="14" aria-hidden="true" />
            <PhCopy v-else :size="14" aria-hidden="true" />
            <span>{{ copied ? '已复制' : '复制此版本全文' }}</span>
          </button>

          <!-- 恢复确认操作条 -->
          <template v-if="confirmingRestore">
            <div class="restore-confirm-popover" role="alertdialog">
              <span class="confirm-prompt">覆盖并恢复到此版本？</span>
              <button type="button" class="btn danger small" @click="handleRestore">确认恢复</button>
              <button type="button" class="btn subtle small" @click="confirmingRestore = false">取消</button>
            </div>
          </template>
          <template v-else>
            <button
              type="button"
              class="btn primary restore-btn"
              title="将当前编辑区内容恢复为此历史版本（可通过 ⌘Z 撤销）"
              :disabled="loading || !baseVersion || diff.identical"
              @click="confirmingRestore = true"
            >
              <PhArrowUUpLeft :size="14" aria-hidden="true" />
              <span>恢复此历史版本</span>
            </button>
          </template>

          <button type="button" class="btn default" @click="close">关闭</button>
        </div>
      </footer>
    </div>
  </dialog>
</template>

<style scoped>
.history-diff-dialog {
  border: 1px solid var(--skin-border);
  border-radius: var(--radius-lg, 12px);
  background: var(--skin-surface);
  color: var(--skin-text);
  box-shadow: 0 16px 48px rgba(0, 0, 0, 0.28);
  padding: 0;
  width: min(1080px, calc(100vw - 32px));
  max-width: min(1080px, calc(100vw - 32px));
  height: min(720px, calc(100vh - 48px));
  max-height: min(720px, calc(100vh - 48px));
  overflow: hidden;
}

.history-diff-window {
  display: flex;
  flex-direction: column;
  height: 100%;
}

/* 顶栏 */
.history-diff-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 18px;
  border-bottom: 1px solid var(--skin-border);
  background: var(--skin-canvas);
  gap: 16px;
}

.history-diff-title-wrap {
  display: flex;
  align-items: center;
  gap: 10px;
}

.history-diff-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: 8px;
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
}

.history-diff-title {
  margin: 0;
  font-size: var(--text-base);
  font-weight: 600;
  line-height: 1.25;
}

.history-diff-subtitle {
  margin: 2px 0 0 0;
  font-size: var(--text-caption);
  color: var(--skin-text-muted);
}

.history-diff-toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
}

.history-diff-stats {
  display: flex;
  align-items: center;
  gap: 8px;
  font-family: 'SFMono-Regular', Consolas, monospace;
  font-size: var(--text-small);
  font-weight: 600;
}

.stats-identical {
  color: var(--skin-text-muted);
  font-weight: normal;
  font-size: var(--text-caption);
  background: var(--skin-subtle);
  padding: 2px 8px;
  border-radius: 4px;
}

.stats-add {
  color: var(--skin-success);
}

.stats-remove {
  color: var(--skin-danger);
}

.history-diff-view-mode {
  display: inline-flex;
  border: 1px solid var(--skin-border);
  border-radius: 6px;
  padding: 2px;
  background: var(--skin-surface);
  gap: 2px;
}

.mode-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 8px;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: var(--skin-text-muted);
  font: inherit;
  font-size: var(--text-caption);
  cursor: pointer;
  transition: all 120ms ease;
}

.mode-btn:hover {
  color: var(--skin-text);
}

.mode-btn.is-active {
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
  font-weight: 500;
}

.history-diff-close-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--skin-text-muted);
  cursor: pointer;
  transition: all 120ms ease;
}

.history-diff-close-btn:hover {
  background: var(--skin-subtle);
  color: var(--skin-text);
}

/* 目标选择条 */
.history-diff-target-bar {
  display: flex;
  align-items: center;
  padding: 8px 18px;
  background: color-mix(in srgb, var(--skin-canvas) 60%, var(--skin-surface));
  border-bottom: 1px solid var(--skin-border);
  font-size: var(--text-small);
  gap: 12px;
}

.label-tag {
  display: inline-block;
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 600;
  margin-right: 6px;
}

.label-tag.base {
  background: color-mix(in srgb, var(--skin-danger) 15%, transparent);
  color: var(--skin-danger);
}

.label-tag.target {
  background: color-mix(in srgb, var(--skin-success) 15%, transparent);
  color: var(--skin-success);
}

.target-col-label {
  display: flex;
  align-items: center;
}

.target-col-arrow {
  color: var(--skin-text-muted);
  font-size: 13px;
}

.target-col-selector {
  display: flex;
  align-items: center;
  flex: 1;
}

.select-wrapper {
  position: relative;
  display: inline-flex;
  align-items: center;
}

.target-select {
  appearance: none;
  background: var(--skin-surface);
  border: 1px solid var(--skin-border);
  border-radius: 6px;
  padding: 3px 24px 3px 10px;
  font: inherit;
  font-size: var(--text-small);
  color: var(--skin-text);
  cursor: pointer;
  min-width: 220px;
  outline: none;
}

.target-select:focus {
  border-color: var(--skin-accent);
}

.select-caret {
  position: absolute;
  right: 8px;
  pointer-events: none;
  color: var(--skin-text-muted);
}

/* 主内容区 */
.history-diff-body {
  flex: 1;
  overflow: auto;
  background: var(--skin-canvas);
  font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', monospace;
  font-size: calc(12 * var(--font-unit));
  line-height: 1.6;
  outline: none;
}

.history-diff-placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  height: 100%;
  color: var(--skin-text-muted);
  font-family: inherit;
  font-size: var(--text-base);
}

.history-diff-placeholder.error {
  color: var(--skin-danger);
}

.history-diff-placeholder.identical {
  color: var(--skin-text-muted);
}

/* 并排视图 */
.history-diff-split-view {
  min-width: 100%;
  display: flex;
  flex-direction: column;
}

.split-view-header {
  display: grid;
  grid-template-columns: 1fr 1fr;
  position: sticky;
  top: 0;
  z-index: 2;
  background: var(--skin-surface);
  border-bottom: 1px solid var(--skin-border);
  font-family: var(--font-sans, system-ui, sans-serif);
  font-size: 11px;
  font-weight: 600;
  color: var(--skin-text-muted);
}

.split-col-header {
  padding: 4px 12px;
}

.split-col-header + .split-col-header {
  border-left: 1px solid var(--skin-border);
}

.split-diff-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  min-height: 20px;
  width: max-content;
  min-width: 100%;
}

.split-side {
  display: grid;
  grid-template-columns: 4.5ch 2ch 1fr;
  align-items: baseline;
  white-space: pre-wrap;
  word-break: break-all;
  padding: 0 6px 0 0;
}

.split-side + .split-side {
  border-left: 1px solid var(--skin-border);
}

.split-side.diff-remove {
  background: var(--skin-danger-soft);
  color: var(--skin-text);
}

.split-side.diff-add {
  background: var(--skin-success-soft);
  color: var(--skin-text);
}

.split-side.diff-empty {
  background: color-mix(in srgb, var(--skin-subtle) 40%, transparent);
}

.split-side.diff-context {
  color: var(--skin-text);
}

/* 逐行视图 */
.history-diff-unified-view {
  min-width: 100%;
}

.unified-diff-row {
  display: grid;
  grid-template-columns: 4.5ch 4.5ch 2ch 1fr;
  align-items: baseline;
  white-space: pre-wrap;
  word-break: break-all;
  padding: 0 8px 0 0;
  min-height: 20px;
  width: max-content;
  min-width: 100%;
}

.unified-diff-row.diff-remove {
  background: var(--skin-danger-soft);
}

.unified-diff-row.diff-add {
  background: var(--skin-success-soft);
}

/* 行号与标记 */
.line-num {
  text-align: right;
  padding-right: 8px;
  color: var(--skin-text-muted);
  user-select: none;
  font-size: 11px;
}

.line-marker {
  text-align: center;
  color: var(--skin-text-muted);
  user-select: none;
  font-weight: bold;
}

.diff-remove .line-marker {
  color: var(--skin-danger);
}

.diff-add .line-marker {
  color: var(--skin-success);
}

.line-text {
  padding-left: 4px;
}

/* 底栏 */
.history-diff-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 18px;
  border-top: 1px solid var(--skin-border);
  background: var(--skin-canvas);
  gap: 12px;
}

.footer-meta {
  font-size: var(--text-caption);
  color: var(--skin-text-muted);
}

.footer-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  position: relative;
}

.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 12px;
  border-radius: 6px;
  font: inherit;
  font-size: var(--text-small);
  cursor: pointer;
  border: 1px solid var(--skin-border);
  background: var(--skin-surface);
  color: var(--skin-text);
  transition: all 120ms ease;
}

.btn:hover:not(:disabled) {
  background: var(--skin-subtle);
  border-color: var(--skin-border);
}

.btn:disabled {
  opacity: 0.5;
  cursor: default;
}

.btn.primary {
  background: var(--skin-accent);
  color: var(--skin-on-accent, #ffffff);
  border-color: var(--skin-accent);
}

.btn.primary:hover:not(:disabled) {
  opacity: 0.9;
}

.btn.danger {
  background: var(--skin-danger);
  color: var(--skin-on-solid, #ffffff);
  border-color: var(--skin-danger);
}

.btn.subtle {
  background: transparent;
  border-color: transparent;
  color: var(--skin-text-muted);
}

.btn.small {
  height: 26px;
  padding: 0 8px;
  font-size: 12px;
}

.restore-confirm-popover {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 10px;
  border-radius: 6px;
  background: var(--skin-surface);
  border: 1px solid var(--skin-danger);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  animation: fadeIn 120ms ease;
}

.confirm-prompt {
  font-size: var(--text-caption);
  color: var(--skin-danger);
  font-weight: 500;
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border-width: 0;
}

.spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

@keyframes fadeIn {
  from {
    opacity: 0;
    transform: scale(0.96);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}
</style>
