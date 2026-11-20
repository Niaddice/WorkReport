<script setup lang="ts">
import { computed, nextTick, ref } from 'vue'
import {
  PhArticle,
  PhArrowsClockwise,
  PhCheck,
  PhCheckSquare,
  PhCopy,
  PhEye,
  PhEyeSlash,
  PhFileDashed,
  PhRocketLaunch,
  PhSlidersHorizontal,
  PhX,
} from '@phosphor-icons/vue'
import DirectorySettings from '../DirectorySettings.vue'
import ModuleSettingsSection from './ModuleSettingsSection.vue'
import SettingHint from '../SettingHint.vue'
import { cloneFileSettings, type FileSettings } from '../../lib/fileSettings'
import { useModuleSettingsContext } from '../../modules/settingsContext'
import { getJournalTemplateContent, interpolateTemplate, type JournalTemplateId } from '../../lib/journalTemplates'
import { isTauri } from '@tauri-apps/api/core'
import { api } from '../../lib/workspace'

/**
 * 编辑模块设置：新日报默认格式、保存位置与文件名、每日预设骨架、AI 润色提示词。
 *
 * 所有修改都直接写入弹窗提供的设置草稿，由弹窗外壳统一保存。
 */
const context = useModuleSettingsContext()
const { draft, busy, workspace } = context
const openGlobalSettings = context.openGlobalSettings
const directoryForm = ref<InstanceType<typeof DirectorySettings>>()
const promptInput = ref<HTMLTextAreaElement>()
const customTemplateInput = ref<HTMLTextAreaElement>()

const SKELETON_ICONS: Record<string, any> = {
  none: PhFileDashed,
  standard: PhArticle,
  agile: PhArrowsClockwise,
  project: PhRocketLaunch,
  minimal: PhCheckSquare,
  custom: PhSlidersHorizontal,
}

const skeletonOptions: Array<{ id: JournalTemplateId; name: string; desc: string }> = [
  { id: 'none', name: '空白', desc: '不使用预设骨架，保持全新空白编辑区。' },
  { id: 'standard', name: '标准工作日报', desc: '今日产出、明日计划与思考复盘。' },
  { id: 'agile', name: '敏捷站会三问', desc: '昨日回顾、今日要务与阻碍风险。' },
  { id: 'project', name: '项目重点推进', desc: '核心里程碑、任务清单与技术细节。' },
  { id: 'minimal', name: '极简打卡清单', desc: '今日事项打卡与备忘备注。' },
  { id: 'custom', name: '自定义模板', desc: '使用你自定义的结构与占位符。' },
]

const PLACEHOLDER_CHIPS = [
  { tag: '{date}', label: '当前日期', desc: 'YYYY-MM-DD' },
  { tag: '{weekday}', label: '星期几', desc: '如 星期日' },
  { tag: '{year}', label: '年份', desc: '如 2026' },
  { tag: '{month}', label: '月份', desc: '如 11' },
  { tag: '{day}', label: '日', desc: '如 08' },
  { tag: '{rawdate}', label: '紧凑日期', desc: 'YYYYMMDD' },
]

const activePreviewId = ref<string | null>(null)
const copied = ref(false)
let copyTimer: ReturnType<typeof setTimeout> | undefined
const showCustomLivePreview = ref(false)

function hasPreviewContent(id: string) {
  return id !== 'none'
}

function togglePreview(id: string) {
  if (activePreviewId.value === id) {
    activePreviewId.value = null
  } else {
    activePreviewId.value = id
    copied.value = false
  }
}

function getPresetName(id: string) {
  return skeletonOptions.find((opt) => opt.id === id)?.name ?? ''
}

const activePreviewContent = computed(() => {
  if (!activePreviewId.value || activePreviewId.value === 'none') return ''
  if (activePreviewId.value === 'custom') {
    return interpolateTemplate(draft.value.editor.customTemplate.trim()) || '（尚未编写自定义模板内容）'
  }
  return getJournalTemplateContent(activePreviewId.value, draft.value.database.defaultFormat)
})

