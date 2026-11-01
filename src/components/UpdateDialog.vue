<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { PhDownloadSimple } from '@phosphor-icons/vue'
import { createDialogMotion } from '../lib/dialogMotion'
import type { AppUpdater } from '../lib/useUpdater'
import UpdateDownloadFallback from './UpdateDownloadFallback.vue'

const props = defineProps<{ updater: AppUpdater }>()
const dialog = ref<HTMLDialogElement>()
const motion = createDialogMotion(() => dialog.value)
watch(
  () => props.updater.visible.value,
  (visible) => {
    if (visible) motion.open()
    else void motion.close()
  },
  { flush: 'post' },
)
const actionText = computed(() => {
  switch (props.updater.status.value) {
    case 'downloading':
      return '正在下载…'
    case 'preparing':
      return '正在保存…'
    case 'installing':
      return '正在安装…'
    case 'restarting':
      return '正在重启…'
    case 'installed':
      return '重新启动'
    default:
      return '保存并更新'
  }
})
onBeforeUnmount(() => motion.dispose())
</script>

<template>
  <dialog
    ref="dialog"
    tabindex="-1"
    class="confirmation-dialog update-dialog"
    aria-modal="true"
    aria-labelledby="update-title"
    aria-describedby="update-description"
    @cancel.prevent="updater.close()"
  >
    <div class="dialog-icon"><PhDownloadSimple :size="28" weight="light" aria-hidden="true" /></div>
    <h2 id="update-title">更新到 WorkReport {{ updater.release.value?.version }}</h2>
    <p id="update-description">安装前会保存日报和设置，完成后自动重新启动。</p>
    <div v-if="updater.release.value?.notes" class="update-release-notes">{{ updater.release.value.notes }}</div>
    <div v-if="updater.busy.value" class="update-progress" role="status" aria-live="polite">
      <progress
        v-if="updater.status.value === 'downloading'"
        :value="updater.progress.value"
        max="100"
        aria-label="更新下载进度"
      ></progress>
      <span
        >{{ actionText
        }}<template v-if="updater.status.value === 'downloading' && updater.progress.value !== undefined">
          {{ updater.progress.value }}%</template
        ></span
      >
    </div>
    <p v-if="updater.error.value" class="error-text dialog-error" role="alert">{{ updater.error.value }}</p>
    <UpdateDownloadFallback :updater="updater" />
    <div class="dialog-actions">
      <md-text-button
        class="dialog-button dialog-button-secondary"
        data-window-close="default"
        :disabled="updater.busy.value"
        @click="updater.close()"
        >稍后</md-text-button
      >
      <md-filled-button
        class="dialog-button dialog-button-primary"
        :disabled="updater.busy.value || updater.checking.value"
        @click="updater.status.value === 'installed' ? updater.restart() : updater.install()"
        >{{ actionText }}</md-filled-button
      >
    </div>
  </dialog>
</template>

<style scoped>
.update-release-notes {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  max-height: 220px;
  overflow: auto;
  margin-top: 18px;
  font-size: var(--text-body);
  line-height: 1.8;
}
.update-progress {
  display: grid;
  gap: 8px;
  margin-top: 18px;
  font-size: var(--text-body);
  color: var(--md-sys-color-primary);
}
.update-progress progress {
  width: 100%;
  height: 6px;
  accent-color: var(--md-sys-color-primary);
}
</style>
