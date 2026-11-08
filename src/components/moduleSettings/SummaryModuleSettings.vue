<script setup lang="ts">
import { computed, ref } from 'vue'
import DirectorySettings from '../DirectorySettings.vue'
import ModuleSettingsSection from './ModuleSettingsSection.vue'
import SettingHint from '../SettingHint.vue'
import { useModuleSettingsContext } from '../../modules/settingsContext'
import { cloneFileSettings, type FileSettings } from '../../lib/fileSettings'
import { SUMMARY_PRESETS, matchSummaryPreset } from '../../lib/summaryPresets'

const context = useModuleSettingsContext()
const { draft, busy, workspace } = context
const openGlobalSettings = context.openGlobalSettings
const directoryForm = ref<InstanceType<typeof DirectorySettings>>()
const promptInput = ref<HTMLTextAreaElement>()

/** 默认 AI 只用于展示，修改仍集中在全局设置的模型配置里。 */
const defaultProvider = computed(() =>
  draft.value.ai.providers.find((provider) => provider.id === draft.value.ai.defaultProviderId),
)
const activePresetId = computed(() => matchSummaryPreset(draft.value.ai.summaryPrompt)?.id ?? 'custom')
const promptTooLong = computed(() => draft.value.ai.summaryPrompt.length > 2000)
/** 目录规则保存在 files 中，旧版 summary.outputDirectory 只作为兜底。 */
const files = computed<FileSettings>({
  get: () => cloneFileSettings(draft.value.files, draft.value.summary.outputDirectory),
  set: (value) => {
    draft.value.files = value
  },
})

function validate(): boolean {
  if (promptTooLong.value) return false
  return directoryForm.value?.validate() ?? true
}

function focusInvalid() {
  if (promptTooLong.value) {
    promptInput.value?.focus()
    return
  }
  directoryForm.value?.focusInvalid()
}

defineExpose({ validate, focusInvalid })
</script>

<template>
  <ModuleSettingsSection
    title="默认 AI"
    heading-id="summary-default-ai-title"
    description="模型与 API 密钥在全局设置中统一管理。"
  >
    <div class="module-settings-ai-status" :class="{ 'is-ready': !!defaultProvider }">
      <span>{{
        defaultProvider ? `使用默认 AI：${defaultProvider.name}` : '尚未配置默认 AI，生成总结前请先配置。'
      }}</span>
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
  <ModuleSettingsSection
    title="总结提示词"
    heading-id="summary-prompt-title"
    description="用于选中日期内的日报，留空时使用默认总结规则。"
  >
    <div class="generation-preference">
      <div class="setting-field-header">
        <label for="summary-prompt">框选总结提示词</label>
        <SettingHint
          text="在总结窗口中临时修改提示词只影响本次生成，不会写回这里的设置。追加在固定的事实、安全、Markdown 结构规则之后；留空时使用默认总结规则。"
        />
      </div>
      <div class="summary-presets-bar" role="group" aria-label="设置默认总结场景预设">
        <span class="summary-presets-label">预设快选：</span>
        <div class="summary-presets-list">
          <button
            v-for="preset in SUMMARY_PRESETS"
            :key="preset.id"
            type="button"
            class="summary-preset-pill"
            :class="{ active: activePresetId === preset.id }"
            :disabled="busy"
            :title="preset.description"
            @click="draft.ai.summaryPrompt = preset.prompt"
          >
            {{ preset.label }}
          </button>
        </div>
      </div>
      <textarea
        id="summary-prompt"
        ref="promptInput"
        v-model="draft.ai.summaryPrompt"
        :disabled="busy"
        maxlength="2000"
        rows="4"
        aria-describedby="summary-prompt-hint"
        placeholder="例如：按项目归类，提炼成果、风险和下一步计划，控制在 500 字内。"
        spellcheck="false"
      ></textarea>
      <span class="preference-count">{{ draft.ai.summaryPrompt.length }} / 2000</span>
    </div>
    <p v-if="promptTooLong" class="settings-error" role="alert">框选总结提示词不能超过 2000 字。</p>
  </ModuleSettingsSection>
  <ModuleSettingsSection
    title="导出保存位置"
    heading-id="summary-directory-title"
    description="Markdown 与 TXT 共用目录和命名规则。"
  >
    <DirectorySettings
      ref="directoryForm"
      :model-value="files"
      :busy="busy"
      :features="['summary']"
      compact
      :workspace-path="workspace?.path ?? ''"
      @update:model-value="files = $event"
    />
  </ModuleSettingsSection>
</template>