async function copyPreviewContent() {
  if (!activePreviewContent.value) return
  try {
    if (isTauri()) await api.copy(activePreviewContent.value)
    else await navigator.clipboard?.writeText(activePreviewContent.value)
    copied.value = true
    clearTimeout(copyTimer)
    copyTimer = setTimeout(() => {
      copied.value = false
    }, 2000)
  } catch {
    /* Safe ignore clipboard failure */
  }
}

function insertPlaceholder(tag: string) {
  const el = customTemplateInput.value
  if (!el) {
    draft.value.editor.customTemplate += tag
    return
  }
  const start = el.selectionStart ?? el.value.length
  const end = el.selectionEnd ?? el.value.length
  const current = draft.value.editor.customTemplate
  draft.value.editor.customTemplate = current.slice(0, start) + tag + current.slice(end)
  void nextTick(() => {
    el.focus()
    const newPos = start + tag.length
    el.setSelectionRange(newPos, newPos)
  })
}

const customLivePreviewContent = computed(() => {
  return interpolateTemplate(draft.value.editor.customTemplate.trim()) || '（请输入自定义模板内容后预览）'
})

const files = computed<FileSettings>({
  get: () => cloneFileSettings(draft.value.files, draft.value.summary.outputDirectory),
  set: (value) => {
    draft.value.files = value
  },
})
const defaultProvider = computed(() =>
  draft.value.ai.providers.find((provider) => provider.id === draft.value.ai.defaultProviderId),
)
const promptError = computed(() =>
  draft.value.ai.generationPreference.length > 2000 ? '日报润色提示词不能超过 2000 字。' : '',
)
const templateError = computed(() =>
  draft.value.editor.customTemplate.length > 10000 ? '自定义模板内容不能超过 10,000 字。' : '',
)

function validate() {
  if (promptError.value || templateError.value) return false
  return directoryForm.value?.validate() ?? true
}
function focusInvalid() {
  if (templateError.value) {
    customTemplateInput.value?.focus()
    return
  }
  if (promptError.value) {
    promptInput.value?.focus()
    return
  }
  directoryForm.value?.focusInvalid()
}
defineExpose({ validate, focusInvalid })
</script>

