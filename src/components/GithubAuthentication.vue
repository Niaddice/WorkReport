<script setup lang="ts">
import { onBeforeUnmount, watch } from 'vue'
import { PhGithubLogo, PhArrowSquareOut, PhSpinnerGap } from '@phosphor-icons/vue'
import { useGithubLogin } from '../lib/useGithubLogin'
import { api } from '../lib/workspace'

const props = defineProps<{ disabled: boolean }>()
const emit = defineEmits<{ authorized: []; busy: [value: boolean] }>()
const { supported, progress, busy, cancelling, error, start, cancel, openBrowser } = useGithubLogin(
  () => props.disabled,
  () => emit('authorized'),
)
watch(busy, (value) => emit('busy', value), { immediate: true })
onBeforeUnmount(() => emit('busy', false))
async function copyCode() {
  if (!progress.value?.code) return
  try {
    await api.copy(progress.value.code)
  } catch (cause) {
    error.value = `复制授权码失败：${String(cause)}`
  }
}
</script>

<template>
  <section class="github-authentication" aria-label="GitHub 浏览器登录" :aria-busy="busy">
    <div class="github-authentication-heading">
      <PhGithubLogo :size="20" aria-hidden="true" /><strong>GitHub 浏览器授权</strong>
    </div>
    <p>在浏览器登录 GitHub 并授权，应用会加密保存凭据，用于之后的 GitHub HTTPS 拉取和推送。</p>
    <small>需要 GitHub CLI（gh）。授权由 GitHub CLI 发起，其登录状态也会保留在本机。</small>
    <p v-if="!supported">请在桌面应用中登录 GitHub。</p>
    <div v-if="busy && progress?.code" class="github-device-code">
      <span>在 GitHub 授权页面输入一次性授权码</span>
      <code aria-label="GitHub 一次性授权码">{{ progress.code }}</code>
      <button type="button" class="text-action" :disabled="cancelling" @click="copyCode">复制授权码</button>
      <button type="button" class="text-action" :disabled="cancelling" @click="openBrowser">
        <PhArrowSquareOut :size="14" aria-hidden="true" />打开授权页面
      </button>
    </div>
    <p v-if="progress" :role="progress.phase === 'failed' ? 'alert' : 'status'">
      {{ progress.message }}<span v-if="progress.username">（{{ progress.username }}）</span>
    </p>
    <p v-if="error" class="settings-error" role="alert">{{ error }}</p>
    <div class="github-authentication-actions">
      <button v-if="!busy" type="button" class="settings-save-button" :disabled="disabled || !supported" @click="start">
        <PhGithubLogo :size="16" aria-hidden="true" />{{
          progress?.phase === 'authorized' ? '重新登录 GitHub' : '使用浏览器登录 GitHub'
        }}
      </button>
      <template v-else>
        <span role="status"
          ><PhSpinnerGap :size="15" class="spin" aria-hidden="true" />{{
            cancelling ? '正在取消…' : '等待浏览器授权…'
          }}</span
        >
        <button type="button" class="text-action" :disabled="cancelling" @click="cancel">取消授权等待</button>
      </template>
    </div>
  </section>
</template>
