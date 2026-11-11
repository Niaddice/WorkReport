<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, useId, watch, type CSSProperties } from 'vue'
import { PhCaretDown, PhCaretUp, PhCircleNotch, PhGitBranch, PhSparkle, PhX } from '@phosphor-icons/vue'
import { selectMenuPosition } from '../lib/select'

const props = defineProps<{
  anchor: HTMLElement | null | undefined
  opened: boolean
  date: string
  hasContent: boolean
  generating?: boolean
}>()

const emit = defineEmits<{
  close: []
  confirm: [options: { mode: 'replace' | 'append'; customPrompt?: string }]
  cancelGeneration: []
}>()

const popoverRef = ref<HTMLDivElement>()
const popoverId = useId()
const style = ref<CSSProperties>({ position: 'fixed', visibility: 'hidden' })
const placement = ref<'top' | 'bottom'>('bottom')

const mode = ref<'append' | 'replace'>('append')
const showCustomPrompt = ref(false)
const customPrompt = ref('')

function formatDate(date: string): string {
  if (date.length === 8) {
    return `${date.slice(0, 4)}年${date.slice(4, 6)}月${date.slice(6, 8)}日`
  }
  return date
}

const displayDate = computed(() => formatDate(props.date))

watch(
  () => props.opened,
  (val) => {
    if (val) {
      mode.value = props.hasContent ? 'append' : 'replace'
      showCustomPrompt.value = false
      customPrompt.value = ''
      style.value = { position: 'fixed', visibility: 'hidden' }
      void nextTick(() => {
        position()
      })
    }
  },
  { immediate: true },
)

function position() {
  if (!props.opened || !props.anchor || !popoverRef.value) return
  const rect = props.anchor.getBoundingClientRect()
  const viewport = {
    width: document.documentElement.clientWidth || window.innerWidth,
    height: window.innerHeight,
  }
  const width = Math.min(340, Math.max(290, viewport.width - 24))
  const left = Math.max(12, Math.min(rect.right - width, viewport.width - width - 12))
  const anchor = {
    top: rect.top,
    bottom: rect.bottom,
    left,
    width,
  }

  const result = selectMenuPosition(anchor, viewport, popoverRef.value.scrollHeight || 300)
  const isTop = result.placement === 'top'
  placement.value = isTop ? 'top' : 'bottom'
  const calculatedTop = isTop ? Math.max(12, rect.top - 6 - (popoverRef.value.scrollHeight || 260)) : rect.bottom + 6

  style.value = {
    position: 'fixed',
    visibility: 'visible',
    left: `${result.left}px`,
    top: `${calculatedTop}px`,
    width: `${result.width}px`,
    maxHeight: `${Math.max(result.maxHeight, 380)}px`,
  }
}

watch(
  () => props.generating,
  (val, prev) => {
    if (prev && !val && props.opened) {
      emit('close')
    }
  },
)

function handleConfirm() {
  if (props.generating) return
  emit('confirm', {
    mode: mode.value,
    customPrompt: customPrompt.value.trim() ? customPrompt.value.trim() : undefined,
  })
  emit('close')
}

function onOutside(event: Event) {
  const target = event.target
  if (target instanceof Node && !props.anchor?.contains(target) && !popoverRef.value?.contains(target)) {
    emit('close')
  }
}

function onScroll(event: Event) {
  if (!(event.target instanceof Node && popoverRef.value?.contains(event.target))) {
    emit('close')
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && !event.isComposing) {
    event.preventDefault()
    emit('close')
  } else if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
    event.preventDefault()
    handleConfirm()
  }
}

watch(
  () => props.opened,
  (val, _, onCleanup) => {
    if (!val) return
    document.addEventListener('pointerdown', onOutside, true)
    document.addEventListener('focusin', onOutside, true)
    window.addEventListener('resize', position)
    window.addEventListener('scroll', onScroll, true)
    window.addEventListener('keydown', onKeydown, true)
    onCleanup(() => {
      document.removeEventListener('pointerdown', onOutside, true)
      document.removeEventListener('focusin', onOutside, true)
      window.removeEventListener('resize', position)
      window.removeEventListener('scroll', onScroll, true)
      window.removeEventListener('keydown', onKeydown, true)
    })
  },
  { immediate: true },
)

