<script setup lang="ts">
import { onBeforeUnmount, reactive, ref } from 'vue'
import { PhFolderOpen } from '@phosphor-icons/vue'
import { createDialogMotion } from '../lib/dialogMotion'
import type { FileChangesPlan, FileFeature } from '../lib/fileSettings'

const dialog = ref<HTMLDialogElement>()
const motion = createDialogMotion(() => dialog.value)
const plan = ref<FileChangesPlan | null>(null)
const choices = reactive<Record<FileFeature, boolean>>({ journal: false, excel: false, summary: false })
interface PendingConfirmation {
  resolve: (value: Record<FileFeature, boolean> | null) => void
  decision: Record<FileFeature, boolean> | null
  finishing: boolean
}
let pending: PendingConfirmation | undefined
function confirm(value: FileChangesPlan): Promise<Record<FileFeature, boolean> | null> {
  pending?.resolve(null)
  plan.value = value
  choices.journal = false
  choices.excel = false
  choices.summary = false
  const result = new Promise<Record<FileFeature, boolean> | null>((resolve) => {
    pending = { resolve, decision: null, finishing: false }
  })
  motion.open()
  return result
}
async function finish(apply: boolean) {
  const active = pending
  if (!active) return
  if (active.finishing) {
    if (!apply) active.decision = null
    return
  }
  active.decision = apply ? { ...choices } : null
  active.finishing = true
  const closed = await motion.close()
  if (pending !== active) return
  pending = undefined
  active.resolve(closed ? active.decision : null)
  plan.value = null
}
defineExpose({ confirm, isOpen: () => !!dialog.value?.open, close: () => finish(false) })
onBeforeUnmount(() => {
  pending?.resolve(null)
  pending = undefined
  motion.dispose()
})
</script>

<template>
  <dialog
    ref="dialog"
    class="confirmation-dialog file-changes-dialog"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="file-changes-title"
    aria-describedby="file-changes-description"
    tabindex="-1"
    @cancel.prevent="finish(false)"
  >
    <div class="dialog-icon"><PhFolderOpen :size="28" weight="light" /></div>
    <h2 id="file-changes-title">确认目录和文件名更改</h2>
    <p id="file-changes-description">
      请核对以下更改。目录中的文件会随目录一起移动；文件名规则可选择是否应用到已有文件。
    </p>
    <div v-if="plan" class="file-changes-body">
      <section v-for="change in plan.directories" :key="`directory-${change.feature}`" class="file-change-item">
        <h3>{{ change.label }}目录</h3>
        <p>
          <strong>{{ change.from || '工作空间根目录' }}</strong> → <strong>{{ change.to || '工作空间根目录' }}</strong>
        </p>
        <p v-if="change.fileCount">目录中有 {{ change.fileCount }} 个文件，应用后将一起移动，请确认是否更改。</p>
        <p v-else>目录为空或尚未创建，应用后使用新目录。</p>
        <p v-if="change.targetFileCount">
          目标目录已有 {{ change.targetFileCount }} 个文件，将保留现有内容，请确认使用该目录。
        </p>
      </section>
      <section v-for="change in plan.filenames" :key="`filename-${change.feature}`" class="file-change-item">
        <h3>{{ change.label }}文件名规则</h3>
        <p class="file-change-template">
          <span>{{ change.from }}</span> → <strong>{{ change.to }}</strong>
        </p>
        <p>检测到 {{ change.fileCount }} 个已有文件。</p>
        <div
          v-if="change.fileCount"
          class="file-change-choices"
          role="radiogroup"
          :aria-label="`${change.label}已有文件处理方式`"
        >
          <label
            ><input
              type="radio"
              :name="`${change.feature}-rename-existing`"
              :checked="!choices[change.feature]"
              @change="choices[change.feature] = false"
            /><span>仅新建或新生成的文件使用新规则</span></label
          >
          <label
            ><input
              type="radio"
              :name="`${change.feature}-rename-existing`"
              :checked="choices[change.feature]"
              @change="choices[change.feature] = true"
            /><span>同时重命名已有文件</span></label
          >
        </div>
        <p v-if="change.unrecognizedCount" class="field-hint">
          {{ change.unrecognizedCount }} 个文件无法识别日期，将保留原文件名。
        </p>
      </section>
      <p v-for="warning in plan.warnings" :key="warning" class="field-hint">{{ warning }}</p>
    </div>
    <div class="dialog-actions">
      <md-text-button class="dialog-button dialog-button-secondary" data-window-close="default" @click="finish(false)"
        >取消</md-text-button
      ><md-filled-button class="dialog-button dialog-button-primary" data-window-close @click="finish(true)"
        >应用更改</md-filled-button
      >
    </div>
  </dialog>
</template>
