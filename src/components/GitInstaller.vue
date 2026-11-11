<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { invoke, isTauri } from '@tauri-apps/api/core'
import { PhArrowClockwise, PhDownloadSimple, PhSpinnerGap } from '@phosphor-icons/vue'

interface GitDetection {
  installed: boolean
  version: string
  path: string
  platform: 'windows' | 'macos' | 'other'
  method: 'winget' | 'homebrew' | 'xcode' | 'unavailable'
}
interface GitInstallation {
  status: 'installed' | 'pending'
  executable: string
  version: string
  message: string
}

const props = defineProps<{ executable: string; disabled: boolean }>()
const emit = defineEmits<{ installed: [executable: string]; busy: [value: boolean] }>()
const supported = isTauri()
const detection = ref<GitDetection | null>(null)
const checking = ref(false)
const installing = ref(false)
const notice = ref('')
const error = ref('')
let active = true
let revision = 0

const methodLabel = computed(
  () =>
    ({
      winget: '通过 Windows 程序包管理器（winget）安装 Git for Windows，安装过程中可能弹出管理员权限确认。',
      homebrew: '通过 Homebrew 安装 Git（brew install git）。',
      xcode: '未检测到 Homebrew，将打开 macOS 命令行开发者工具安装窗口，其中包含 Git。',
      unavailable: '',
    })[detection.value?.method ?? 'unavailable'],
)
const canInstall = computed(
  () => supported && !!detection.value && detection.value.method !== 'unavailable' && !props.disabled,
)
const unsupportedHint = computed(() => {
  if (!supported) return '请在桌面应用中使用一键安装。'
  if (detection.value?.method === 'unavailable')
    return detection.value.platform === 'windows'
      ? '未检测到 winget，请从 https://git-scm.com/download/win 下载安装 Git。'
      : '当前系统暂不支持一键安装，请使用系统包管理器安装 Git。'
  return ''
})

async function detect(keepMessages = false) {
  if (!supported || checking.value) return
  const token = ++revision
  checking.value = true
  if (!keepMessages) {
    error.value = ''
    notice.value = ''
  }
  try {
    const value = await invoke<GitDetection | undefined>('detect_git', { executable: props.executable })
    if (active && token === revision && value) detection.value = value
  } catch (cause) {
    if (active && token === revision) error.value = `检测 Git 失败：${String(cause)}`
  } finally {
    if (token === revision) checking.value = false
  }
}
async function install() {
  if (!canInstall.value || installing.value) return
  installing.value = true
  notice.value = ''
  error.value = ''
  try {
    const result = await invoke<GitInstallation>('install_git')
    if (!active) return
    notice.value = result.message
    if (result.status === 'installed' && result.executable) {
      emit('installed', result.executable)
      notice.value += ' 已填入 Git 程序路径，保存设置后生效。'
    }
  } catch (cause) {
    if (active) error.value = String(cause)
  } finally {
    installing.value = false
    if (active) {
      revision++
      checking.value = false
      void detect(true)
    }
  }
}
watch(installing, (value) => emit('busy', value), { immediate: true })
defineExpose({ detect })
onBeforeUnmount(() => {
  active = false
  emit('busy', false)
})
</script>

<template>
  <section class="git-installer" aria-label="一键安装 Git" :aria-busy="installing">
    <div class="git-installer-heading">
      <strong>一键安装 Git</strong>
      <span v-if="checking && !detection" role="status">正在检测…</span>
      <span v-else-if="detection?.installed" class="git-installer-ok" role="status">
        已检测到 {{ detection.version.replace(/^git version\s*/, 'Git ') }}
      </span>
      <span v-else-if="detection" class="git-installer-missing" role="status">未检测到可用的 Git</span>
    </div>
    <small v-if="detection?.installed && detection.path" class="git-installer-path" :title="detection.path">
      {{ detection.path }}
    </small>
    <small v-if="methodLabel && !detection?.installed">{{ methodLabel }}</small>
    <small v-if="unsupportedHint">{{ unsupportedHint }}</small>
    <p v-if="installing" role="status" class="git-installer-progress">
      <PhSpinnerGap :size="15" class="spin" aria-hidden="true" />正在安装 Git，请勿关闭应用，可能需要几分钟…
    </p>
    <p v-if="notice" role="status" class="git-installer-notice">{{ notice }}</p>
    <p v-if="error" role="alert" class="settings-error">{{ error }}</p>
    <div class="git-installer-actions">
      <button type="button" class="settings-save-button" :disabled="!canInstall || installing" @click="install">
        <PhDownloadSimple :size="16" aria-hidden="true" />{{
          detection?.installed ? '重新安装 / 更新 Git' : '一键安装 Git'
        }}
      </button>
      <button type="button" class="text-action" :disabled="!supported || checking || installing" @click="detect()">
        <PhArrowClockwise :size="14" aria-hidden="true" />重新检测
      </button>
    </div>
  </section>
</template>

<style scoped>
.git-installer {
  display: grid;
  gap: 8px;
  margin-bottom: 16px;
  padding-bottom: 16px;
  border-bottom: 1px solid var(--skin-border);
  font-size: var(--text-body);
  color: var(--skin-text);
}
.git-installer-heading,
.git-installer-actions,
.git-installer-progress {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
}
.git-installer-heading span,
.git-installer small,
.git-installer-notice {
  margin: 0;
  color: var(--skin-text-muted);
  font-size: var(--text-small);
  line-height: 1.7;
}
.git-installer-ok {
  color: var(--skin-accent) !important;
}
.git-installer-path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.git-installer-progress {
  margin: 0;
  font-size: var(--text-small);
}
.text-action {
  display: flex;
  align-items: center;
  gap: 6px;
}
</style>
