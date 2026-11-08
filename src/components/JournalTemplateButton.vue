<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, useId, watch, type CSSProperties } from 'vue'
import {
  PhArticle,
  PhArrowDown,
  PhArrowsClockwise,
  PhCaretDown,
  PhCaretUp,
  PhCheck,
  PhGearSix,
} from '@phosphor-icons/vue'
import type { FileFormat } from '../lib/workspace'
import { selectMenuPosition } from '../lib/select'
import {
  JOURNAL_TEMPLATE_PRESETS,
  getJournalTemplateContent,
  type JournalTemplateId,
  type JournalTemplatePreset,
} from '../lib/journalTemplates'
import type { EditorSettings } from '../lib/settingsDraft'

const props = defineProps<{
  disabled: boolean
  hasContent: boolean
  currentFormat: FileFormat
  date: string
  editorSettings: EditorSettings
}>()

const emit = defineEmits<{
  apply: [content: string, mode: 'replace' | 'append']
  openSettings: []
}>()

const opened = ref(false)
const trigger = ref<HTMLButtonElement>()
const popover = ref<HTMLElement>()
const menuId = useId()
const style = ref<CSSProperties>({ position: 'fixed', visibility: 'hidden' })
const expandedPreviews = ref<Record<string, boolean>>({})

interface DisplayTemplate {
  id: JournalTemplateId
  name: string
  description: string
  content: string
}

const displayTemplates = computed<DisplayTemplate[]>(() => {
  const list: DisplayTemplate[] = JOURNAL_TEMPLATE_PRESETS.map((preset: JournalTemplatePreset) => ({
    id: preset.id,
    name: preset.name,
    description: preset.description,
    content: getJournalTemplateContent(preset.id, props.currentFormat, '', props.date),
  }))

  if (props.editorSettings.customTemplate.trim()) {
    list.push({
      id: 'custom',
      name: '自定义模板',
      description: '你在设置中自定义的常用日报预设骨架。',
      content: getJournalTemplateContent(
        'custom',
        props.currentFormat,
        props.editorSettings.customTemplate,
        props.date,
      ),
    })
  }

  return list
})

function close(restoreFocus = false) {
  opened.value = false
  if (restoreFocus) trigger.value?.focus({ preventScroll: true })
}

function position() {
  if (!opened.value || !trigger.value || !popover.value) return
  const rect = trigger.value.getBoundingClientRect()
  const viewport = { width: document.documentElement.clientWidth || window.innerWidth, height: window.innerHeight }
  const anchor = { top: rect.top, bottom: rect.bottom, left: Math.max(12, rect.right - 340), width: 340 }
  popover.value.style.width = `${selectMenuPosition(anchor, viewport, 0).width}px`
  const result = selectMenuPosition(anchor, viewport, popover.value.scrollHeight)
  style.value = {
    position: 'fixed',
    visibility: 'visible',
    left: `${result.left}px`,
    top: `${result.top}px`,
    width: `${result.width}px`,
    maxHeight: `${result.maxHeight}px`,
  }
}

async function toggle() {
  if (props.disabled) return
  if (opened.value) {
    close(true)
    return
  }
  style.value = { position: 'fixed', visibility: 'hidden' }
  opened.value = true
  await nextTick()
  if (!opened.value) return
  position()
}

function togglePreview(id: string) {
  expandedPreviews.value[id] = !expandedPreviews.value[id]
  void nextTick(position)
}

function applyTemplate(item: DisplayTemplate, mode: 'replace' | 'append') {
  if (props.disabled) return
  close(true)
  emit('apply', item.content, mode)
}

function handleOpenSettings() {
  close()
  emit('openSettings')
}

function onOutside(event: Event) {
  const target = event.target
  if (target instanceof Node && !trigger.value?.contains(target) && !popover.value?.contains(target)) {
    close()
  }
}

function onScroll(event: Event) {
  if (!(event.target instanceof Node && popover.value?.contains(event.target))) {
    close()
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && !event.isComposing) {
    event.preventDefault()
    event.stopPropagation()
    close(true)
  }
}

watch(opened, (value, _previous, onCleanup) => {
  if (!value) return
  document.addEventListener('pointerdown', onOutside)
  document.addEventListener('focusin', onOutside)
  window.addEventListener('resize', position)
  window.addEventListener('scroll', onScroll, true)
  window.addEventListener('keydown', onKeydown)
  onCleanup(() => {
    document.removeEventListener('pointerdown', onOutside)
    document.removeEventListener('focusin', onOutside)
    window.removeEventListener('resize', position)
    window.removeEventListener('scroll', onScroll, true)
    window.removeEventListener('keydown', onKeydown)
  })
})

onBeforeUnmount(() => {
  close()
})
</script>

