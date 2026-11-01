<script setup lang="ts">
import { PhArrowSquareOut } from '@phosphor-icons/vue'
import type { AppUpdater } from '../lib/useUpdater'

defineProps<{ updater: AppUpdater }>()
</script>

<template>
  <div v-if="updater.manualDownloadAvailable.value" class="update-manual-download" role="status">
    <p>自动更新暂不可用，可前往下载页面手动下载并安装最新版本。</p>
    <a
      class="text-action"
      :href="updater.manualDownloadUrl"
      target="_blank"
      rel="noopener noreferrer"
      :aria-disabled="updater.manualDownloadOpening.value"
      @click.prevent="updater.openManualDownload()"
    >
      <PhArrowSquareOut :size="16" aria-hidden="true" />
      {{ updater.manualDownloadOpening.value ? '正在打开…' : '打开下载页面' }}
    </a>
    <span class="update-manual-url">{{ updater.manualDownloadUrl }}</span>
    <p>手动安装前，请先保存并退出当前应用。</p>
    <p v-if="updater.manualDownloadError.value" class="update-manual-error" role="alert">
      {{ updater.manualDownloadError.value }}
    </p>
  </div>
</template>

<style scoped>
.update-manual-download {
  display: grid;
  gap: 10px;
  margin-top: 16px;
  padding: 14px 16px;
  border-radius: 16px;
  background: var(--md-sys-color-secondary-container);
  font-size: var(--text-body);
}
.update-manual-download p {
  margin: 0;
  color: var(--md-sys-color-on-surface-variant);
  font-size: var(--text-body);
  line-height: 1.7;
}
.update-manual-download .text-action {
  justify-self: start;
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.update-manual-url {
  color: var(--md-sys-color-on-surface-variant);
  overflow-wrap: anywhere;
  user-select: all;
  font-size: var(--text-small);
}
.update-manual-download .update-manual-error {
  color: #b34232;
}
</style>
