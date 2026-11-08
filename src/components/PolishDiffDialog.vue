<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { PhCheck, PhColumns, PhCopy, PhListDashes, PhMagicWand, PhX } from '@phosphor-icons/vue'
import { isTauri } from '@tauri-apps/api/core'
import { createDialogMotion } from '../lib/dialogMotion'
import { computeLineDiff, type DiffResult } from '../lib/lineDiff'
import { api } from '../lib/workspace'

const props = withDefaults(
  defineProps<{
    visible: boolean
    originalText: string
    polishedText: string
    isSelection: boolean
    optionLabel?: string
  }>(),
  {
    optionLabel: '智能润色',
  },
)

const emit = defineEmits<{
  accept: []
  discard: []
}>()

const dialog = ref<HTMLDialogElement>()
const motion = createDialogMotion(() => dialog.value)

type ViewMode = 'split' | 'unified'
const viewMode = ref<ViewMode>('split')
const copied = ref(false)
let copiedTimer: ReturnType<typeof setTimeout> | undefined

const diff = computed<DiffResult>(() => {
  return computeLineDiff(props.originalText, props.polishedText)
})

const originalLen = computed(() => props.originalText.length)
const polishedLen = computed(() => props.polishedText.length)
const charDelta = computed(() => polishedLen.value - originalLen.value)
const deltaText = computed(() => {
  if (charDelta.value === 0) return '字数未变'
  if (charDelta.value > 0) return `增加 ${charDelta.value} 字`
  return `精简 ${Math.abs(charDelta.value)} 字`
})
const deltaClass = computed(() => {
  if (charDelta.value === 0) return 'neutral'
  if (charDelta.value > 0) return 'addition'
  return 'reduction'
})

watch(
  () => props.visible,
  (val) => {
    if (val) {
      copied.value = false
      motion.open()
      void nextTick(() => {
        dialog.value?.focus()
      })
    } else {
      void motion.close()
    }
  },
  { immediate: true, flush: 'post' },
)

onBeforeUnmount(() => {
  clearTimeout(copiedTimer)
  motion.dispose()
})

async function handleCopy() {
  try {
    if (isTauri()) {
      await api.copy(props.polishedText)
    } else {
      await navigator.clipboard.writeText(props.polishedText)
    }
    copied.value = true
    clearTimeout(copiedTimer)
    copiedTimer = setTimeout(() => {
      copied.value = false
    }, 2000)
  } catch {
    /* Copy failure must not crash */
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    emit('discard')
  } else if (event.key === 'Enter') {
    const active = document.activeElement
    if (active instanceof HTMLButtonElement && !active.classList.contains('polish-diff-accept')) {
      return
    }
    event.preventDefault()
    emit('accept')
  }
}
</script>