onBeforeUnmount(() => {
  emit('close')
})
</script>

<template>
  <Teleport to="body">
    <Transition name="select-popover">
      <div
        v-if="opened"
        :id="popoverId"
        ref="popoverRef"
        class="git-draft-popover"
        :data-placement="placement"
        role="dialog"
        aria-label="从 Git 提交生成日报确认"
        :style="style"
      >
        <header class="popover-header">
          <div class="popover-title-row">
            <span class="popover-icon">
              <PhGitBranch :size="16" aria-hidden="true" />
            </span>
            <strong class="popover-title">从 Git 提交生成日报</strong>
          </div>
          <button
            type="button"
            class="popover-close-btn"
            aria-label="关闭确认框 (Esc)"
            title="关闭 (Esc)"
            @click="emit('close')"
          >
            <PhX :size="14" aria-hidden="true" />
          </button>
        </header>

        <main class="popover-body">
          <div class="git-draft-info-card">
            <div class="info-tag-row">
              <span class="info-badge">{{ displayDate }}</span>
              <span class="info-status">今日提交记录</span>
            </div>
            <p class="info-description">
              将读取当前工作空间配置的 Git 仓库在今日的全部提交，通过 AI 提炼归纳为规范的日报工作进展与明日计划。
            </p>
          </div>

          <!-- 若编辑区已有正文，提供写入模式选择 -->
          <div v-if="hasContent" class="mode-select-wrap">
            <label class="mode-title">写入方式：</label>
            <div class="mode-options" role="radiogroup" aria-label="写入方式">
              <label class="mode-option-label" :class="{ 'is-selected': mode === 'append' }">
                <input v-model="mode" type="radio" value="append" name="git-insert-mode" />
                <div class="mode-option-text">
                  <strong>追加到文末</strong>
                  <span>保留现有内容，在下方新增 Git 归纳内容</span>
                </div>
              </label>
              <label class="mode-option-label" :class="{ 'is-selected': mode === 'replace' }">
                <input v-model="mode" type="radio" value="replace" name="git-insert-mode" />
                <div class="mode-option-text">
                  <strong>完全替换当前内容</strong>
                  <span>以 Git 归纳内容覆盖当前编辑区的全部文字</span>
                </div>
              </label>
            </div>
          </div>

          <!-- 可折叠的自定义提示词 -->
          <div class="custom-prompt-wrap">
            <button
              type="button"
              class="prompt-toggle-btn"
              :aria-expanded="showCustomPrompt"
              @click="showCustomPrompt = !showCustomPrompt"
            >
              <component :is="showCustomPrompt ? PhCaretUp : PhCaretDown" :size="12" aria-hidden="true" />
              <span>自定义提示要求（可选）</span>
            </button>
            <textarea
              v-if="showCustomPrompt"
              v-model="customPrompt"
              rows="3"
              class="custom-prompt-input"
              placeholder="例如：重点突出重构进展与解决的关键缺陷，简要列出待办事项…"
              aria-label="自定义生成提示词"
            />
          </div>
        </main>

        <footer class="popover-footer">
          <div class="footer-status">
            <span v-if="generating" class="generating-indicator">
              <PhCircleNotch class="spin" :size="13" aria-hidden="true" />
              <span>AI 正在生成…</span>
              <button type="button" class="cancel-link" title="中断当前生成" @click="emit('cancelGeneration')">
                取消
              </button>
            </span>
          </div>
          <div class="footer-actions">
            <button type="button" class="popover-btn popover-btn-secondary" @click="emit('close')">取消</button>
            <button type="button" class="popover-btn popover-btn-primary" :disabled="generating" @click="handleConfirm">
              <PhCircleNotch v-if="generating" class="spin" :size="14" aria-hidden="true" />
              <PhSparkle v-else :size="14" aria-hidden="true" />
              <span>{{ generating ? '生成中…' : '立即生成' }}</span>
            </button>
          </div>
        </footer>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.git-draft-popover {
  display: flex;
  flex-direction: column;
  background: var(--skin-surface);
  border: 1px solid var(--skin-border);
  border-radius: 10px;
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.22);
  z-index: 1000;
  overflow: hidden;
  user-select: none;
}

.git-draft-popover[data-placement='top'].select-popover-enter-from,
.git-draft-popover[data-placement='top'].select-popover-leave-to {
  transform: translateY(3px) scale(0.985);
}