<template>
  <ModuleSettingsSection
    title="新日报默认格式"
    heading-id="editor-format-title"
    description="用于之后新建的日报，已有日报可在编辑区单独切换。"
  >
    <div class="database-field">
      <div class="database-mode-options" role="radiogroup" aria-labelledby="editor-format-title">
        <label :class="{ selected: draft.database.defaultFormat === 'md' }" class="database-mode-label">
          <input
            type="radio"
            name="default-entry-format"
            value="md"
            :checked="draft.database.defaultFormat === 'md'"
            :disabled="busy"
            @change="draft.database.defaultFormat = 'md'"
          /><span class="database-mode-copy"
            ><strong>Markdown（.md）</strong><span>支持标题、列表与加粗，适合结构化日报。</span></span
          >
        </label>
        <label :class="{ selected: draft.database.defaultFormat === 'txt' }" class="database-mode-label">
          <input
            type="radio"
            name="default-entry-format"
            value="txt"
            :checked="draft.database.defaultFormat === 'txt'"
            :disabled="busy"
            @change="draft.database.defaultFormat = 'txt'"
          /><span class="database-mode-copy"
            ><strong>纯文本（.txt）</strong><span>只保留纯文本，适合直接粘贴或打印。</span></span
          >
        </label>
      </div>
    </div>
  </ModuleSettingsSection>

  <ModuleSettingsSection
    title="保存位置与文件名"
    heading-id="editor-files-title"
    description="管理日报的保存目录和日期命名规则。"
  >
    <DirectorySettings
      ref="directoryForm"
      :model-value="files"
      :busy="busy"
      :features="['journal']"
      compact
      :workspace-path="workspace?.path ?? ''"
      :journal-format="draft.database.defaultFormat"
      @update:model-value="files = $event"
    />
  </ModuleSettingsSection>

  <ModuleSettingsSection
    title="每日预设骨架"
    heading-id="editor-skeleton-title"
    description="配置新建空白日报时使用的预设骨架。在编辑区也可随时通过工具栏模板按钮插入。"
  >
    <div class="skeleton-settings-container">
      <div class="template-preset-grid" role="radiogroup" aria-labelledby="editor-skeleton-title">
        <label
          v-for="item in skeletonOptions"
          :key="item.id"
          :class="{
            selected: draft.editor.defaultTemplateId === item.id,
            'is-custom': item.id === 'custom',
            'is-blank': item.id === 'none',
            'has-active-preview': activePreviewId === item.id,
          }"
          class="skeleton-card"
        >
          <div class="skeleton-card-top">
            <span class="skeleton-card-icon" :class="`icon-${item.id}`">
              <component :is="SKELETON_ICONS[item.id]" :size="16" aria-hidden="true" />
            </span>
            <div class="skeleton-card-title-group">
              <span class="skeleton-card-name">{{ item.name }}</span>
              <span v-if="draft.editor.defaultTemplateId === item.id" class="skeleton-active-pill"> 默认 </span>
            </div>
            <input
              type="radio"
              name="editor-default-template"
              :value="item.id"
              :checked="draft.editor.defaultTemplateId === item.id"
              :disabled="busy"
              class="skeleton-radio"
              @change="draft.editor.defaultTemplateId = item.id"
            />
          </div>

          <p class="skeleton-card-desc">{{ item.desc }}</p>

          <div v-if="hasPreviewContent(item.id)" class="skeleton-card-footer">
            <button
              type="button"
              class="skeleton-preview-toggle-btn"
              :class="{ 'is-active': activePreviewId === item.id }"
              :title="activePreviewId === item.id ? '收起骨架预览' : '展开骨架结构预览'"
              @click.prevent.stop="togglePreview(item.id)"
            >
              <PhEye v-if="activePreviewId !== item.id" :size="12" aria-hidden="true" />
              <PhEyeSlash v-else :size="12" aria-hidden="true" />
              <span>{{ activePreviewId === item.id ? '收起预览' : '预览结构' }}</span>
            </button>
          </div>
        </label>
      </div>

      <!-- 预设骨架结构预览抽屉面板 -->
      <Transition name="settings-reveal">
        <div v-if="activePreviewId && activePreviewContent" class="skeleton-preview-drawer">
          <div class="skeleton-preview-header">
            <div class="skeleton-preview-title">
              <component :is="SKELETON_ICONS[activePreviewId]" :size="14" aria-hidden="true" />
              <span>{{ getPresetName(activePreviewId) }} · 结构预览</span>
              <span class="skeleton-preview-format-tag">{{
                draft.database.defaultFormat === 'txt' ? '纯文本' : 'Markdown'
              }}</span>
            </div>
            <div class="skeleton-preview-actions">
              <button
                type="button"
                class="skeleton-preview-copy-btn"
                :title="copied ? '已复制到剪贴板' : '复制骨架文本'"
                @click="copyPreviewContent"
              >
                <PhCheck v-if="copied" :size="12" aria-hidden="true" />
                <PhCopy v-else :size="12" aria-hidden="true" />
                <span>{{ copied ? '已复制' : '复制骨架' }}</span>
              </button>
              <button
                type="button"
                class="skeleton-preview-close-btn"
                title="关闭预览"
                aria-label="关闭骨架预览"
                @click="activePreviewId = null"
              >
                <PhX :size="12" aria-hidden="true" />
              </button>
            </div>
          </div>
          <pre class="skeleton-preview-code"><code>{{ activePreviewContent }}</code></pre>
        </div>
      </Transition>

      <!-- 自定义模板编辑区域 -->
      <Transition name="settings-reveal">
        <div v-if="draft.editor.defaultTemplateId === 'custom'" class="settings-reveal-shell">
          <div class="settings-reveal-inner">
            <div class="generation-preference custom-template-editor">
              <div class="setting-field-header">
                <label for="custom-template-input">自定义预设骨架</label>
                <SettingHint text="支持变量占位符插值，切换或创建新日期时将自动替换为具体日期与星期。" />
              </div>

              <!-- 快捷变量药丸插入栏 -->
              <div class="custom-template-chips">
                <span class="custom-template-chips-label">快捷插入变量：</span>
                <button
                  v-for="chip in PLACEHOLDER_CHIPS"
                  :key="chip.tag"
                  type="button"
                  class="placeholder-chip"
                  :title="`插入 ${chip.tag}（${chip.desc}）`"
                  :disabled="busy"
                  @click="insertPlaceholder(chip.tag)"
                >
                  <code>{{ chip.tag }}</code>
                  <span>{{ chip.label }}</span>
                </button>
              </div>

              <textarea
                id="custom-template-input"
                ref="customTemplateInput"
                v-model="draft.editor.customTemplate"
                :disabled="busy"
                maxlength="10000"
                rows="6"
                placeholder="### 📌 {date} {weekday} 工作日报&#10;&#10;1. "
                spellcheck="false"
              ></textarea>

              <div class="custom-template-meta">
                <button
                  type="button"
                  class="custom-preview-toggle-btn"
                  :class="{ 'is-active': showCustomLivePreview }"
                  @click="showCustomLivePreview = !showCustomLivePreview"
                >
                  <PhEye v-if="!showCustomLivePreview" :size="12" aria-hidden="true" />
                  <PhEyeSlash v-else :size="12" aria-hidden="true" />
                  <span>{{ showCustomLivePreview ? '收起实时预览' : '实时渲染预览' }}</span>
                </button>
                <span class="preference-count">{{ draft.editor.customTemplate.length }} / 10000</span>
              </div>

              <div v-if="showCustomLivePreview" class="custom-live-preview-box">
                <div class="custom-live-preview-header">
                  <span
                    >今日渲染效果预览（{{ draft.database.defaultFormat === 'txt' ? '纯文本' : 'Markdown' }}）：</span
                  >
                </div>
                <pre class="skeleton-preview-code"><code>{{ customLivePreviewContent }}</code></pre>
              </div>
            </div>
          </div>
        </div>
      </Transition>
      <p v-if="templateError" class="settings-error" role="alert">{{ templateError }}</p>

      <!-- 自动载入开关卡片 -->
      <div
        class="skeleton-auto-apply-card"
        :class="{ 'is-disabled': busy || draft.editor.defaultTemplateId === 'none' }"
      >
        <div class="skeleton-auto-apply-info">
          <div class="skeleton-auto-apply-heading">
            <strong>新建或切换空白日报时自动载入此骨架</strong>
            <span v-if="draft.editor.defaultTemplateId === 'none'" class="skeleton-disabled-pill">
              当前为空白，已停用
            </span>
            <span v-else-if="draft.editor.autoApplyTemplate" class="skeleton-enabled-pill"> 自动载入中 </span>
          </div>
          <p class="skeleton-auto-apply-desc">
            打开未创建或无内容的日期时会自动填入所选骨架；在未手动编辑前不会向磁盘写入任何文件。
          </p>
        </div>
        <label
          class="skeleton-switch"
          :title="draft.editor.defaultTemplateId === 'none' ? '选择非空白预设骨架后方可开启' : undefined"
        >
          <input
            v-model="draft.editor.autoApplyTemplate"
            type="checkbox"
            :disabled="busy || draft.editor.defaultTemplateId === 'none'"
            class="skeleton-switch-input"
          />
          <span class="skeleton-switch-track">
            <span class="skeleton-switch-thumb"></span>
          </span>
        </label>
      </div>
    </div>
  </ModuleSettingsSection>

  <ModuleSettingsSection
    title="日报润色"
    heading-id="editor-prompt-title"
    description="自定义 AI 润色方式，留空时使用默认规则。"
  >
    <div class="generation-preference">
      <div class="setting-field-header">
        <label for="polish-prompt">日报润色提示词</label>
        <SettingHint text="追加在固定的事实、安全和格式规则之后；留空时使用默认润色规则。" />
      </div>
      <textarea
        id="polish-prompt"
        ref="promptInput"
        v-model="draft.ai.generationPreference"
        :disabled="busy"
        maxlength="2000"
        rows="4"
        placeholder="例如：简洁客观，突出已完成事项和下一步计划，保留原有条目结构。"
        spellcheck="false"
      ></textarea>
      <span class="preference-count">{{ draft.ai.generationPreference.length }} / 2000</span>
    </div>
    <p v-if="promptError" class="settings-error" role="alert">{{ promptError }}</p>
    <div class="module-settings-ai-status" :class="{ 'is-ready': !!defaultProvider }">
      <span>{{ defaultProvider ? `使用默认 AI：${defaultProvider.name || '未命名 AI'}` : '尚未配置默认 AI' }}</span>
      <button
        v-if="!defaultProvider"
        type="button"
        class="text-action"
        :disabled="busy"
        @click="openGlobalSettings('ai')"
      >
        前往模型配置
      </button>
    </div>
  </ModuleSettingsSection>