<template>
  <Teleport to="body">
    <dialog
      id="polish-diff-dialog"
      ref="dialog"
      tabindex="-1"
      role="dialog"
      aria-modal="true"
      class="polish-diff-dialog confirmation-dialog"
      aria-labelledby="polish-diff-title"
      @cancel.prevent="emit('discard')"
      @click.self="emit('discard')"
      @keydown="onKeydown"
    >
      <div class="polish-diff-window">
        <!-- 弹窗顶栏 -->
        <header class="polish-diff-header">
          <div class="polish-diff-title-wrap">
            <div class="polish-diff-icon" aria-hidden="true">
              <PhMagicWand :size="20" />
            </div>
            <div>
              <div class="polish-diff-title-row">
                <h3 id="polish-diff-title" class="polish-diff-title">AI 润色效果对比</h3>
                <span class="polish-badge scope">{{ isSelection ? '选区润色' : '全文润色' }}</span>
                <span v-if="optionLabel" class="polish-badge option">{{ optionLabel }}</span>
              </div>
              <div class="polish-diff-stats-row">
                <span class="char-stat">
                  原 <strong>{{ originalLen }}</strong> 字 → 现 <strong>{{ polishedLen }}</strong> 字
                </span>
                <span class="char-delta" :class="deltaClass">（{{ deltaText }}）</span>
                <span v-if="!diff.identical" class="line-stats" aria-label="增删行数统计">
                  <span class="stats-add">+{{ diff.additions }}</span>
                  <span class="stats-remove">−{{ diff.deletions }}</span>
                </span>
              </div>
            </div>
          </div>

          <div class="polish-diff-toolbar">
            <!-- 视图模式切换 -->
            <div class="diff-view-mode-toggle" role="radiogroup" aria-label="差异视图模式">
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
              class="polish-diff-close-btn"
              aria-label="关闭对比并放弃 (Esc)"
              title="关闭 (Esc)"
              @click="emit('discard')"
            >
              <PhX :size="16" aria-hidden="true" />
            </button>
          </div>
        </header>

        <!-- 差异对比内容区 -->
        <main class="polish-diff-body" tabindex="0">
          <div v-if="diff.identical" class="polish-diff-identical">
            <p>润色结果与原内容完全一致，没有文本修改。</p>
          </div>

          <!-- 双栏并排模式 -->
          <div v-else-if="viewMode === 'split'" class="polish-diff-split-view">
            <div class="split-view-header">
              <div class="split-col-header">
                <span>原文内容</span>
              </div>
              <div class="split-col-header">
                <span>AI 润色后内容</span>
              </div>
            </div>
            <div class="split-view-content">
              <div v-for="(row, idx) in diff.splitRows" :key="idx" class="split-diff-row">
                <!-- 左栏：原文 (删除高亮) -->
                <div class="split-side left" :class="`diff-${row.left.kind}`">
                  <span class="line-num" aria-hidden="true">{{ row.left.line ?? '' }}</span>
                  <span class="line-marker" aria-hidden="true">{{ row.left.kind === 'remove' ? '−' : ' ' }}</span>
                  <span class="line-text">{{ row.left.text || ' ' }}</span>
                </div>
                <!-- 右栏：润色后 (新增高亮) -->
                <div class="split-side right" :class="`diff-${row.right.kind}`">
                  <span class="line-num" aria-hidden="true">{{ row.right.line ?? '' }}</span>
                  <span class="line-marker" aria-hidden="true">{{ row.right.kind === 'add' ? '+' : ' ' }}</span>
                  <span class="line-text">{{ row.right.text || ' ' }}</span>
                </div>
              </div>
            </div>
          </div>

          <!-- 单栏逐行模式 -->
          <div v-else class="polish-diff-unified-view">
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

        <!-- 弹窗底栏操作 -->
        <footer class="polish-diff-footer">
          <div class="footer-left">
            <button type="button" class="btn-copy" title="复制润色后的全文内容" @click="handleCopy">
              <PhCheck v-if="copied" :size="14" aria-hidden="true" />
              <PhCopy v-else :size="14" aria-hidden="true" />
              <span>{{ copied ? '已复制结果' : '复制润色结果' }}</span>
            </button>
          </div>

          <div class="footer-right">
            <button
              type="button"
              class="dialog-button dialog-button-secondary polish-diff-discard"
              title="不应用本次润色改动 (Esc)"
              @click="emit('discard')"
            >
              放弃 (Esc)
            </button>
            <button
              type="button"
              class="dialog-button dialog-button-primary polish-diff-accept"
              title="采纳本次润色并替换到编辑区 (Enter)"
              @click="emit('accept')"
            >
              <PhCheck :size="15" aria-hidden="true" />
              <span>采纳并替换 (Enter)</span>
            </button>
          </div>
        </footer>
      </div>
    </dialog>
  </Teleport>
</template>

<style scoped>
.polish-diff-dialog {
  padding: 0;
  border: 1px solid var(--skin-border);
  border-radius: 12px;
  background: var(--skin-canvas);
  color: var(--skin-text);
  box-shadow: 0 16px 40px rgba(0, 0, 0, 0.28);
  width: min(980px, calc(100vw - 32px));
  max-width: min(980px, calc(100vw - 32px));
  height: min(720px, calc(100vh - 48px));
  max-height: min(720px, calc(100vh - 48px));
  outline: none;
  overflow: hidden;
}

.polish-diff-window {
  display: flex;
  flex-direction: column;
  height: 100%;
  width: 100%;
  min-height: 0;
}

/* 顶栏 */
.polish-diff-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 14px 18px;
  background: var(--skin-surface);
  border-bottom: 1px solid var(--skin-border);
  flex-shrink: 0;
}

.polish-diff-title-wrap {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}

.polish-diff-icon {
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
  border-radius: 8px;
  flex-shrink: 0;
}

.polish-diff-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.polish-diff-title {
  margin: 0;
  font-size: var(--text-subtitle, 16px);
  font-weight: 600;
  color: var(--skin-text);
}

.polish-badge {
  font-size: calc(11 * var(--font-unit));
  padding: 2px 7px;
  border-radius: 4px;
  line-height: 1.2;
  font-weight: 500;
}

.polish-badge.scope {
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
}

.polish-badge.option {
  background: var(--skin-subtle-strong);
  color: var(--skin-text-muted);
}

.polish-diff-stats-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: var(--text-small);
  color: var(--skin-text-muted);
  margin-top: 2px;
}

.polish-diff-stats-row strong {
  color: var(--skin-text);
}

.char-delta.addition {
  color: var(--skin-success);
}

.char-delta.reduction {
  color: var(--skin-accent);
}

.char-delta.neutral {
  color: var(--skin-text-muted);
}

.line-stats {
  display: inline-flex;
  gap: 6px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  margin-left: 4px;
}

.stats-add {
  color: var(--skin-success);
}

.stats-remove {
  color: var(--skin-danger);
}

.polish-diff-toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
}

.diff-view-mode-toggle {
  display: inline-flex;
  border: 1px solid var(--skin-border);
  border-radius: 6px;
  padding: 2px;
  background: var(--skin-canvas);
  gap: 2px;
}

