<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, useId } from 'vue'

const props = defineProps<{ url: string; operation: string }>()
const emit = defineEmits<{ confirm: [value: { username: string; password: string; save?: boolean }]; cancel: [] }>()
const id = useId()
const username = ref('')
const password = ref('')
const save = ref(false)
const input = ref<HTMLInputElement>()
const action = computed(
  () => ({ fetch: 'Fetch 获取', pull: 'Pull 拉取', push: 'Push 推送' })[props.operation] ?? 'Git 操作',
)
function submit() {
  if (!username.value.trim() || !password.value) return
  const value = { username: username.value.trim(), password: password.value, ...(save.value ? { save: true } : {}) }
  password.value = ''
  emit('confirm', value)
}
onMounted(() => input.value?.focus())
onBeforeUnmount(() => {
  password.value = ''
  username.value = ''
})
</script>

<template>
  <form
    class="git-authentication"
    aria-label="Git 远程认证"
    @submit.prevent="submit"
    @keydown.esc.prevent.stop="emit('cancel')"
  >
    <strong>登录远程仓库后重试 {{ action }}</strong>
    <p class="git-authentication-server">{{ url }}</p>
    <label :for="`${id}-username`">用户名</label>
    <input
      :id="`${id}-username`"
      ref="input"
      v-model="username"
      autocomplete="username"
      aria-label="Git 用户名"
      maxlength="512"
      required
    />
    <label :for="`${id}-password`">密码或访问令牌</label>
    <input
      :id="`${id}-password`"
      v-model="password"
      type="password"
      autocomplete="current-password"
      aria-label="Git 密码或访问令牌"
      maxlength="8192"
      required
    />
    <label class="git-authentication-remember" :for="`${id}-remember`">
      <input :id="`${id}-remember`" v-model="save" type="checkbox" aria-label="保存 Git 凭据到本机" />
      保存凭据（加密保存在本机）
    </label>
    <small>{{
      save
        ? '认证成功后加密保存，后续优先自动使用，失效后再提示认证。可在 Git 设置中清除。'
        : '仅用于本次操作，不保存凭据。'
    }}</small>
    <small v-if="url.startsWith('http://')">该服务器使用 HTTP，凭据将通过未加密连接发送。</small>
    <div class="git-authentication-actions">
      <button type="button" @click="emit('cancel')">取消</button>
      <button type="submit" :disabled="!username.trim() || !password">认证并重试</button>
    </div>
  </form>
</template>

<style scoped>
.git-authentication {
  flex: 0 0 auto;
  max-height: min(360px, 60%);
  overflow: auto;
  display: grid;
  gap: 8px;
  padding: 14px;
  margin: 8px 12px;
  border: 1px solid var(--skin-border);
  border-radius: 10px;
  background: var(--skin-canvas);
  color: var(--skin-text-body);
  font-size: var(--text-body);
}
.git-authentication-server {
  margin: 0;
  overflow-wrap: anywhere;
}
.git-authentication input:not([type='checkbox']) {
  min-width: 0;
  width: 100%;
  padding: 7px 9px;
  border: 1px solid var(--skin-border);
  border-radius: 6px;
  background: var(--skin-surface);
  color: var(--skin-text);
  font: inherit;
}
.git-authentication-remember {
  display: flex;
  align-items: center;
  gap: 8px;
}
.git-authentication-remember input {
  flex: 0 0 auto;
  margin: 0;
}
.git-authentication small {
  color: var(--skin-text-muted);
}
.git-authentication-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.git-authentication button {
  padding: 6px 10px;
  border: 1px solid var(--skin-border);
  border-radius: 6px;
  background: var(--skin-surface);
  color: var(--skin-accent);
  font: inherit;
}
.git-authentication button:disabled {
  opacity: 0.45;
}
</style>