</template>

<style scoped>
.skeleton-settings-container {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.template-preset-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
  gap: 10px;
}

.skeleton-card {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 14px;
  border: 1px solid var(--skin-border);
  border-radius: 9px;
  background: var(--skin-surface);
  cursor: pointer;
  min-width: 0;
  transition:
    background-color var(--motion-fast, 140ms) ease,
    border-color var(--motion-fast, 140ms) ease,
    box-shadow var(--motion-fast, 140ms) ease,
    transform var(--motion-fast, 140ms) ease;
  user-select: none;
}

.skeleton-card:hover:not(:has(input:disabled)) {
  border-color: var(--control-border-hover);
  background: var(--skin-surface);
  transform: translateY(-1.5px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.04);
}

.skeleton-card.selected {
  border-color: var(--skin-accent);
  background: var(--skin-accent-soft);
  box-shadow: 0 2px 8px color-mix(in srgb, var(--skin-accent) 15%, transparent);
}

.skeleton-card:has(input:focus-visible) {
  outline: 2px solid var(--control-focus);
  outline-offset: 2px;
}

.skeleton-card:has(input:disabled) {
  cursor: not-allowed;
  opacity: 0.65;
}

.skeleton-card-top {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.skeleton-card-icon {
  display: grid;
  place-items: center;
  flex-shrink: 0;
  width: 26px;
  height: 26px;
  border-radius: 7px;
  background: var(--skin-canvas);
  color: var(--skin-text-muted);
  transition:
    background-color var(--motion-fast, 140ms) ease,
    color var(--motion-fast, 140ms) ease;
}

.skeleton-card.selected .skeleton-card-icon {
  background: color-mix(in srgb, var(--skin-accent) 18%, transparent);
  color: var(--skin-accent);
}

.skeleton-card-title-group {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 6px;
}

.skeleton-card-name {
  font-size: var(--text-body);
  font-weight: 600;
  color: var(--skin-text-body);
  line-height: 1.4;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.skeleton-active-pill {
  flex-shrink: 0;
  padding: 1px 6px;
  border-radius: 999px;
  background: var(--skin-accent);
  color: var(--skin-on-accent, #ffffff);
  font-size: var(--text-micro, 10.5px);
  font-weight: 600;
  line-height: 1.4;
}

.skeleton-radio {
  flex-shrink: 0;
  width: 16px;
  height: 16px;
  margin: 0;
  accent-color: var(--skin-accent);
  cursor: pointer;
}

.skeleton-card-desc {
  margin: 0;
  font-size: var(--text-small);
  color: var(--skin-text-muted);
  line-height: 1.5;
  flex-grow: 1;
}

.skeleton-card-footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  margin-top: 2px;
  padding-top: 6px;
  border-top: 1px dashed color-mix(in srgb, var(--skin-border) 80%, transparent);
}

.skeleton-preview-toggle-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 7px;
  border: 1px solid transparent;
  border-radius: 5px;
  background: transparent;
  color: var(--skin-text-muted);
  font: inherit;
  font-size: var(--text-caption);
  font-weight: 500;
  cursor: pointer;
  transition: all var(--motion-fast, 140ms) ease;
}

.skeleton-preview-toggle-btn:hover {
  color: var(--skin-accent);
  background: color-mix(in srgb, var(--skin-accent) 12%, transparent);
  border-color: var(--skin-accent-border, transparent);
}

.skeleton-preview-toggle-btn.is-active {
  color: var(--skin-accent);
  font-weight: 600;
  background: color-mix(in srgb, var(--skin-accent) 16%, transparent);
}

/* 预览抽屉卡片 */
.skeleton-preview-drawer {
  border: 1px solid var(--skin-border);
  border-radius: 8px;
  background: var(--skin-canvas);
  overflow: hidden;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.03);
}

