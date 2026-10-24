<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useId, watch } from 'vue'
import { PhCaretLeft, PhCaretRight, PhMinus, PhPlus } from '@phosphor-icons/vue'
import {
  buildExcelRegionGrid,
  excelCellContentHeight,
  excelCellCss,
  excelColumnName,
  excelGridCellContainsColumn,
  excelPreviewZoom,
  initialExcelSheet,
  nextExcelSheetIndex,
  type ExcelGridCell,
  type ExcelSheetPreviewData,
} from '../lib/excelPreview'
import './excelSheetPreview.css'

const props = defineProps<{ sheets: ExcelSheetPreviewData[]; previewId: string }>()
const instanceId = useId()
const activeSheetName = ref('')
const activeRegionIndex = ref(0)
const zoomMode = ref<'fit' | number>('fit')
const selected = ref<{ row: number; column: number } | null>(null)
const scroller = ref<HTMLElement>()
const sheetTabs = ref<HTMLElement>()
const viewportWidth = ref(0)
let observer: ResizeObserver | undefined

const activeSheet = computed(
  () => props.sheets.find((sheet) => sheet.name === activeSheetName.value) ?? props.sheets[0],
)
const regionGrids = computed(() => activeSheet.value?.regions.map(buildExcelRegionGrid) ?? [])
const grid = computed(() => regionGrids.value[activeRegionIndex.value] ?? regionGrids.value[0])
const scale = computed(() => excelPreviewZoom(zoomMode.value, viewportWidth.value, grid.value?.width ?? 0))
const zoomLabel = computed(() => `${Math.round(scale.value * 100)}%`)
const selectedCell = computed(() =>
  grid.value?.rows
    .flatMap((row) => row.cells)
    .find((cell) => cell.row === selected.value?.row && cell.column === selected.value?.column),
)
const selectedAddress = computed(
  () =>
    selectedCell.value?.source?.address ??
    (selected.value ? `${excelColumnName(selected.value.column)}${selected.value.row}` : ''),
)
const formulaText = computed(() => {
  const source = selectedCell.value?.source
  return source?.formula
    ? source.formula.startsWith('=')
      ? source.formula
      : `=${source.formula}`
    : (source?.text ?? '')
})
const isTruncated = computed(() => activeSheet.value?.truncated || grid.value?.truncated)

function updateViewport() {
  viewportWidth.value = scroller.value?.clientWidth ?? 0
}

function resetRegion() {
  selected.value = grid.value?.rows[0]?.cells[0]
    ? { row: grid.value.rows[0]!.cells[0]!.row, column: grid.value.rows[0]!.cells[0]!.column }
    : null
  if (scroller.value) {
    scroller.value.scrollLeft = 0
    scroller.value.scrollTop = 0
  }
  void nextTick(updateViewport)
}

watch(
  () => props.previewId,
  () => {
    activeSheetName.value = initialExcelSheet(props.sheets)
    activeRegionIndex.value = 0
    zoomMode.value = 'fit'
  },
  { immediate: true },
)
watch(
  () => props.sheets,
  (sheets) => {
    if (!sheets.some((sheet) => sheet.name === activeSheetName.value)) activeSheetName.value = initialExcelSheet(sheets)
  },
)
watch(activeSheetName, () => {
  activeRegionIndex.value = 0
})
watch([grid, () => props.previewId], resetRegion, { immediate: true, flush: 'post' })

function selectSheet(name: string) {
  activeSheetName.value = name
}
function onSheetKeydown(event: KeyboardEvent, index: number) {
  const next = nextExcelSheetIndex(index, props.sheets.length, event.key)
  if (next === null) return
  event.preventDefault()
  const sheet = props.sheets[next]
  if (!sheet) return
  selectSheet(sheet.name)
  void nextTick(() => sheetTabs.value?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus())
}

