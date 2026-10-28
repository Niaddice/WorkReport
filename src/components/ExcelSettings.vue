<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { isTauri } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { PhFileXls, PhSparkle } from '@phosphor-icons/vue'
import { cloneExcelSettings, RECOMMENDED_EXCEL_PROMPT, type ExcelSettings } from '../lib/useExcel'

const props = defineProps<{ modelValue: ExcelSettings; busy: boolean; aiReady: boolean; workspacePath: string }>()
const emit = defineEmits<{ 'update:modelValue': [settings: ExcelSettings]; configureAi: [] }>()
const draft = computed(() => props.modelValue)
const picking = ref(false)
const localError = ref('')
const formBusy = computed(() => props.busy || picking.value)
const nativeApp = isTauri()
watch(
  () => props.workspacePath,
  () => {
    localError.value = ''
  },
)
watch(
  () => props.modelValue,
  () => {
    localError.value = ''
  },
)

function update(value: Partial<ExcelSettings>) {
  if (props.busy) return
  localError.value = ''
  emit('update:modelValue', cloneExcelSettings({ ...draft.value, ...value }))
}

function toggle() {
  if (formBusy.value || (!draft.value.enabled && (!props.aiReady || !props.workspacePath))) return
  update({ enabled: !draft.value.enabled })
}

function applyRecommendedPrompt() {
  if (formBusy.value || !props.workspacePath) return
  update({ prompt: RECOMMENDED_EXCEL_PROMPT })
}

async function chooseTemplate() {
  if (formBusy.value || !props.workspacePath) return
  if (!nativeApp) {
    localError.value = '请在桌面应用中选择 Excel 模板。'
    return
  }
  const workspacePath = props.workspacePath
  picking.value = true
  localError.value = ''
  try {
    const path = await open({
      title: '选择 Excel 输出模板',
      multiple: false,
      directory: false,
      filters: [{ name: 'Excel 工作簿', extensions: ['xlsx'] }],
    })
    if (typeof path === 'string' && props.workspacePath === workspacePath) update({ templatePath: path })
  } catch (cause) {
    localError.value = `选择模板失败：${String(cause)}`
  } finally {
    picking.value = false
  }
}

function validate(): boolean {
  localError.value = ''
  if (picking.value) {
    localError.value = '请先完成模板选择。'
    return false
  }
  if (draft.value.enabled) {
    if (!props.workspacePath) {
      localError.value = '请先选择工作空间，再配置 Excel 输出。'
      return false
    }
    if (!props.aiReady) {
      localError.value = '请先配置并选择默认 AI。'
      return false
    }
    if (!draft.value.templatePath.trim() || !/\.xlsx$/i.test(draft.value.templatePath.trim())) {
      localError.value = '请选择一个 .xlsx 格式的 Excel 模板。'
      return false
    }
    if (!draft.value.prompt.trim()) {
      localError.value = '请填写 Excel 输出提示词，说明需要填入的内容和位置。'
      return false
    }
    if (draft.value.prompt.length > 6000) {
      localError.value = 'Excel 输出提示词不能超过 6000 字。'
      return false
    }
  }
  return true
}

defineExpose({ validate })
</script>

<template>
  <div class="excel-settings">
    <div class="excel-enable-row">
      <div class="excel-enable-copy">
        <h3 id="excel-enable-label">启用 Excel 输出</h3>
        <p id="excel-enable-description">
          在 Excel报告模块点击 Xls 图标，在窗口内核对或临时修改提示词，确认后调用默认 AI 整理日报并填充模板。
        </p>
      </div>
      <button
        type="button"
        class="excel-enable-switch"
        role="switch"
        :aria-checked="draft.enabled"
        aria-labelledby="excel-enable-label"
        aria-describedby="excel-enable-description"
        :disabled="formBusy || (!draft.enabled && (!aiReady || !workspacePath))"
        @click="toggle"
      >
        <span aria-hidden="true"></span>
      </button>
    </div>
    <p v-if="!workspacePath" class="settings-muted excel-prerequisite">请先选择工作空间，再配置 Excel 输出。</p>
    <p v-else-if="!aiReady" class="settings-muted excel-prerequisite">
      启用前请配置并选择默认 AI。<button type="button" class="text-action" @click="emit('configureAi')">
        前往模型配置
      </button>
    </p>
    <Transition name="settings-reveal">
      <div v-if="draft.enabled" class="settings-reveal-shell">
        <div class="settings-reveal-inner">
          <div class="excel-settings-fields">
            <div class="excel-path-field">
              <label for="excel-template">Excel 模板</label>
              <div class="excel-path-control">
                <input
                  id="excel-template"
                  :value="draft.templatePath"
                  type="text"
                  placeholder="选择 .xlsx 模板文件"
                  readonly
                  :disabled="formBusy || !workspacePath"
                  spellcheck="false"
                /><button
                  type="button"
                  class="text-action"
                  :disabled="formBusy || !workspacePath"
                  @click="chooseTemplate"
                >
                  <PhFileXls :size="17" aria-hidden="true" />选择模板
                </button>
              </div>
              <p class="field-hint">
                生成新文件，默认沿用模板样式，按提示词调整指定区域。需要填写的公式单元格会直接替换为 AI
                结果，原模板保持不变。
              </p>
            </div>
            <div class="generation-preference excel-prompt-field">
              <div class="excel-prompt-heading">
                <label for="excel-output-prompt">Excel 输出提示词</label>
                <button
                  type="button"
                  class="text-action"
                  :disabled="formBusy || !workspacePath || draft.prompt === RECOMMENDED_EXCEL_PROMPT"
                  @click="applyRecommendedPrompt"
                >
                  <PhSparkle :size="16" aria-hidden="true" />使用推荐设置
                </button>
              </div>
              <p id="excel-output-prompt-hint">
                说明日报整理方式和填写位置，也可要求黑色细边框、自动换行及按内容增高行高。仅对指定区域应用格式。
                推荐设置会替换当前提示词，点击“保存设置”后生效。
              </p>
              <textarea
                id="excel-output-prompt"
                :value="draft.prompt"
                :disabled="formBusy || !workspacePath"
                maxlength="6000"
                rows="5"
                aria-describedby="excel-output-prompt-hint"
                placeholder="使用推荐设置填入按项目分类、日期和工时的模板规则，也可输入自定义提示词。"
                spellcheck="false"
                @input="update({ prompt: ($event.target as HTMLTextAreaElement).value })"
              ></textarea
              ><span class="preference-count">{{ draft.prompt.length }} / 6000</span>
            </div>
            <p class="field-hint">报告固定保存到当前工作空间下的 excel 目录，目录不存在时自动创建。</p>
            <p class="field-hint excel-range-note">支持单日或任意日期范围，可跨周、跨月或跨年。</p>
          </div>
        </div>
      </div>
    </Transition>
    <Transition name="status-fade" mode="out-in">
      <p v-if="localError" key="error" class="settings-error" role="alert">{{ localError }}</p>
    </Transition>
  </div>
</template>
