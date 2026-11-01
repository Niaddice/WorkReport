<script setup lang="ts">
import { computed } from 'vue'
import dayjs from 'dayjs'
import { PhArrowClockwise, PhDownloadSimple } from '@phosphor-icons/vue'
import type { AppUpdater } from '../lib/useUpdater'
import UpdateDownloadFallback from './UpdateDownloadFallback.vue'

const props = defineProps<{ updater: AppUpdater }>()
const statusText = computed(() => {
  const update = props.updater
  if (!update.supported) return '请在桌面 App 中检查更新'
  if (update.checking.value) return '正在检查更新…'
  if (update.status.value === 'installed') return '新版本已安装，等待重新启动'
  if (update.release.value) return `发现新版本 ${update.release.value.version}`
  if (update.status.value === 'latest') return '当前已是最新版本'
  if (update.status.value === 'unpublished') return '暂未发布支持自动更新的版本'
  if (update.status.value === 'error') return '暂时无法检查更新'
  return '启动后自动检查更新'
})
</script>

<template>
  <div class="update-settings" :aria-busy="updater.checking.value">
    <div class="settings-section-heading">
      <div>
        <h3>WorkReport {{ updater.currentVersion }}</h3>
        <p role="status">{{ statusText }}</p>
      </div>
      <button
        type="button"
        class="settings-save-button"
        :disabled="
          !updater.supported || updater.checking.value || updater.busy.value || updater.status.value === 'installed'
        "
        @click="updater.checkForUpdates()"
      >
        <PhArrowClockwise :size="15" :class="{ spin: updater.checking.value }" aria-hidden="true" />
        {{ updater.checking.value ? '正在检查…' : '检查更新' }}
      </button>
    </div>
    <button v-if="updater.release.value" type="button" class="text-action" @click="updater.open()">
      <PhDownloadSimple :size="16" aria-hidden="true" />查看更新说明
    </button>
    <p v-if="updater.error.value" class="settings-error" role="alert">{{ updater.error.value }}</p>
    <UpdateDownloadFallback :updater="updater" />
    <div class="settings-details-list">
      <div><strong>更新方式</strong><span>启动后后台检查，发现新版本时提醒，由你选择更新时间。</span></div>
      <div><strong>安装前保存</strong><span>更新前自动保存日报和设置，保存失败或任务未完成时暂停安装。</span></div>
      <div v-if="updater.lastChecked.value">
        <strong>最近检查</strong><span>{{ dayjs(updater.lastChecked.value).format('YYYY 年 M 月 D 日 HH:mm') }}</span>
      </div>
    </div>
  </div>
</template>