function selectCell(cell: ExcelGridCell) {
  selected.value = { row: cell.row, column: cell.column }
}
function onCellKeydown(event: KeyboardEvent, cell: ExcelGridCell) {
  if (!grid.value || !['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) return
  if (event.altKey || event.ctrlKey || event.metaKey) return
  event.preventDefault()
  let target: ExcelGridCell | undefined
  const rows = grid.value.rows
  const rowIndex = rows.findIndex((row) => row.index === cell.row)
  const currentRow = rows[rowIndex]
  if (!currentRow) return
  const cellIndex = currentRow.cells.indexOf(cell)
  if (event.key === 'ArrowLeft') target = currentRow.cells[Math.max(0, cellIndex - 1)]
  if (event.key === 'ArrowRight') target = currentRow.cells[Math.min(currentRow.cells.length - 1, cellIndex + 1)]
  if (event.key === 'Home') target = currentRow.cells[0]
  if (event.key === 'End') target = currentRow.cells[currentRow.cells.length - 1]
  if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
    const direction = event.key === 'ArrowUp' ? -1 : 1
    for (let index = rowIndex + direction; index >= 0 && index < rows.length; index += direction) {
      target = rows[index]?.cells.find((candidate) => excelGridCellContainsColumn(grid.value!, candidate, cell.column))
      if (target) break
    }
  }
  if (!target) return
  selectCell(target)
  const address = `${target.row}:${target.column}`
  void nextTick(() =>
    scroller.value?.querySelector<HTMLElement>(`[data-grid-cell="${address}"]`)?.focus({ preventScroll: false }),
  )
}

function changeZoom(direction: -1 | 1) {
  const current = Math.round(scale.value * 100)
  zoomMode.value = Math.max(
    0.4,
    Math.min(2, (direction > 0 ? Math.floor(current / 10 + 1) * 10 : Math.ceil(current / 10 - 1) * 10) / 100),
  )
}

onMounted(() => {
  updateViewport()
  if (typeof ResizeObserver !== 'undefined') {
    observer = new ResizeObserver(updateViewport)
    if (scroller.value) observer.observe(scroller.value)
  }
})
onBeforeUnmount(() => observer?.disconnect())
</script>

<template>
  <div class="excel-workbook-preview">
    <div class="excel-sheet-toolbar">
      <div class="excel-sheet-region-control">
        <template v-if="regionGrids.length > 1">
          <button
            type="button"
            aria-label="上一个预览区域"
            :disabled="activeRegionIndex === 0"
            @click="activeRegionIndex--"
          >
            <PhCaretLeft :size="13" aria-hidden="true" />
          </button>
          <span :title="`区域 ${activeRegionIndex + 1} / ${regionGrids.length}`"
            >{{ grid?.address }} <small>{{ activeRegionIndex + 1 }} / {{ regionGrids.length }}</small></span
          >
          <button
            type="button"
            aria-label="下一个预览区域"
            :disabled="activeRegionIndex >= regionGrids.length - 1"
            @click="activeRegionIndex++"
          >
            <PhCaretRight :size="13" aria-hidden="true" />
          </button>
        </template>
        <span v-else class="excel-sheet-readonly">只读预览</span>
      </div>
      <div class="excel-sheet-zoom" aria-label="预览缩放">
        <button type="button" aria-label="缩小预览" :disabled="scale <= 0.4" @click="changeZoom(-1)">
          <PhMinus :size="13" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="excel-sheet-zoom-value"
          :aria-label="`预览缩放 ${zoomLabel}，点击恢复 100%`"
          @click="zoomMode = 1"
        >
          {{ zoomLabel }}
        </button>
        <button type="button" aria-label="放大预览" :disabled="scale >= 2" @click="changeZoom(1)">
          <PhPlus :size="13" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="excel-sheet-fit"
          :class="{ 'is-active': zoomMode === 'fit' }"
          :aria-pressed="zoomMode === 'fit'"
          @click="zoomMode = 'fit'"
        >
          适应宽度
        </button>
      </div>
    </div>
    <div class="excel-sheet-formula-bar" aria-label="当前单元格内容">
      <span class="excel-sheet-address" aria-label="单元格位置">{{ selectedAddress || '—' }}</span>
      <span class="excel-sheet-fx" aria-hidden="true">fx</span>
      <span class="excel-sheet-formula-value" :title="formulaText">{{ formulaText }}</span>
      <span v-if="selectedCell?.source?.changed" class="excel-sheet-change-label">已调整</span>
    </div>
    <p v-if="isTruncated" class="excel-sheet-window-note">
      模板较大，当前展示主要区域；可切换区域查看其余变更，或查看填充明细。
    </p>
    <div
      :id="`${instanceId}-sheet-panel`"
      ref="scroller"
      role="tabpanel"
      :aria-label="`${activeSheet?.name ?? ''} 工作表预览`"
      class="excel-sheet-scroll"
      tabindex="0"
    >
      <div v-if="grid?.columns.length && grid.rows.length" class="excel-sheet-canvas" :style="{ zoom: scale }">
        <table class="excel-sheet-grid" :style="{ width: `${grid.width}px` }" aria-readonly="true">
          <caption class="sr-only">
            {{
              activeSheet?.name
            }}，{{
              grid.address
            }}，只读预览。方向键可浏览单元格。
          </caption>
          <colgroup>
            <col :style="{ width: '44px' }" />
            <col v-for="column in grid.columns" :key="column.index" :style="{ width: `${column.width}px` }" />
          </colgroup>
          <thead>
            <tr>
              <th class="excel-sheet-corner" aria-hidden="true"></th>
              <th
                v-for="column in grid.columns"
                :key="column.index"
                scope="col"
                :class="{ 'is-selected': selected?.column === column.index }"
              >
                {{ column.label }}
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in grid.rows" :key="row.index" :style="{ height: `${row.height}px` }">
              <th scope="row" class="excel-sheet-row-number" :class="{ 'is-selected': selected?.row === row.index }">
                {{ row.index }}
              </th>
              <td
                v-for="cell in row.cells"
                :key="cell.column"
                :rowspan="cell.rowSpan > 1 ? cell.rowSpan : undefined"
                :colspan="cell.columnSpan > 1 ? cell.columnSpan : undefined"
                :style="{ ...excelCellCss(cell.source?.style), height: `${cell.height}px` }"
                :data-grid-cell="`${cell.row}:${cell.column}`"
                :class="{
                  'is-changed': cell.source?.changed,
                  'is-selected': selected?.row === cell.row && selected?.column === cell.column,
                }"
                :tabindex="selected?.row === cell.row && selected?.column === cell.column ? 0 : -1"
                :aria-label="`${cell.source?.address ?? `${excelColumnName(cell.column)}${cell.row}`} ${cell.source?.text ?? ''}${cell.source?.changed ? '，已调整' : ''}`"
                @click="selectCell(cell)"
                @focus="selectCell(cell)"
                @keydown="onCellKeydown($event, cell)"
              >
                <div class="excel-sheet-cell-content" :style="{ maxHeight: `${excelCellContentHeight(cell)}px` }">
                  {{ cell.source?.text || '\u00a0' }}
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <div v-else class="excel-sheet-empty">此工作表没有可预览的单元格，请查看填充明细。</div>
    </div>
    <footer class="excel-sheet-footer">
      <div ref="sheetTabs" class="excel-sheet-tabs" role="tablist" aria-label="工作表">
        <button
          v-for="(sheet, index) in sheets"
          :key="sheet.name"
          type="button"
          role="tab"
          :aria-selected="activeSheetName === sheet.name"
          :aria-controls="`${instanceId}-sheet-panel`"
          :tabindex="activeSheetName === sheet.name ? 0 : -1"
          :class="{ 'is-active': activeSheetName === sheet.name }"
          @click="selectSheet(sheet.name)"
          @keydown="onSheetKeydown($event, index)"
        >
          {{ sheet.name }}<small v-if="sheet.state !== 'visible'">已隐藏</small>
        </button>
      </div>
      <span class="excel-sheet-legend"><i aria-hidden="true"></i>填充或格式调整</span>
    </footer>
  </div>
</template>
