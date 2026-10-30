<script setup lang="ts">
import { onBeforeUnmount, ref } from 'vue'
import { PhArrowsInSimple, PhSquaresFour, PhTrashSimple, PhX } from '@phosphor-icons/vue'
import { createDialogMotion } from '../lib/dialogMotion'
import type { SavedLayout } from '../modules/types'
import { LAYOUT_PRESETS } from '../modules/layoutPresets'

defineProps<{ savedLayouts: SavedLayout[] }>()
const emit = defineEmits<{
  reset: []
  save: [name: string]
  apply: [name: string]
  preset: [id: string]
  remove: [name: string]
  close: []
}>()

const dialog = ref<HTMLDialogElement>()
const motion = createDialogMotion(() => dialog.value)
const name = ref('')

function save() {
  const value = name.value.trim()
  if (!value) return
  emit('save', value)
  name.value = ''
}

function open() {
  motion.open()
}
function close() {
  return motion.close()
}
function isOpen() {
  return !!dialog.value?.open
}
defineExpose({ open, close, isOpen })
onBeforeUnmount(() => motion.dispose())
</script>

<template>
  <dialog ref="dialog" class="layout-dialog" aria-labelledby="layout-dialog-title" @cancel.prevent="emit('close')">
    <header class="layout-dialog-heading">
      <div>
        <PhSquaresFour :size="22" aria-hidden="true" />
        <h2 id="layout-dialog-title">布局</h2>
        <md-icon-button aria-label="关闭布局面板" data-window-close="default" @click="emit('close')"
          ><PhX :size="18"
        /></md-icon-button>
      </div>
      <p>
        将模块拖到其他模块上，可生成左右或上下分栏；拖动模块之间的间隙，可调整两侧模块大小。拖动只影响接触该边界的模块；一侧有上下分栏时，上下模块会同步调整宽度，非相邻模块的位置和尺寸保持不变。首页始终一屏展示，内容在模块内部滚动。
      </p>
    </header>
    <div class="layout-dialog-tips">
      <p>
        <strong>分栏</strong
        >按住模块标题栏空白处或模块图标，拖到目标模块的左/右/上/下侧，松开即可分栏；拖到两个模块之间的间隙中，可把模块插入到两者之间。标题文字和功能按钮不触发拖动。
      </p>
      <p><strong>调整比例</strong>拖动模块间的间隙，或聚焦间隙后按方向键微调。</p>
    </div>

    <section class="layout-presets" aria-label="推荐布局">
      <h3>推荐布局</h3>
      <button v-for="item in LAYOUT_PRESETS" :key="item.id" type="button" @click="emit('preset', item.id)">
        <strong>{{ item.name }}</strong
        ><span>{{ item.description }}</span>
      </button>
      <p>切换前可先保存当前布局，方便随时恢复。</p>
    </section>
    <section class="layout-dialog-save" aria-label="保存布局">
      <h3>保存布局</h3>
      <div class="layout-dialog-save-row">
        <input
          v-model="name"
          type="text"
          placeholder="输入布局名称"
          maxlength="50"
          aria-label="布局名称"
          @keydown.enter="save"
        />
        <button type="button" class="text-action" :disabled="!name.trim()" @click="save">保存当前布局</button>
      </div>
    </section>

    <section class="layout-dialog-saved" aria-label="已保存布局">
      <h3>已保存布局</h3>
      <p v-if="!savedLayouts.length" class="layout-dialog-saved-empty">还没有保存的布局。</p>
      <ul v-else class="layout-dialog-saved-list">
        <li v-for="item in savedLayouts" :key="item.name">
          <button
            type="button"
            class="layout-dialog-saved-item"
            :title="`切换到「${item.name}」`"
            @click="emit('apply', item.name)"
          >
            {{ item.name }}
          </button>
          <button
            type="button"
            class="layout-dialog-saved-remove"
            aria-label="删除布局"
            title="删除布局"
            @click="emit('remove', item.name)"
          >
            <PhTrashSimple :size="14" aria-hidden="true" />
          </button>
        </li>
      </ul>
    </section>

    <footer class="layout-dialog-actions">
      <button type="button" class="text-action" @click="emit('reset')">
        <PhArrowsInSimple :size="15" aria-hidden="true" />恢复默认布局
      </button>
      <button type="button" class="text-action" data-window-close @click="emit('close')">完成</button>
    </footer>
  </dialog>
</template>

<style scoped>
.layout-presets {
  padding: 16px 20px;
  display: grid;
  gap: 10px;
}
.layout-presets h3 {
  font-size: var(--text-subtitle);
  font-weight: 500;
  margin: 0;
}
.layout-presets button {
  display: grid;
  gap: 6px;
  padding: 14px;
  text-align: left;
  border: 1px solid var(--skin-border);
  border-radius: 10px;
  background: var(--skin-surface);
  color: var(--skin-text);
  font: inherit;
  cursor: pointer;
}
.layout-presets button:hover {
  border-color: var(--skin-accent);
  background: var(--skin-accent-soft);
}
.layout-presets button:focus-visible {
  outline: 2px solid var(--skin-accent);
  outline-offset: 2px;
}
.layout-presets button strong {
  font-weight: 500;
}
.layout-presets button span,
.layout-presets p {
  font-size: var(--text-small);
  color: var(--skin-text-muted);
}
.layout-presets p {
  margin: 0;
}
</style>
