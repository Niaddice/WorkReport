import { readonly, ref } from 'vue'

export type MessageTone = 'info' | 'success' | 'warning' | 'error'

export interface MessageAction {
  label: string
  run: () => void
  /** 执行动作后保留消息，默认关闭。 */
  keepOpen?: boolean
}

export interface MessageOptions {
  text: string
  tone?: MessageTone
  title?: string
  actions?: MessageAction[]
  /** 0 表示不自动关闭，需用户操作。 */
  duration?: number
}

export interface AppMessage {
  id: number
  tone: MessageTone
  title: string
  text: string
  actions: MessageAction[]
  duration: number
}

export const DEFAULT_MESSAGE_DURATION: Record<MessageTone, number> = {
  info: 3200,
  success: 3200,
  warning: 5200,
  error: 6500,
}

const MAX_MESSAGES = 4
export const messages = ref<AppMessage[]>([])
const timers = new Map<number, ReturnType<typeof setTimeout>>()
let nextId = 1

function clearTimer(id: number) {
  const timer = timers.get(id)
  if (timer === undefined) return
  clearTimeout(timer)
  timers.delete(id)
}

function schedule(id: number, duration: number) {
  clearTimer(id)
  if (duration <= 0) return
  timers.set(
    id,
    setTimeout(() => dismissMessage(id), duration),
  )
}

/** 顶部 Message：同一时间最多保留 4 条，相同文案会刷新计时而不是重复堆叠。 */
export function pushMessage(options: MessageOptions): number {
  const tone = options.tone ?? 'info'
  const text = options.text.trim()
  if (!text) return 0
  const existing = messages.value.find((message) => message.text === text && message.tone === tone)
  if (existing) {
    schedule(existing.id, options.duration ?? existing.duration)
    return existing.id
  }
  const message: AppMessage = {
    id: nextId++,
    tone,
    title: options.title?.trim() ?? '',
    text,
    actions: options.actions ?? [],
    duration: options.duration ?? DEFAULT_MESSAGE_DURATION[tone],
  }
  messages.value = [...messages.value.slice(-(MAX_MESSAGES - 1)), message]
  // 被挤出队列的旧消息也要清掉计时器。
  for (const id of [...timers.keys()]) {
    if (!messages.value.some((item) => item.id === id)) clearTimer(id)
  }
  schedule(message.id, message.duration)
  return message.id
}

export function dismissMessage(id: number) {
  clearTimer(id)
  messages.value = messages.value.filter((message) => message.id !== id)
}

export function clearMessages() {
  for (const id of [...timers.keys()]) clearTimer(id)
  messages.value = []
}

export function useMessages() {
  return {
    messages: readonly(messages),
    pushMessage,
    dismissMessage,
    clearMessages,
  }
}
