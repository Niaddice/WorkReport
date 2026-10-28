<script setup lang="ts">
import { ref, type Component } from 'vue'
import { PhCornersIn, PhCornersOut, PhDotsSixVertical, PhQuestion, PhX } from '@phosphor-icons/vue'
import ModuleHelpPopover from './ModuleHelpPopover.vue'
import type { ModuleId } from '../modules/types'

withDefaults(
  defineProps<{
    id: ModuleId
    title: string
    icon: Component
    /** 只剩一个模块时不允许关闭。 */
    canRemove: boolean
    dragging: boolean
    /** 当前正在操作的模块。 */
    active?: boolean
    /** 当前是否以浮层最大化显示。 */
    maximized?: boolean
    /** 使用说明，标题栏的帮助按钮据此展示。 */
    help?: readonly string[]
  }>(),
  { active: false, maximized: false, help: () => [] },
)
const emit = defineEmits<{
  activate: []
  'drag-start': [event: PointerEvent]
  remove: []
  'toggle-maximize': []
}>()

const helpPopover = ref<InstanceType<typeof ModuleHelpPopover>>()
const helpButton = ref<HTMLButtonElement>()

function onPointerDown(event: PointerEvent) {
  if (event.button !== 0) return
  emit('drag-start', event)
}
function openHelp() {
  helpPopover.value?.toggle()
}
</script>

<template>
  <section
    class="module-panel"
    :class="{ 'is-active': active, 'is-dragging': dragging, 'is-maximized': maximized }"
    :data-module-id="id"
    :role="maximized ? 'dialog' : undefined"
    :aria-modal="maximized ? 'true' : undefined"
    :aria-label="maximized ? title : undefined"
    @pointerdown.capture="emit('activate')"
    @click.capture="emit('activate')"
    @focusin="emit('activate')"
  >
    <header class="module-panel-header">
      <button
        type="button"
        class="module-drag-handle"
        :aria-label="`拖动「${title}」模块到其他模块上分栏`"
        aria-roledescription="可拖动模块"
        :disabled="maximized"
        title="拖动到其他模块上分栏"
        @pointerdown="onPointerDown"
      >
        <PhDotsSixVertical :size="15" aria-hidden="true" />
      </button>
      <component :is="icon" class="module-panel-icon" :size="15" aria-hidden="true" />
      <h2 class="module-panel-title">{{ title }}</h2>
      <slot name="subtitle" />
      <div class="module-panel-actions">
        <button
          v-if="help.length"
          ref="helpButton"
          type="button"
          class="module-panel-action module-help-button"
          :aria-label="`查看「${title}」使用说明`"
          :title="helpPopover?.opened ? undefined : '使用说明'"
          aria-haspopup="dialog"
          :aria-expanded="helpPopover?.opened ?? false"
          :aria-controls="helpPopover?.opened ? helpPopover.id : undefined"
          @click="openHelp"
        >
          <PhQuestion :size="15" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="module-panel-action module-maximize-button"
          :aria-label="maximized ? `还原「${title}」模块大小` : `放大「${title}」模块`"
          :title="maximized ? '还原模块大小（Esc）' : '放大到整个窗口（Esc 退出）'"
          @click="emit('toggle-maximize')"
        >
          <PhCornersIn v-if="maximized" :size="15" aria-hidden="true" />
          <PhCornersOut v-else :size="15" aria-hidden="true" />
        </button>
        <button
          v-if="canRemove && !maximized"
          type="button"
          class="module-panel-action module-panel-remove"
          :aria-label="`从首页移除「${title}」模块`"
          title="移除模块"
          @click="emit('remove')"
        >
          <PhX :size="15" aria-hidden="true" />
        </button>
      </div>
    </header>
    <div class="module-panel-body">
      <slot />
    </div>
    <ModuleHelpPopover ref="helpPopover" :title="title" :help="help" :anchor="helpButton" />
  </section>
</template>
