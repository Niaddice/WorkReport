<script setup lang="ts">
import { type Component } from 'vue'
import { PhCheckCircle, PhInfo, PhWarning, PhX, PhXCircle } from '@phosphor-icons/vue'
import { dismissMessage, messages, type AppMessage, type MessageAction, type MessageTone } from '../lib/messages'

const icons: Record<MessageTone, Component> = {
  info: PhInfo,
  success: PhCheckCircle,
  warning: PhWarning,
  error: PhXCircle,
}

function runAction(message: AppMessage, action: MessageAction) {
  action.run()
  if (!action.keepOpen) dismissMessage(message.id)
}
</script>

<template>
  <div class="message-host" aria-label="通知">
    <TransitionGroup name="message">
      <div
        v-for="message in messages"
        :key="message.id"
        class="app-message"
        :class="`is-${message.tone}`"
        :role="message.tone === 'error' ? 'alert' : 'status'"
        :aria-live="message.tone === 'error' ? 'assertive' : 'polite'"
      >
        <component :is="icons[message.tone]" class="app-message-icon" :size="18" aria-hidden="true" />
        <div class="app-message-copy">
          <strong v-if="message.title">{{ message.title }}</strong>
          <p>{{ message.text }}</p>
        </div>
        <div v-if="message.actions.length || message.duration === 0" class="app-message-actions">
          <button
            v-for="action in message.actions"
            :key="action.label"
            type="button"
            class="app-message-action"
            @click="runAction(message, action)"
          >
            {{ action.label }}
          </button>
          <button
            type="button"
            class="app-message-close"
            aria-label="关闭提示"
            title="关闭提示"
            @click="dismissMessage(message.id)"
          >
            <PhX :size="14" aria-hidden="true" />
          </button>
        </div>
        <button
          v-else
          type="button"
          class="app-message-close"
          aria-label="关闭提示"
          title="关闭提示"
          @click="dismissMessage(message.id)"
        >
          <PhX :size="14" aria-hidden="true" />
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>
