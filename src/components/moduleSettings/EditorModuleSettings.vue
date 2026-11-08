<script setup lang="ts">
import { computed, ref } from 'vue'
import DirectorySettings from '../DirectorySettings.vue'
import ModuleSettingsSection from './ModuleSettingsSection.vue'
import SettingHint from '../SettingHint.vue'
import { cloneFileSettings, type FileSettings } from '../../lib/fileSettings'
import { useModuleSettingsContext } from '../../modules/settingsContext'

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

const skeletonOptions = [
  { id: 'none', name: '空白', desc: '不使用预设骨架，保持全新空白编辑区。' },
  { id: 'standard', name: '标准工作日报', desc: '今日产出、明日计划与思考复盘。' },
  { id: 'agile', name: '敏捷站会三问', desc: '昨日回顾、今日要务与阻碍风险。' },
  { id: 'project', name: '项目重点推进', desc: '核心里程碑、任务清单与技术细节。' },
  { id: 'minimal', name: '极简打卡清单', desc: '今日事项打卡与备忘备注。' },
  { id: 'custom', name: '自定义模板', desc: '使用你自定义的结构与占位符。' },
]

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
    <div class="database-field">
      <div class="template-preset-grid" role="radiogroup" aria-labelledby="editor-skeleton-title">
        <label
          v-for="item in skeletonOptions"
          :key="item.id"
          :class="{ selected: draft.editor.defaultTemplateId === item.id }"
          class="database-mode-label"
        >
          <input
            type="radio"
            name="editor-default-template"
            :value="item.id"
            :checked="draft.editor.defaultTemplateId === item.id"
            :disabled="busy"
            @change="draft.editor.defaultTemplateId = item.id"
          />
          <span class="database-mode-copy">
            <strong>{{ item.name }}</strong>
            <span>{{ item.desc }}</span>
          </span>
        </label>
      </div>

      <div v-if="draft.editor.defaultTemplateId === 'custom'" class="generation-preference custom-template-editor">
        <div class="setting-field-header">
          <label for="custom-template-input">自定义预设骨架</label>
          <SettingHint text="支持占位符插值：{date}、{weekday}、{year}、{month}、{day}" />
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
        <span class="preference-count">{{ draft.editor.customTemplate.length }} / 10000</span>
      </div>
      <p v-if="templateError" class="settings-error" role="alert">{{ templateError }}</p>

      <div class="auto-apply-row">
        <label class="auto-apply-label">
          <input
            v-model="draft.editor.autoApplyTemplate"
            type="checkbox"
            :disabled="busy || draft.editor.defaultTemplateId === 'none'"
          />
          <span>新建或切换至空白日报时自动载入此骨架</span>
          <SettingHint text="开启后，打开未创建或无内容的日期时会自动填入所选骨架；在未修改前不会向磁盘写入文件。" />
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
.template-preset-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 10px;
}

.custom-template-editor {
  margin-top: 14px;
}

.auto-apply-row {
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--skin-border);
}

.auto-apply-label {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: var(--text-body);
  font-weight: 500;
  color: var(--skin-text);
  cursor: pointer;
}

.auto-apply-hint {
  margin: 4px 0 0 24px;
  color: var(--skin-text-muted);
  font-size: var(--text-small);
  line-height: 1.45;
}
</style>
