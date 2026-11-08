<script setup lang="ts">
import { onBeforeUnmount, ref } from 'vue'
import { invoke, isTauri } from '@tauri-apps/api/core'
import { PhKey } from '@phosphor-icons/vue'

const props = defineProps<{ disabled: boolean }>()
const opened = ref(false)
const busy = ref(false)
const error = ref('')
const entries = ref<{ url: string; username: string }[]>([])
const confirmReset = ref(false)
let revision = 0
onBeforeUnmount(() => {
  revision++
})
async function load() {
  if (props.disabled || busy.value) return
  opened.value = true
  error.value = ''
  if (!isTauri()) {
    error.value = '请在桌面应用中管理已保存的 Git 凭据。'
    return
  }
  const current = ++revision
  busy.value = true
  try {
    const values = await invoke<{ url: string; username: string }[]>('list_git_credentials')
    if (current === revision) entries.value = values
  } catch (cause) {
    if (current === revision) error.value = String(cause)
  } finally {
    if (current === revision) busy.value = false
  }
}
async function forget(origin: string | null) {
  if (props.disabled || busy.value) return
  const current = ++revision
  busy.value = true
  error.value = ''
  try {
    await invoke('forget_git_credential', { origin })
    if (current !== revision) return
    entries.value = entries.value.filter((entry) => origin !== null && entry.url !== origin)
    confirmReset.value = false
  } catch (cause) {
    if (current === revision) error.value = String(cause)
  } finally {
    if (current === revision) busy.value = false
  }
}
</script>

<template>
  <section class="saved-git-credentials" aria-label="本机 Git 凭据">
    <div class="saved-git-heading">
      <h4>本机保存的 Git 凭据</h4>
      <button type="button" class="text-action" :disabled="disabled || busy" @click="load">
        {{ opened ? '刷新列表' : '管理已保存凭据' }}
      </button>
    </div>
    <p>凭据加密保存在本机配置文件中，由系统安全存储保管密钥，可供本机的其他工作空间使用。</p>
    <template v-if="opened">
      <p v-if="busy" role="status">正在处理凭据…</p>
      <div v-else-if="!entries.length && !error" class="settings-empty-card">
        <div class="settings-empty-icon">
          <PhKey :size="22" aria-hidden="true" />
        </div>
        <p class="settings-empty-title">尚未保存 Git 凭据</p>
        <p class="settings-empty-desc">在 Git 操作需要身份认证时，勾选“保存凭据”后会自动加密保存在本机。</p>
      </div>
      <ul v-if="entries.length">
        <li v-for="entry in entries" :key="entry.url">
          <span
            ><strong>{{ entry.url }}</strong
            ><small>{{ entry.username }}</small></span
          >
          <button
            type="button"
            class="text-action"
            :aria-label="`清除 ${entry.url} 的 Git 凭据`"
            :disabled="disabled || busy"
            @click="forget(entry.url)"
          >
            清除
          </button>
        </li>
      </ul>
      <p v-if="error" class="error-text" role="alert">{{ error }}</p>
      <div v-if="entries.length || error" class="saved-git-reset">
        <template v-if="confirmReset">
          <span>清除全部后，下次认证需重新输入。</span>
          <button type="button" class="text-action" :disabled="disabled || busy" @click="confirmReset = false">
            取消
          </button>
          <button type="button" class="text-action" :disabled="disabled || busy" @click="forget(null)">
            确认清除全部
          </button>
        </template>
        <button v-else type="button" class="text-action" :disabled="disabled || busy" @click="confirmReset = true">
          清除全部已保存凭据
        </button>
      </div>
    </template>
  </section>
</template>

<style scoped>
.saved-git-credentials {
  display: grid;
  gap: 12px;
  padding-top: 20px;
  border-top: 1px solid var(--skin-border);
}
.saved-git-heading,
.saved-git-credentials li,
.saved-git-reset {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.saved-git-heading,
.saved-git-credentials li {
  justify-content: space-between;
}
.saved-git-credentials h4,
.saved-git-credentials p {
  margin: 0;
}
.saved-git-credentials h4 {
  font-size: var(--text-subtitle);
  font-weight: 500;
}
.saved-git-credentials p,
.saved-git-credentials small,
.saved-git-reset {
  color: var(--skin-text-muted);
  font-size: var(--text-small);
  line-height: 1.7;
}
.saved-git-credentials ul {
  list-style: none;
  display: grid;
  gap: 10px;
  padding: 0;
  margin: 0;
}
.saved-git-credentials li {
  padding: 10px;
  background: var(--skin-canvas);
  border-radius: 8px;
}
.saved-git-credentials li span {
  display: grid;
  gap: 3px;
  min-width: 0;
  overflow-wrap: anywhere;
}
.saved-git-credentials strong {
  font-size: var(--text-body);
  font-weight: 500;
}
</style>
