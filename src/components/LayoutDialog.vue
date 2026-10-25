<script setup lang="ts">
import { onBeforeUnmount, ref } from 'vue'
import { PhArrowsInSimple, PhSquaresFour, PhX } from '@phosphor-icons/vue'
import { createDialogMotion } from '../lib/dialogMotion'

const emit = defineEmits<{ reset: []; close: [] }>()
const dialog = ref<HTMLDialogElement>()
const motion = createDialogMotion(() => dialog.value)

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
        <md-icon-button aria-label="关闭布局面板" @click="emit('close')"><PhX :size="18" /></md-icon-button>
      </div>
      <p>
        将模块拖到其他模块上，可生成左右或上下分栏；拖动模块之间的间隙，可调整两侧模块大小。拖动只影响间隙两侧相邻的两个模块，其他模块尺寸不变；极少数混合嵌套间隙会按整块区域缩放。首页始终一屏展示，内容在模块内部滚动。
      </p>
    </header>
    <div class="layout-dialog-tips">
      <p>
        <strong>分栏</strong
        >把拖动手柄拖到目标模块的左/右/上/下侧，松开即可分栏；拖到两个模块之间的滑块上，可把模块插入到两者之间。
      </p>
      <p><strong>调整比例</strong>拖动模块间的间隙，或聚焦间隙后按方向键微调。</p>
    </div>
    <footer class="layout-dialog-actions">
      <button type="button" class="text-action" @click="emit('reset')">
        <PhArrowsInSimple :size="15" aria-hidden="true" />恢复默认布局
      </button>
      <button type="button" class="text-action" @click="emit('close')">完成</button>
    </footer>
  </dialog>
</template>
