<script setup lang="ts">
import { computed, ref } from 'vue'
import DirectorySettings from '../DirectorySettings.vue'
import ModuleSettingsSection from './ModuleSettingsSection.vue'
import { cloneFileSettings, type FileSettings } from '../../lib/fileSettings'
import { useModuleSettingsContext } from '../../modules/settingsContext'

/**
 * 编辑模块设置：新日报默认格式、日报保存位置与文件名、AI 润色提示词。
 *
 * 所有修改都直接写入弹窗提供的设置草稿，由弹窗外壳统一保存。
 */
const context = useModuleSettingsContext()
const { draft, busy, workspace } = context
const openGlobalSettings = context.openGlobalSettings
const directoryForm = ref<InstanceType<typeof DirectorySettings>>()
const promptInput = ref<HTMLTextAreaElement>()
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

function validate() {
  if (promptError.value) return false
  return directoryForm.value?.validate() ?? true
}
function focusInvalid() {
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
    title="日报润色"
    heading-id="editor-prompt-title"
    description="自定义 AI 润色方式，留空时使用默认规则。"
  >
    <div class="generation-preference">
      <label for="polish-prompt">日报润色提示词</label>
      <p id="polish-prompt-hint">追加在固定的事实、安全和格式规则之后；留空时使用默认润色规则。</p>
      <textarea
        id="polish-prompt"
        ref="promptInput"
        v-model="draft.ai.generationPreference"
        :disabled="busy"
        maxlength="2000"
        rows="4"
        aria-describedby="polish-prompt-hint"
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
