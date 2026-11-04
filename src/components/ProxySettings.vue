<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { isTauri } from '@tauri-apps/api/core'
import { PhArrowClockwise, PhPlugsConnected } from '@phosphor-icons/vue'
import { cloneProxySettings, proxyApi, validateProxySettings } from '../lib/proxy'
import AppSelect from './AppSelect.vue'

const props = defineProps<{ busy: boolean }>()
const supported = isTauri()
const baseline = ref(cloneProxySettings())
const draft = ref(cloneProxySettings())
const loading = ref(true)
const saving = ref(false)
const testing = ref(false)
const error = ref('')
const needsSave = ref(false)
const testMessage = ref('')
const testError = ref('')
const unchanged = computed(() => !needsSave.value && JSON.stringify(draft.value) === JSON.stringify(baseline.value))
const disabled = computed(() => !supported || props.busy || loading.value || saving.value)
const options = [
  { value: 'system', label: '跟随系统' },
  { value: 'direct', label: '直连' },
  { value: 'manual', label: '手动代理' },
]
let active = true
let testRevision = 0
onBeforeUnmount(() => {
  active = false
  testRevision++
})
onMounted(async () => {
  if (!supported) {
    loading.value = false
    return
  }
  try {
    const settings = await proxyApi.load()
    if (!active) return
    baseline.value = cloneProxySettings(settings)
    draft.value = cloneProxySettings(settings)
  } catch (cause) {
    if (active) {
      error.value = `读取代理配置失败：${String(cause)}；可重新填写后保存。`
    }
  } finally {
    if (active) loading.value = false
  }
})
watch(
  draft,
  () => {
    error.value = ''
    testMessage.value = ''
    testError.value = ''
    testRevision++
  },
  { deep: true, flush: 'sync' },
)
function validate() {
  error.value = validateProxySettings(draft.value)
  return !error.value
}
let pending: Promise<boolean> | undefined
function save(): Promise<boolean> {
  if (pending) return pending
  if (unchanged.value) return Promise.resolve(true)
  if (!supported || loading.value || !validate()) return Promise.resolve(false)
  const snapshot = cloneProxySettings(draft.value)
  saving.value = true
  pending = (async () => {
    try {
      const committed = await proxyApi.save(snapshot)
      if (!active) return false
      const edited = JSON.stringify(snapshot) !== JSON.stringify(draft.value)
      baseline.value = cloneProxySettings(committed)
      if (!edited) draft.value = cloneProxySettings(committed)
      needsSave.value = false
      if (!unchanged.value) error.value = '代理配置还有未保存的修改，请再次保存。'
      return unchanged.value
    } catch (cause) {
      if (active) error.value = `保存代理配置失败：${String(cause)}`
      return false
    } finally {
      saving.value = false
      pending = undefined
    }
  })()
  return pending
}
function reset() {
  draft.value = cloneProxySettings(baseline.value)
  needsSave.value = false
  error.value = ''
}
function restoreDefaults() {
  draft.value = cloneProxySettings()
  needsSave.value = true
}
async function test() {
  if (disabled.value || testing.value || !validate()) return
  const revision = ++testRevision
  testing.value = true
  testMessage.value = ''
  testError.value = ''
  try {
    const message = await proxyApi.test(draft.value)
    if (active && revision === testRevision) testMessage.value = message
  } catch (cause) {
    if (active && revision === testRevision) testError.value = String(cause)
  } finally {
    if (active) testing.value = false
  }
}
defineExpose({ unchanged, loading, saving, save, reset, validate, error })
</script>

<template>
  <div class="proxy-settings" :aria-busy="loading || saving">
    <div class="settings-section-heading">
      <div>
        <h3>本机网络代理</h3>
        <p>独立于工作空间保存。点击底部“保存设置”后，后续网络请求生效。</p>
      </div>
    </div>
    <p v-if="loading" role="status">正在读取代理配置…</p>
    <p v-if="!supported" role="status">请在桌面应用中配置网络代理。</p>
    <div class="proxy-fields">
      <label for="proxy-mode">连接方式</label>
      <AppSelect id="proxy-mode" v-model="draft.mode" :options="options" :disabled="disabled" />
      <p class="field-hint" role="status">
        {{
          draft.mode === 'manual'
            ? '使用指定代理，可设置直连地址。'
            : draft.mode === 'direct'
              ? '直接连接，忽略代理环境变量。'
              : '使用系统代理或当前进程的代理环境变量。'
        }}
      </p>
      <template v-if="draft.mode === 'manual'">
        <label for="proxy-url">代理地址</label>
        <input
          id="proxy-url"
          v-model="draft.url"
          type="text"
          placeholder="http://127.0.0.1:7890"
          autocomplete="off"
          spellcheck="false"
          :maxlength="2048"
          :disabled="disabled"
          aria-describedby="proxy-url-hint"
        />
        <p id="proxy-url-hint" class="field-hint">支持 HTTP、HTTPS、SOCKS5 和 SOCKS5H；地址不包含账号密码。</p>
        <label for="proxy-bypass">绕过代理的地址</label>
        <input
          id="proxy-bypass"
          v-model="draft.noProxy"
          type="text"
          placeholder="localhost,127.0.0.1,::1,.example.com"
          autocomplete="off"
          spellcheck="false"
          :maxlength="4096"
          :disabled="disabled"
          aria-describedby="proxy-bypass-hint"
        />
        <p id="proxy-bypass-hint" class="field-hint">
          用逗号分隔域名、IP 或 CIDR 网段，例如 .example.com、192.168.0.0/16。
        </p>
      </template>
    </div>
    <div class="proxy-actions">
      <button type="button" class="text-action" :disabled="disabled" @click="restoreDefaults">恢复默认</button>
      <button type="button" class="settings-save-button" :disabled="disabled || testing" @click="test">
        <PhArrowClockwise v-if="testing" :size="16" class="spin" aria-hidden="true" />
        <PhPlugsConnected v-else :size="16" aria-hidden="true" />
        {{ testing ? '正在测试…' : '测试连接' }}
      </button>
      <span class="field-hint">使用当前填写的配置连接 GitHub API；测试不会保存设置。</span>
    </div>
    <p v-if="error" class="settings-error" role="alert">{{ error }}</p>
    <p v-if="testError" class="settings-error" role="alert">{{ testError }}</p>
    <p v-if="testMessage" role="status">{{ testMessage }}</p>
    <div class="settings-details-list">
      <div>
        <strong>应用请求</strong
        ><span>用于 AI 接口、获取模型列表、Git HTTP/HTTPS、应用更新与放假信息同步。本机 AI 接口始终直连。</span>
      </div>
      <div>
        <strong>CLI 调用</strong
        ><span>手动代理和直连模式会同步代理环境变量；是否使用由 CLI 自身决定。Git SSH 和数据库连接沿用各自配置。</span>
      </div>
    </div>
  </div>
</template>
