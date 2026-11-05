<script setup lang="ts">
import { computed, nextTick, reactive, ref, watch } from 'vue'
import dayjs from 'dayjs'
import { PhFolderOpen, PhPencilSimple } from '@phosphor-icons/vue'
import type { FileFormat } from '../lib/workspace'
import {
  cloneFileSettings,
  DATE_TOKENS,
  DEFAULT_FILE_SETTINGS,
  FILE_FEATURES,
  fileDirectoryError,
  fileSettingsError,
  filenameTemplateError,
  formatFileName,
  REPORT_TOKENS,
  type FileFeature,
  type FileSettings,
} from '../lib/fileSettings'

const props = defineProps<{
  modelValue: FileSettings
  busy: boolean
  workspacePath: string
  journalFormat?: FileFormat
  /** 只渲染指定功能；模块自己的设置弹窗只展示本模块的目录规则。 */
  features?: FileFeature[]
  /** 在模块弹窗中直接展示字段，避免重复的卡片与功能标题。 */
  compact?: boolean
}>()
const emit = defineEmits<{ 'update:modelValue': [value: FileSettings] }>()
const editing = reactive<Record<FileFeature, boolean>>({ journal: false, excel: false, summary: false })
const errors = reactive<Record<FileFeature, string>>({ journal: '', excel: '', summary: '' })
/** 跨功能冲突（例如 Excel 与总结使用同一目录）可能不在当前展示的功能上，单独在顶部提示。 */
const globalError = ref('')
const root = ref<HTMLElement>()
const disabled = computed(() => props.busy || !props.workspacePath)
/** 模块设置弹窗只显示本模块负责的功能，全局目录管理已拆分到各模块。 */
const activeFeatures = computed(() =>
  FILE_FEATURES.filter((feature) => !props.features?.length || props.features.includes(feature.id)),
)
const context = computed(() => ({
  date: dayjs().format('YYYYMMDD'),
  startDate: dayjs().startOf('month').format('YYYYMMDD'),
  endDate: dayjs().format('YYYYMMDD'),
  time: dayjs().format('HHmmss'),
}))
watch(
  () => props.workspacePath,
  () => {
    globalError.value = ''
    for (const { id } of activeFeatures.value) {
      editing[id] = false
      errors[id] = ''
    }
  },
)
function update(feature: FileFeature, key: 'directory' | 'filenameTemplate', value: string) {
  if (disabled.value) return
  errors[feature] = ''
  globalError.value = ''
  const settings = cloneFileSettings(props.modelValue)
  settings[feature][key] = value
  emit('update:modelValue', settings)
}
function location(feature: FileFeature) {
  const base = props.workspacePath.replace(/[\\/]+$/, '')
  const directory = props.modelValue[feature].directory.trim().replace(/\\/g, '/')
  return `${base}${directory ? `/${directory}` : ''}${feature === 'journal' ? `/${context.value.date.slice(0, 4)}` : ''}`
}
function example(feature: FileFeature) {
  return formatFileName(
    props.modelValue[feature],
    context.value,
    feature === 'excel' ? 'xlsx' : feature === 'journal' ? (props.journalFormat ?? 'md') : 'md',
  )
}
async function editDirectory(feature: FileFeature) {
  if (disabled.value) return
  if (editing[feature]) {
    errors[feature] = fileDirectoryError(feature, props.modelValue[feature].directory)
    if (errors[feature]) return
    update(feature, 'directory', props.modelValue[feature].directory.trim().replace(/\\/g, '/'))
    editing[feature] = false
    return
  }
  editing[feature] = true
  await nextTick()
  const input = root.value?.querySelector<HTMLInputElement>(`#${feature}-save-directory`)
  input?.focus()
  input?.select()
}
function insertToken(feature: FileFeature, token: string) {
  if (disabled.value) return
  const input = root.value?.querySelector<HTMLInputElement>(`#${feature}-filename-template`)
  const current = props.modelValue[feature].filenameTemplate
  const from = input?.selectionStart ?? current.length,
    to = input?.selectionEnd ?? from
  const text = `{${token}}`
  update(feature, 'filenameTemplate', current.slice(0, from) + text + current.slice(to))
  void nextTick(() => {
    input?.focus()
    input?.setSelectionRange(from + text.length, from + text.length)
  })
}
function restore(feature: FileFeature) {
  if (disabled.value) return
  const settings = cloneFileSettings(props.modelValue)
  settings[feature] = { ...DEFAULT_FILE_SETTINGS[feature] }
  errors[feature] = ''
  globalError.value = ''
  editing[feature] = false
  emit('update:modelValue', settings)
}
function validate() {
  globalError.value = ''
  for (const { id } of activeFeatures.value) errors[id] = ''
  if (!props.workspacePath) {
    errors[activeFeatures.value[0]?.id ?? 'journal'] = '请先选择工作空间。'
    return false
  }
  const invalid = fileSettingsError(props.modelValue)
  if (invalid) {
    if (activeFeatures.value.some((feature) => feature.id === invalid.feature))
      errors[invalid.feature] = invalid.message
    else globalError.value = invalid.message
    return false
  }
  const settings = cloneFileSettings(props.modelValue)
  for (const { id } of activeFeatures.value) {
    settings[id].directory = settings[id].directory.trim().replace(/\\/g, '/')
    settings[id].filenameTemplate = settings[id].filenameTemplate.trim()
  }
  if (JSON.stringify(settings) !== JSON.stringify(props.modelValue)) emit('update:modelValue', settings)
  return true
}
function focusInvalid() {
  const feature = activeFeatures.value.find(({ id }) => errors[id])?.id
  if (!feature) {
    if (globalError.value)
      root.value
        ?.querySelector<HTMLInputElement>(`#${activeFeatures.value[0]?.id ?? 'journal'}-filename-template`)
        ?.focus()
    return
  }
  const field = filenameTemplateError(feature, props.modelValue[feature].filenameTemplate)
    ? 'filename-template'
    : 'save-directory'
  root.value?.querySelector<HTMLInputElement>(`#${feature}-${field}`)?.focus()
}
defineExpose({ validate, focusInvalid })
</script>

