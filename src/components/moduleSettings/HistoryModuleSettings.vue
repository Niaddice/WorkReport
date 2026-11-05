<script setup lang="ts">
import { computed } from 'vue'
import { clampHistoryLimit, MAX_HISTORY_LIMIT_BYTES, MIN_HISTORY_LIMIT_BYTES } from '../../lib/database'
import { useModuleSettingsContext } from '../../modules/settingsContext'
import ModuleSettingsSection from './ModuleSettingsSection.vue'

/**
 * 历史记录模块设置：历史版本存储上限。
 *
 * 修改直接写入设置草稿；边界由 clampHistoryLimit 统一收敛。
 */
const context = useModuleSettingsContext()
const { draft, busy } = context
const GB = 1024 * 1024 * 1024
const historyLimitGb = computed(() => Math.round(draft.value.database.historyLimitBytes / GB))

function onHistoryLimitInput(event: Event) {
  const value = Number((event.target as HTMLInputElement).value)
  draft.value.database.historyLimitBytes = clampHistoryLimit(Number.isFinite(value) ? value * GB : Number.NaN)
}

const validate = () => true
const focusInvalid = () => {}
defineExpose({ validate, focusInvalid })
</script>

<template>
  <ModuleSettingsSection
    title="历史版本"
    heading-id="history-limit-title"
    description="限制历史版本占用空间，超出后自动清理最旧的版本。"
  >
    <div class="database-field">
      <label class="database-field-label" for="history-limit">历史版本存储上限</label>
      <div class="history-limit">
        <input
          id="history-limit"
          type="range"
          :min="MIN_HISTORY_LIMIT_BYTES / GB"
          :max="MAX_HISTORY_LIMIT_BYTES / GB"
          step="1"
          :value="historyLimitGb"
          :disabled="busy"
          :aria-valuetext="`${historyLimitGb} GB`"
          @input="onHistoryLimitInput"
        /><output for="history-limit">{{ historyLimitGb }} GB</output>
      </div>
      <p class="field-hint">
        默认 1 GB，滑块每格 1 GB（最大 10 GB）。历史版本超过该容量时，从最旧的一天开始整天清理；单日最多保留 100
        个版本。
      </p>
    </div>
    <p class="field-hint">
      首次保存立即记录一份历史版本，之后自动保存每分钟最多记录一份；手动保存、切换日报或关闭应用时会补存最终版本，
      内容与格式相同的版本不会重复记录。
    </p>
  </ModuleSettingsSection>
</template>