.popover-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  background: var(--skin-surface-raised, var(--skin-surface));
  border-bottom: 1px solid var(--skin-border);
}

.popover-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.popover-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border-radius: 6px;
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
}

.popover-title {
  font-size: var(--text-body);
  font-weight: 600;
  color: var(--skin-text);
}

.popover-close-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  padding: 0;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: var(--skin-text-muted);
  cursor: pointer;
  transition: all 120ms ease;
}

.popover-close-btn:hover {
  background: var(--skin-subtle);
  color: var(--skin-text);
}

.popover-body {
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  max-height: 380px;
  overflow-y: auto;
}

.git-draft-info-card {
  padding: 9px 11px;
  border-radius: 8px;
  background: var(--skin-canvas);
  border: 1px solid var(--skin-border);
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.info-tag-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.info-badge {
  font-size: var(--text-caption);
  font-weight: 600;
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
}

.info-status {
  font-size: var(--text-caption);
  color: var(--skin-text-muted);
}

.info-description {
  margin: 0;
  font-size: var(--text-small);
  line-height: 1.5;
  color: var(--skin-text-muted);
}

.mode-select-wrap {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.mode-title {
  font-size: var(--text-small);
  font-weight: 500;
  color: var(--skin-text);
}

.mode-options {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.mode-option-label {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 7px;
  border: 1px solid var(--skin-border);
  background: var(--skin-canvas);
  cursor: pointer;
  transition: all 120ms ease;
}

.mode-option-label:hover {
  background: var(--skin-subtle);
  border-color: var(--skin-border-strong, var(--skin-accent-border));
}

.mode-option-label.is-selected {
  border-color: var(--skin-accent);
  background: var(--skin-accent-soft);
}

.mode-option-label input[type='radio'] {
  margin-top: 2px;
  accent-color: var(--skin-accent);
}

.mode-option-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.mode-option-text strong {
  font-size: var(--text-small);
  color: var(--skin-text);
}

.mode-option-text span {
  font-size: var(--text-caption);
  color: var(--skin-text-muted);
  line-height: 1.35;
}

.custom-prompt-wrap {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.prompt-toggle-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 0;
  border: none;
  background: transparent;
  color: var(--skin-accent);
  font-size: var(--text-caption);
  cursor: pointer;
  align-self: flex-start;
}

.prompt-toggle-btn:hover {
  text-decoration: underline;
}

.custom-prompt-input {
  width: 100%;
  padding: 7px 9px;
  border: 1px solid var(--skin-border);
  border-radius: 6px;
  background: var(--skin-canvas);
  color: var(--skin-text);
  font-family: inherit;
  font-size: var(--text-small);
  line-height: 1.45;
  resize: vertical;
  outline: none;
  box-sizing: border-box;
}

.custom-prompt-input:focus {
  border-color: var(--skin-accent);
}

.popover-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 9px 14px;
  background: var(--skin-surface-raised, var(--skin-surface));
  border-top: 1px solid var(--skin-border);
}

.footer-status {
  display: flex;
  align-items: center;
  font-size: var(--text-caption);
  color: var(--skin-text-muted);
}

.generating-indicator {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: var(--skin-accent);
}

.cancel-link {
  border: none;
  background: transparent;
  color: var(--skin-danger, #dc2626);
  font-size: var(--text-caption);
  cursor: pointer;
  padding: 0 4px;
  text-decoration: underline;
}

.footer-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-left: auto;
}

.popover-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 5px 12px;
  border-radius: 6px;
  font-size: var(--text-small);
  font-weight: 500;
  cursor: pointer;
  transition: all 120ms ease;
  user-select: none;
}

.popover-btn-secondary {
  border: 1px solid var(--skin-border);
  background: transparent;
  color: var(--skin-text);
}

.popover-btn-secondary:hover {
  background: var(--skin-subtle);
  border-color: var(--skin-border-strong, var(--skin-border));
}

.popover-btn-primary {
  border: 1px solid var(--skin-accent);
  background: var(--skin-accent);
  color: var(--skin-on-accent, #ffffff);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12);
}

.popover-btn-primary:hover:not(:disabled) {
  opacity: 0.92;
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.18);
}

.popover-btn-primary:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}
</style>