.mode-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 8px;
  border: none;
  background: transparent;
  color: var(--skin-text-muted);
  font-size: var(--text-small);
  border-radius: 4px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.mode-btn:hover {
  color: var(--skin-text);
}

.mode-btn.is-active {
  background: var(--skin-surface);
  color: var(--skin-accent);
  font-weight: 500;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.08);
}

.polish-diff-close-btn {
  border: none;
  background: transparent;
  color: var(--skin-text-muted);
  cursor: pointer;
  padding: 6px;
  border-radius: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition:
    background 0.15s,
    color 0.15s;
}

.polish-diff-close-btn:hover {
  background: var(--skin-subtle);
  color: var(--skin-text);
}

/* 主内容区 */
.polish-diff-body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  background: var(--skin-canvas);
  font-family: var(--font-mono, monospace);
  font-size: calc(12 * var(--font-unit));
  line-height: 1.6;
}

.polish-diff-identical {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  padding: 40px;
  color: var(--skin-text-muted);
  font-size: var(--text-body);
}

/* 并排模式 */
.polish-diff-split-view {
  display: flex;
  flex-direction: column;
  min-height: 100%;
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
  font-size: var(--text-small);
  font-weight: 500;
  color: var(--skin-text-muted);
}

.split-col-header {
  padding: 6px 14px;
}

.split-col-header:first-child {
  border-right: 1px solid var(--skin-border);
}

.split-view-content {
  flex: 1;
}

.split-diff-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
}

.split-side {
  display: flex;
  align-items: stretch;
  min-width: 0;
  padding: 0 4px;
  white-space: pre-wrap;
  word-break: break-all;
}

.split-side.left {
  border-right: 1px solid var(--skin-border);
}

.split-side.diff-context {
  background: transparent;
  color: var(--skin-text-body);
}

.split-side.diff-remove {
  background: var(--skin-danger-soft, rgba(239, 68, 68, 0.12));
  color: var(--skin-danger);
}

.split-side.diff-add {
  background: var(--skin-success-soft, rgba(34, 197, 94, 0.12));
  color: var(--skin-success);
}

.split-side.diff-empty {
  background: var(--skin-subtle, rgba(0, 0, 0, 0.03));
}

.line-num {
  width: 38px;
  padding: 2px 6px;
  text-align: right;
  color: var(--skin-text-muted);
  user-select: none;
  flex-shrink: 0;
  opacity: 0.65;
}

.line-marker {
  width: 14px;
  text-align: center;
  user-select: none;
  flex-shrink: 0;
  font-weight: bold;
}

.line-text {
  flex: 1;
  padding: 2px 4px;
  min-width: 0;
}

/* 逐行模式 */
.polish-diff-unified-view {
  min-height: 100%;
}

.unified-diff-row {
  display: flex;
  align-items: stretch;
  padding: 0 4px;
  white-space: pre-wrap;
  word-break: break-all;
}

.unified-diff-row.diff-context {
  color: var(--skin-text-body);
}

.unified-diff-row.diff-add {
  background: var(--skin-success-soft, rgba(34, 197, 94, 0.12));
  color: var(--skin-success);
}

.unified-diff-row.diff-remove {
  background: var(--skin-danger-soft, rgba(239, 68, 68, 0.12));
  color: var(--skin-danger);
}

.line-num.old,
.line-num.new {
  width: 34px;
  padding: 2px 4px;
  text-align: right;
  color: var(--skin-text-muted);
  user-select: none;
  flex-shrink: 0;
  opacity: 0.65;
}

/* 底栏 */
.polish-diff-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 12px 18px;
  background: var(--skin-surface);
  border-top: 1px solid var(--skin-border);
  flex-shrink: 0;
}

.footer-left {
  display: flex;
  align-items: center;
}

.btn-copy {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  background: transparent;
  border: 1px solid var(--skin-border);
  color: var(--skin-text-muted);
  font-size: var(--text-small);
  padding: 5px 12px;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.btn-copy:hover {
  background: var(--skin-subtle);
  color: var(--skin-text);
  border-color: var(--skin-text-muted);
}

.footer-right {
  display: flex;
  align-items: center;
  gap: 10px;
}

.dialog-button {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 7px 16px;
  border-radius: 6px;
  font-size: var(--text-small);
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
  user-select: none;
}

.dialog-button-secondary {
  background: transparent;
  border: 1px solid var(--skin-border);
  color: var(--skin-text-body);
}

.dialog-button-secondary:hover {
  background: var(--skin-subtle);
  border-color: var(--skin-border-strong);
}

.dialog-button-primary {
  background: var(--skin-accent);
  border: 1px solid var(--skin-accent);
  color: var(--skin-on-accent, #ffffff);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);
}

.dialog-button-primary:hover {
  opacity: 0.92;
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.22);
}
</style>