.skeleton-preview-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--skin-border);
  background: var(--skin-surface);
}

.skeleton-preview-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-small);
  font-weight: 600;
  color: var(--skin-text-body);
}

.skeleton-preview-format-tag {
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
  font-size: var(--text-micro, 10.5px);
  font-weight: 500;
}

.skeleton-preview-actions {
  display: flex;
  align-items: center;
  gap: 6px;
}

.skeleton-preview-copy-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 8px;
  border: 1px solid var(--skin-border);
  border-radius: 5px;
  background: var(--skin-surface);
  color: var(--skin-text-secondary);
  font: inherit;
  font-size: var(--text-caption);
  cursor: pointer;
  transition: all var(--motion-fast, 140ms) ease;
}

.skeleton-preview-copy-btn:hover {
  border-color: var(--skin-accent);
  color: var(--skin-accent);
  background: var(--skin-accent-soft);
}

.skeleton-preview-close-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  padding: 0;
  border: 0;
  border-radius: 5px;
  background: transparent;
  color: var(--skin-text-muted);
  cursor: pointer;
  transition: all var(--motion-fast, 140ms) ease;
}

.skeleton-preview-close-btn:hover {
  background: var(--skin-canvas);
  color: var(--skin-text);
}

.skeleton-preview-code {
  margin: 0;
  padding: 10px 12px;
  font-family: var(--font-code, monospace);
  font-size: var(--text-small);
  line-height: 1.6;
  color: var(--skin-text-secondary);
  max-height: 190px;
  overflow-y: auto;
  white-space: pre-wrap;
  word-break: break-word;
}

