<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, shallowRef, useId, type CSSProperties } from 'vue'
import { installButtonTooltips, type ButtonTooltipTarget } from '../lib/buttonTooltips'
import { buttonTooltipPosition, type TooltipPlacement } from '../lib/tooltipPosition'

const id = `button-tooltip-${useId()}`
const target = shallowRef<ButtonTooltipTarget | null>(null)
const element = ref<HTMLElement>()
const placement = ref<TooltipPlacement>('top')
const style = ref<CSSProperties>({ visibility: 'hidden' })
const supportsPopover = typeof HTMLElement !== 'undefined' && typeof HTMLElement.prototype.showPopover === 'function'
let dispose: (() => void) | undefined
let revision = 0
let describedAnchor: HTMLElement | undefined

function removeDescription() {
  const anchor = describedAnchor
  describedAnchor = undefined
  if (!anchor) return
  const ids = (anchor.getAttribute('aria-describedby') ?? '').split(/\s+/).filter((value) => value && value !== id)
  if (ids.length) anchor.setAttribute('aria-describedby', ids.join(' '))
  else anchor.removeAttribute('aria-describedby')
}

function hide() {
  revision++
  removeDescription()
  target.value = null
}

async function show(next: ButtonTooltipTarget) {
  const current = ++revision
  if (describedAnchor !== next.anchor) removeDescription()
  target.value = next
  style.value = { visibility: 'hidden' }
  await nextTick()
  const tip = element.value
  if (!tip || current !== revision || !next.anchor.isConnected) return
  let topLayer = false
  if (supportsPopover) {
    try {
      if (!tip.matches(':popover-open')) tip.showPopover()
      topLayer = true
    } catch {
      // Keep the dialog-local fallback available in older WebViews.
    }
  }
  const viewport = { width: document.documentElement.clientWidth || window.innerWidth, height: window.innerHeight }
  tip.style.maxWidth = `${Math.max(0, Math.min(280, viewport.width - 16))}px`
  tip.style.maxHeight = `${Math.max(0, viewport.height - 16)}px`
  tip.style.left = '0px'
  tip.style.top = '0px'
  const position = buttonTooltipPosition(
    next.anchor.getBoundingClientRect(),
    { width: tip.offsetWidth, height: tip.offsetHeight },
    viewport,
  )
  // A transformed native dialog can contain fixed descendants. Top-layer
  // popovers use viewport coordinates; the fallback accounts for its origin.
  const origin = !topLayer ? tip.getBoundingClientRect() : { left: 0, top: 0 }
  placement.value = position.placement
  style.value = {
    visibility: 'visible',
    left: `${position.left - origin.left}px`,
    top: `${position.top - origin.top}px`,
    maxWidth: `${Math.max(0, Math.min(280, viewport.width - 16))}px`,
    maxHeight: `${Math.max(0, viewport.height - 16)}px`,
    '--tooltip-arrow-offset': `${position.arrow}px`,
  }
  const descriptions = new Set((next.anchor.getAttribute('aria-describedby') ?? '').split(/\s+/).filter(Boolean))
  descriptions.add(id)
  next.anchor.setAttribute('aria-describedby', [...descriptions].join(' '))
  describedAnchor = next.anchor
}

onMounted(() => {
  dispose = installButtonTooltips({
    show: (value) => {
      void show(value)
    },
    hide,
  })
})
onBeforeUnmount(() => {
  dispose?.()
  hide()
})
</script>

<template>
  <Teleport v-if="target" :to="target.portal">
    <div
      :id="id"
      ref="element"
      class="button-tooltip"
      :data-placement="placement"
      role="tooltip"
      :popover="supportsPopover ? 'manual' : undefined"
      :style="style"
    >
      <span class="button-tooltip-text">{{ target.text }}</span>
      <span class="button-tooltip-arrow" aria-hidden="true"></span>
    </div>
  </Teleport>
</template>