<template>
  <div ref="root" class="directory-settings" :class="{ 'is-compact': compact }">
    <p v-if="!compact" class="field-hint">目录和文件名随当前工作空间保存。编辑完成后，点击“保存设置”统一应用。</p>
    <p v-if="globalError" class="settings-error" role="alert">{{ globalError }}</p>
    <section
      v-for="feature in activeFeatures"
      :key="feature.id"
      class="directory-settings-card"
      :aria-labelledby="`${feature.id}-files-title`"
    >
      <header class="directory-settings-card-heading">
        <h3 :id="`${feature.id}-files-title`" :class="{ 'sr-only': compact }">{{ feature.label }}</h3>
        <button type="button" class="text-action" :disabled="disabled" @click="restore(feature.id)">恢复默认</button>
      </header>
      <div class="excel-path-field">
        <label :for="`${feature.id}-save-directory`">{{ feature.label }}目录名称</label>
        <div class="excel-path-control">
          <input
            :id="`${feature.id}-save-directory`"
            :value="modelValue[feature.id].directory"
            :readonly="!editing[feature.id]"
            :disabled="disabled"
            type="text"
            spellcheck="false"
            :placeholder="
              feature.id === 'journal' ? '沿用工作空间根目录，按年份归档' : DEFAULT_FILE_SETTINGS[feature.id].directory
            "
            :aria-describedby="`${feature.id}-directory-hint`"
            @input="update(feature.id, 'directory', ($event.target as HTMLInputElement).value)"
          />
          <button type="button" class="text-action" :disabled="disabled" @click="editDirectory(feature.id)">
            <PhPencilSimple :size="16" aria-hidden="true" />{{
              editing[feature.id] ? '完成' : modelValue[feature.id].directory ? '修改名称' : '设置目录'
            }}
          </button>
        </div>
        <p :id="`${feature.id}-directory-hint`" class="field-hint">
          仅允许工作空间内的目录名称或相对路径，例如“{{ feature.label }}”或“输出/{{
            feature.label
          }}”。更改现有目录名称时会移动其内容；包含文件时保存前会提示确认。
        </p>
      </div>
      <div class="excel-path-field">
        <label :for="`${feature.id}-filename-template`">{{ feature.label }}文件名规则（不含扩展名）</label>
        <input
          :id="`${feature.id}-filename-template`"
          :value="modelValue[feature.id].filenameTemplate"
          :disabled="disabled"
          type="text"
          spellcheck="false"
          maxlength="120"
          :aria-describedby="`${feature.id}-filename-hint`"
          @input="update(feature.id, 'filenameTemplate', ($event.target as HTMLInputElement).value)"
        />
        <div class="filename-placeholder-options" :aria-label="`${feature.label}日期占位符`">
          <button
            v-for="token in feature.id === 'journal' ? DATE_TOKENS : REPORT_TOKENS"
            :key="token"
            type="button"
            :disabled="disabled"
            :aria-label="`插入 ${token} 占位符`"
            @mousedown.prevent
            @click="insertToken(feature.id, token)"
          >
            {{ '{' + token + '}' }}
          </button>
        </div>
        <p :id="`${feature.id}-filename-hint`" class="field-hint">
          {{
            feature.id === 'journal'
              ? '{date} 表示日报日期；也可用 {YYYY}-{MM}-{DD} 组合完整年月日。'
              : '{startDate}、{endDate} 表示所选范围，{date}、{time} 表示生成日期与时间。'
          }}文字可以直接输入，扩展名自动添加。
        </p>
      </div>
      <div class="workspace-location directory-settings-preview">
        <PhFolderOpen :size="19" aria-hidden="true" />
        <div>
          <strong>保存位置示例</strong
          ><span>{{ workspacePath ? `${location(feature.id)}/${example(feature.id)}` : '请先选择工作空间' }}</span>
        </div>
      </div>
      <p v-if="errors[feature.id]" class="settings-error" role="alert">{{ errors[feature.id] }}</p>
    </section>
  </div>
</template>