/* 自定义模板编辑器 */
.custom-template-editor {
  margin-top: 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.custom-template-chips {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  margin: 2px 0 4px;
}

.custom-template-chips-label {
  font-size: var(--text-caption);
  color: var(--skin-text-muted);
  font-weight: 500;
}

.placeholder-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 7px;
  border: 1px solid var(--skin-border);
  border-radius: 5px;
  background: var(--skin-canvas);
  color: var(--skin-text-body);
  font: inherit;
  font-size: var(--text-caption);
  cursor: pointer;
  transition: all var(--motion-fast, 140ms) ease;
}

.placeholder-chip code {
  font-family: var(--font-code, monospace);
  font-weight: 600;
  color: var(--skin-accent);
  font-size: var(--text-caption);
}

.placeholder-chip span {
  color: var(--skin-text-muted);
  font-size: var(--text-micro, 10.5px);
}

.placeholder-chip:hover:not(:disabled) {
  border-color: var(--skin-accent);
  background: var(--skin-accent-soft);
  transform: translateY(-1px);
}

.custom-template-meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.custom-preview-toggle-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  border: 1px solid var(--skin-border);
  border-radius: 5px;
  background: var(--skin-surface);
  color: var(--skin-text-muted);
  font: inherit;
  font-size: var(--text-caption);
  cursor: pointer;
  transition: all var(--motion-fast, 140ms) ease;
}

