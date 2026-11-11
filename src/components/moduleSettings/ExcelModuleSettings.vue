<script setup lang="ts">
import { computed, ref } from 'vue'
import DirectorySettings from '../DirectorySettings.vue'
import ExcelSettings from '../ExcelSettings.vue'
import ModuleSettingsSection from './ModuleSettingsSection.vue'
import { useModuleSettingsContext } from '../../modules/settingsContext'
import { cloneFileSettings, type FileSettings } from '../../lib/fileSettings'

const context = useModuleSettingsContext()
const { draft, busy, aiReady, workspace } = context
const openGlobalSettings = context.openGlobalSettings
const root = ref<HTMLElement>()
const excelForm = ref<InstanceType<typeof ExcelSettings>>()
const directoryForm = ref<InstanceType<typeof DirectorySettings>>()
const directorySection = ref<InstanceType<typeof ModuleSettingsSection>>()
/** 目录规则保存在 files 中，旧版 summary.outputDirectory 只作为兜底。 */
const files = computed<FileSettings>({
  get: () => cloneFileSettings(draft.value.files, draft.value.summary.outputDirectory),
  set: (value) => {
    draft.value.files = value
  },
})
let excelInvalid = false
let directoryInvalid = false

function scrollToDirectory() {
  const sectionEl = directorySection.value?.$el as HTMLElement | undefined
  if (!sectionEl) return
  const container = sectionEl.closest<HTMLElement>('.module-settings-body')
  if (container) {
    const containerRect = container.getBoundingClientRect()
    const targetRect = sectionEl.getBoundingClientRect()
    const top = Math.max(0, container.scrollTop + (targetRect.top - containerRect.top) - 12)
    container.scrollTo({ top, behavior: 'smooth' })
  }
}

function validate(): boolean {
  excelInvalid = !(excelForm.value?.validate() ?? true)
  directoryInvalid = !(directoryForm.value?.validate() ?? true)
  return !excelInvalid && !directoryInvalid
}

function focusInvalid() {
  if (excelInvalid) {
    // ExcelSettings 暂未暴露 focusInvalid，失败时按提示定位到模板或提示词。
    const form = excelForm.value as unknown as { focusInvalid?: () => void } | undefined
    if (form?.focusInvalid) {
      form.focusInvalid()
      return
    }
    const templatePath = draft.value.excel.templatePath.trim()
    const templateInvalid = !templatePath || !/\.xlsx$/i.test(templatePath)
    const selector =
      templateInvalid || !workspace.value?.path || !aiReady.value ? '#excel-template' : '#excel-output-prompt'
    root.value?.querySelector<HTMLElement>(selector)?.focus()
    return
  }
  if (directoryInvalid) directoryForm.value?.focusInvalid()
}

defineExpose({ validate, focusInvalid })
</script>

<template>
  <div ref="root" class="module-settings-stack">
    <ModuleSettingsSection
      title="Excel 输出"
      heading-id="excel-output-title"
      description="用默认 AI 整理所选日报，并填入指定模板。"
    >
      <ExcelSettings
        ref="excelForm"
        :model-value="draft.excel"
        :busy="busy"
        :ai-ready="aiReady"
        :workspace-path="workspace?.path ?? ''"
        :output-directory="files.excel.directory"
        @update:model-value="draft.excel = $event"
        @configure-ai="openGlobalSettings('ai')"
        @configure-directories="scrollToDirectory"
      />
    </ModuleSettingsSection>
    <ModuleSettingsSection
      ref="directorySection"
      title="保存位置与文件名"
      heading-id="excel-directory-title"
      description="报告保存在当前工作空间内，扩展名自动添加。"
    >
      <DirectorySettings
        ref="directoryForm"
        :model-value="files"
        :busy="busy"
        :features="['excel']"
        compact
        :workspace-path="workspace?.path ?? ''"
        @update:model-value="files = $event"
      />
    </ModuleSettingsSection>
  </div>
</template>