<template>
  <button
    ref="trigger"
    type="button"
    class="editor-tool-button journal-template-trigger"
    :class="{ 'is-active': opened }"
    aria-label="选择日报预设模板"
    title="日报模板与预设骨架"
    :aria-haspopup="'dialog'"
    :aria-expanded="opened"
    :aria-controls="opened ? menuId : undefined"
    :disabled="disabled"
    @click="toggle"
  >
    <PhArticle :size="15" aria-hidden="true" />
  </button>

  <Teleport to="body">
    <Transition name="select-popover">
      <div
        v-if="opened"
        :id="menuId"
        ref="popover"
        class="journal-template-popover"
        role="dialog"
        aria-label="常用日报模板"
        :style="style"
      >
        <header class="template-popover-header">
          <div class="template-popover-title">
            <PhArticle :size="16" aria-hidden="true" />
            <strong>常用预设模板</strong>
          </div>
          <button type="button" class="template-settings-shortcut" title="配置默认预设骨架" @click="handleOpenSettings">
            <PhGearSix :size="13" aria-hidden="true" />
            <span>设置</span>
          </button>
        </header>

        <div class="template-popover-body">
          <div v-for="item in displayTemplates" :key="item.id" class="template-card">
            <div class="template-card-header">
              <span class="template-card-title">
                <strong>{{ item.name }}</strong>
                <span
                  v-if="editorSettings.defaultTemplateId === item.id"
                  class="template-default-badge"
                  title="当前默认骨架"
                  >默认</span
                >
              </span>
              <button
                type="button"
                class="template-preview-toggle"
                :aria-label="expandedPreviews[item.id] ? '收起预览' : '展开预览'"
                :title="expandedPreviews[item.id] ? '收起预览' : '展开预览'"
                @click="togglePreview(item.id)"
              >
                <span>预览</span>
                <PhCaretUp v-if="expandedPreviews[item.id]" :size="12" aria-hidden="true" />
                <PhCaretDown v-else :size="12" aria-hidden="true" />
              </button>
            </div>
            <p class="template-card-desc">{{ item.description }}</p>

            <pre v-if="expandedPreviews[item.id]" class="template-card-preview"><code>{{ item.content }}</code></pre>

            <div class="template-card-actions">
              <template v-if="!hasContent">
                <button type="button" class="template-action-btn primary" @click="applyTemplate(item, 'replace')">
                  <PhCheck :size="13" aria-hidden="true" />
                  <span>应用此模板</span>
                </button>
              </template>
              <template v-else>
                <button
                  type="button"
                  class="template-action-btn secondary"
                  title="保留当前内容，并在末尾追加此模板"
                  @click="applyTemplate(item, 'append')"
                >
                  <PhArrowDown :size="13" aria-hidden="true" />
                  <span>追加到末尾</span>
                </button>
                <button
                  type="button"
                  class="template-action-btn warning"
                  title="清空当前日报并应用此模板（可撤销）"
                  @click="applyTemplate(item, 'replace')"
                >
                  <PhArrowsClockwise :size="13" aria-hidden="true" />
                  <span>替换全文</span>
                </button>
              </template>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.journal-template-popover {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--skin-border);
  border-radius: 12px;
  background: var(--skin-surface);
  color: var(--skin-text);
  box-shadow: 0 12px 36px #0000002b;
  overflow: hidden;
  z-index: 1000;
}

.template-popover-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px 8px;
  border-bottom: 1px solid var(--skin-border);
  background: var(--skin-canvas);
}

.template-popover-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-body);
  color: var(--skin-text);
}

.template-settings-shortcut {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 7px;
  border: 1px solid var(--skin-border);
  border-radius: 6px;
  background: var(--skin-surface);
  color: var(--skin-text-muted);
  font: inherit;
  font-size: 11px;
  cursor: pointer;
  transition:
    background-color 120ms ease,
    color 120ms ease;
}

.template-settings-shortcut:hover {
  background: var(--skin-accent-soft);
  color: var(--skin-text);
}

.template-popover-body {
  padding: 8px;
  display: grid;
  gap: 8px;
  overflow-y: auto;
  max-height: 380px;
}

.template-card {
  padding: 9px 11px;
  border: 1px solid var(--skin-border);
  border-radius: 9px;
  background: var(--skin-canvas);
  display: grid;
  gap: 5px;
}

.template-card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.template-card-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-body);
}

.template-default-badge {
  padding: 1px 5px;
  border-radius: 4px;
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
  font-size: 10px;
  font-weight: 600;
}

.template-preview-toggle {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--skin-text-muted);
  font: inherit;
  font-size: 11px;
  cursor: pointer;
}

.template-preview-toggle:hover {
  color: var(--skin-accent);
}

.template-card-desc {
  margin: 0;
  font-size: var(--text-small);
  color: var(--skin-text-muted);
  line-height: 1.4;
}

.template-card-preview {
  margin: 4px 0 0;
  padding: 6px 8px;
  border-radius: 6px;
  background: var(--skin-surface);
  color: var(--skin-text-secondary);
  font-family: var(--font-code, monospace);
  font-size: 11px;
  line-height: 1.45;
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 120px;
  overflow-y: auto;
  border: 1px dashed var(--skin-border);
}

.template-card-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 4px;
}

.template-action-btn {
  flex: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  padding: 4px 10px;
  border: 1px solid var(--skin-border);
  border-radius: 6px;
  font: inherit;
  font-size: var(--text-small);
  cursor: pointer;
  transition:
    background-color 120ms ease,
    border-color 120ms ease,
    color 120ms ease;
}

.template-action-btn.primary {
  background: var(--skin-accent-soft);
  border-color: var(--skin-accent);
  color: var(--skin-accent);
  font-weight: 500;
}

.template-action-btn.primary:hover {
  background: var(--skin-accent);
  color: var(--skin-on-accent, #ffffff);
}

.template-action-btn.secondary {
  background: var(--skin-surface);
  color: var(--skin-text);
}

.template-action-btn.secondary:hover {
  border-color: var(--skin-accent);
  color: var(--skin-accent);
}

.template-action-btn.warning {
  background: var(--skin-surface);
  color: var(--skin-text-muted);
}

.template-action-btn.warning:hover {
  border-color: var(--skin-danger, #d32f2f);
  color: var(--skin-danger, #d32f2f);
}
</style>