.custom-preview-toggle-btn:hover {
  border-color: var(--skin-accent);
  color: var(--skin-accent);
}

.custom-preview-toggle-btn.is-active {
  border-color: var(--skin-accent);
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
  font-weight: 500;
}

.custom-live-preview-box {
  margin-top: 6px;
  border: 1px solid var(--skin-border);
  border-radius: 8px;
  background: var(--skin-canvas);
  overflow: hidden;
}

.custom-live-preview-header {
  padding: 6px 10px;
  border-bottom: 1px dashed var(--skin-border);
  font-size: var(--text-caption);
  color: var(--skin-text-muted);
  background: var(--skin-surface);
}

/* 自动载入开关卡片 */
.skeleton-auto-apply-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  padding: 12px 14px;
  border: 1px solid var(--skin-border);
  border-radius: 9px;
  background: var(--skin-surface);
  transition: opacity var(--motion-fast, 140ms) ease;
}

.skeleton-auto-apply-card.is-disabled {
  opacity: 0.65;
}

.skeleton-auto-apply-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.skeleton-auto-apply-heading {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px 8px;
}

.skeleton-auto-apply-heading strong {
  font-size: var(--text-body);
  font-weight: 500;
  color: var(--skin-text-body);
  line-height: 1.4;
}

.skeleton-disabled-pill {
  font-size: var(--text-micro, 10.5px);
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--skin-canvas);
  color: var(--skin-text-muted);
  border: 1px solid var(--skin-border);
}

.skeleton-enabled-pill {
  font-size: var(--text-micro, 10.5px);
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
  font-weight: 500;
}

.skeleton-auto-apply-desc {
  margin: 0;
  font-size: var(--text-small);
  color: var(--skin-text-muted);
  line-height: 1.5;
}

/* 自定义 Toggle Switch */
.skeleton-switch {
  position: relative;
  display: inline-block;
  width: 38px;
  height: 22px;
  flex-shrink: 0;
  cursor: pointer;
}

.skeleton-switch-input {
  opacity: 0;
  width: 0;
  height: 0;
  position: absolute;
}

.skeleton-switch-track {
  position: absolute;
  inset: 0;
  background-color: var(--control-border, #d1d5db);
  border-radius: 22px;
  transition: background-color var(--motion-fast, 140ms) ease;
}

.skeleton-switch-thumb {
  position: absolute;
  content: '';
  height: 16px;
  width: 16px;
  left: 3px;
  bottom: 3px;
  background-color: #ffffff;
  border-radius: 50%;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  transition: transform var(--motion-fast, 140ms) ease;
}

.skeleton-switch-input:checked + .skeleton-switch-track {
  background-color: var(--skin-accent);
}

.skeleton-switch-input:checked + .skeleton-switch-track .skeleton-switch-thumb {
  transform: translateX(16px);
}

.skeleton-switch-input:focus-visible + .skeleton-switch-track {
  outline: 2px solid var(--control-focus);
  outline-offset: 2px;
}

.skeleton-switch-input:disabled + .skeleton-switch-track {
  cursor: not-allowed;
  opacity: 0.6;
}

@container module-settings (max-width: 500px) {
  .template-preset-grid {
    grid-template-columns: 1fr;
  }
  .skeleton-auto-apply-card {
    flex-direction: column;
    align-items: flex-start;
  }
  .skeleton-switch {
    align-self: flex-end;
  }
}
</style>
