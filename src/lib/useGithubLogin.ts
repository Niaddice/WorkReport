import { invoke, isTauri } from '@tauri-apps/api/core'
import { onBeforeUnmount, ref, watch } from 'vue'

export interface GithubLoginProgress {
  requestId: string
  phase: 'waiting' | 'authorized' | 'failed' | 'cancelled'
  code: string | null
  username: string | null
  message: string
}

/** Public progress only; the native layer stores the token before authorizing. */
export function useGithubLogin(disabled: () => boolean, authorized: () => void) {
  const supported = isTauri()
  const progress = ref<GithubLoginProgress | null>(null)
  const busy = ref(false)
  const cancelling = ref(false)
  const error = ref('')
  let active = true
  let revision = 0
  let requestId = ''
  let timer: ReturnType<typeof setTimeout> | undefined
  let starting: Promise<GithubLoginProgress> | undefined

  async function cancelRequest(id: string) {
    await invoke('cancel_github_login', { requestId: id })
  }
  function stop() {
    clearTimeout(timer)
    timer = undefined
  }
  function accept(value: GithubLoginProgress, token: number) {
    if (!active || revision !== token || requestId !== value.requestId) return
    progress.value = value
    if (value.phase === 'waiting') {
      timer = setTimeout(() => void poll(token), 1000)
      return
    }
    stop()
    requestId = ''
    busy.value = false
    if (value.phase === 'authorized') authorized()
  }
  async function poll(token: number) {
    const id = requestId
    if (!active || revision !== token || !id) return
    try {
      const value = await invoke<GithubLoginProgress>('read_github_login', { requestId: id })
      if (!value || value.requestId !== id) throw new Error('GitHub 登录会话已变化，请重新登录。')
      accept(value, token)
    } catch (cause) {
      if (!active || revision !== token) return
      error.value = `读取 GitHub 登录状态失败：${String(cause)}`
      requestId = ''
      busy.value = false
      void cancelRequest(id).catch(() => {})
    }
  }
  async function start() {
    if (!supported || busy.value || disabled()) return
    const token = ++revision
    stop()
    progress.value = null
    error.value = ''
    busy.value = true
    const task = invoke<GithubLoginProgress>('start_github_login')
    starting = task
    try {
      const value = await task
      if (!value?.requestId) throw new Error('GitHub 登录未返回有效会话。')
      if (!active || revision !== token) return
      requestId = value.requestId
      accept(value, token)
    } catch (cause) {
      if (active && revision === token) {
        error.value = `GitHub 登录失败：${String(cause)}`
        busy.value = false
      }
    } finally {
      if (starting === task) starting = undefined
    }
  }
  async function cancel() {
    const token = ++revision
    stop()
    const pending = starting
    const id = requestId
    requestId = ''
    cancelling.value = true
    try {
      const resolvedId = id || (pending ? (await pending).requestId : '')
      if (resolvedId) await cancelRequest(resolvedId)
      if (active && revision === token) {
        progress.value = null
        error.value = ''
      }
    } catch (cause) {
      if (active && revision === token) error.value = `取消 GitHub 登录失败：${String(cause)}`
    } finally {
      if (active && revision === token) {
        busy.value = false
        cancelling.value = false
      }
    }
  }
  async function openBrowser() {
    if (!supported || !busy.value || cancelling.value || !progress.value?.code) return
    error.value = ''
    try {
      await invoke('open_github_login_browser')
    } catch (cause) {
      if (active) error.value = String(cause)
    }
  }
  watch(disabled, (value) => {
    if (value && busy.value && !cancelling.value) void cancel()
  })
  onBeforeUnmount(() => {
    active = false
    void cancel()
  })
  return { supported, progress, busy, cancelling, error, start, cancel, openBrowser }
}
