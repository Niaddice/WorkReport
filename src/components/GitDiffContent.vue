<script setup lang="ts">
import type { GitDiffLine } from '../lib/gitDiff'

const props = defineProps<{
  lines: GitDiffLine[]
  mode?: 'stage' | 'unstage' | null
  disabled?: boolean
}>()

const emit = defineEmits<{
  (e: 'stage-hunk', patch: string): void
  (e: 'unstage-hunk', patch: string): void
  (e: 'discard-hunk', patch: string): void
}>()

function onHunkAction(patch: string) {
  if (props.mode === 'stage') {
    emit('stage-hunk', patch)
  } else if (props.mode === 'unstage') {
    emit('unstage-hunk', patch)
  }
}
</script>

<template>
  <pre
    class="diff-content"
  ><code v-for="(line, index) in lines" :key="index" :class="`diff-${line.kind}`"><span class="diff-line-number" aria-hidden="true">{{ line.oldLine ?? '' }}</span><span class="diff-line-number" aria-hidden="true">{{ line.newLine ?? '' }}</span><span class="diff-line-text"><span>{{ line.text || ' ' }}</span><span
    v-if="mode && line.kind === 'hunk' && line.patch"
    class="hunk-actions"
  ><button
    type="button"
    class="hunk-action-btn"
    :disabled="disabled"
    :title="mode === 'stage' ? '暂存此代码块到暂存区' : '将此代码块从暂存区撤出'"
    :aria-label="mode === 'stage' ? '暂存此代码块' : '取消暂存此代码块'"
    @click.stop="onHunkAction(line.patch)"
  >{{ mode === 'stage' ? '暂存此块' : '取消暂存' }}</button><button
    v-if="mode === 'stage'"
    type="button"
    class="hunk-action-btn danger"
    :disabled="disabled"
    title="从工作区丢弃此代码块（无法撤销）"
    aria-label="丢弃此代码块"
    @click.stop="emit('discard-hunk', line.patch)"
  >丢弃此块</button></span></span></code></pre>
</template>

<style scoped>
.diff-content {
  font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', monospace;
  font-size: calc(12 * var(--font-unit));
  line-height: 1.6;
  margin: 0;
  padding: 4px 0;
  tab-size: 4;
}
.diff-content code {
  display: grid;
  grid-template-columns: 4ch 4ch auto;
  width: max-content;
  min-width: 100%;
  padding-right: 14px;
  font: inherit;
}
.diff-line-number {
  position: sticky;
  left: 0;
  z-index: 1;
  background: var(--skin-canvas);
  color: var(--skin-text-muted);
  user-select: none;
  text-align: right;
  padding-right: 1ch;
  border-right: 1px solid var(--skin-border);
}
.diff-line-number + .diff-line-number {
  left: 4ch;
}
.diff-add .diff-line-number {
  background: var(--skin-success-soft);
}
.diff-remove .diff-line-number {
  background: var(--skin-danger-soft);
}
.diff-hunk .diff-line-number {
  background: var(--skin-accent-soft);
}
.diff-line-text {
  padding-left: 12px;
}
.diff-hunk .diff-line-text {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding-right: 8px;
}
.hunk-actions {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  position: sticky;
  right: 8px;
  z-index: 2;
}
.hunk-action-btn {
  font-family: var(--font-sans, system-ui, sans-serif);
  font-size: calc(11 * var(--font-unit));
  line-height: 1.2;
  padding: 1px 8px;
  border-radius: var(--radius-sm, 4px);
  border: 1px solid var(--skin-border);
  background: var(--skin-surface);
  color: var(--skin-text);
  cursor: pointer;
  white-space: nowrap;
  user-select: none;
  transition:
    background 0.15s,
    border-color 0.15s,
    color 0.15s;
}
.hunk-action-btn:hover:not(:disabled) {
  background: var(--skin-accent-soft);
  border-color: var(--skin-accent);
  color: var(--skin-accent);
}
.hunk-action-btn.danger {
  color: var(--skin-danger);
}
.hunk-action-btn.danger:hover:not(:disabled) {
  background: var(--skin-danger-soft);
  border-color: var(--skin-danger);
  color: var(--skin-danger);
}
.hunk-action-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.diff-add {
  color: var(--skin-success);
  background: var(--skin-success-soft);
}
.diff-remove {
  color: var(--skin-danger);
  background: var(--skin-danger-soft);
}
.diff-hunk {
  color: var(--skin-accent);
  background: var(--skin-accent-soft);
}
.diff-meta {
  color: var(--skin-text-muted);
}
</style>
