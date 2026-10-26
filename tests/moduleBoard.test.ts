// @vitest-environment happy-dom
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { computed, nextTick, ref } from 'vue'
import dayjs from 'dayjs'
import ModuleBoard from '../src/components/ModuleBoard.vue'
import { MODULE_CONTEXT } from '../src/modules/context'
import type { LayoutNode, ModuleId } from '../src/modules/types'

beforeAll(() => {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver
})

let spies: ReturnType<typeof vi.spyOn>[] = []
afterEach(() => {
  for (const spy of spies) spy.mockRestore()
  spies = []
})

function fakeContext() {
  const noop = () => {}
  return {
    journal: {
      workspace: ref(null),
      entries: ref([{ date: '20261024', filename: '20261024.md', format: 'md' }]),
      busy: ref(false),
      saving: ref(false),
      selectedDate: ref('20261024'),
      previews: ref<Record<string, string>>({}),
      loadPreviews: vi.fn(async () => {}),
      reportError: noop,
    },
    ai: {},
    holidays: {
      state: ref({ years: [], failures: [] }),
      loading: ref(false),
      syncing: ref(false),
      error: ref(''),
    },
    now: ref(dayjs('2026-10-25')),
    today: computed(() => '20261025'),
    storageMode: computed(() => 'files' as const),
    canRestore: computed(() => false),
    disabled: computed(() => false),
    rangeDates: ref<string[]>([]),
    rangeDocuments: ref([]),
    rangeLoading: ref(false),
    rangeError: ref(''),
    editorPreview: ref(false),
    journalPreview: {
      available: computed(() => false),
      visible: ref(false),
      sourceKey: computed(() => ''),
      scrollPosition: ref(null),
      toggle: noop,
    },
    calendar: ref(null),
    editor: ref(null),
    actions: {
      selectDate: vi.fn(async () => {}),
      loadRange: vi.fn(async () => {}),
      requestSummary: noop,
      requestFormat: noop,
      requestReload: noop,
      requestRestore: noop,
      openHolidaySettings: noop,
      openWorkspaceDirectory: noop,
      syncHolidays: noop,
    },
  }
}

function leaf(id: ModuleId): LayoutNode {
  return { type: 'leaf', id }
}
const rowLayout: LayoutNode = {
  type: 'split',
  direction: 'row',
  ratio: 0.5,
  first: leaf('overview'),
  second: leaf('history'),
}

function mountBoard(layout: LayoutNode = rowLayout, width = 800, height = 600) {
  spies.push(
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(function (this: HTMLElement) {
      return this.classList.contains('module-board') ? width : 0
    }),
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockImplementation(function (this: HTMLElement) {
      return this.classList.contains('module-board') ? height : 0
    }),
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
      const isBoard = this.classList.contains('module-board')
      const w = isBoard ? width : 0
      const h = isBoard ? height : 0
      return { left: 0, top: 0, right: w, bottom: h, width: w, height: h, x: 0, y: 0, toJSON: () => ({}) } as DOMRect
    }),
  )
  return mount(ModuleBoard, {
    props: { layout },
    global: { provide: { [MODULE_CONTEXT as symbol]: fakeContext() as never } },
    attachTo: document.body,
  })
}

function pointer(type: string, x: number, y: number, buttons: number) {
  return new PointerEvent(type, {
    bubbles: true,
    clientX: x,
    clientY: y,
    buttons,
    pointerId: 1,
    isPrimary: true,
    pointerType: 'mouse',
  })
}

describe('module board', () => {
  it('positions modules absolutely from the split tree without scrolling', async () => {
    const wrapper = mountBoard()
    await flushPromises()
    const slots = wrapper.findAll('.module-slot')
    expect(slots).toHaveLength(2)
    expect(slots[0]!.attributes('style')).toContain('left: 0px')
    expect(slots[0]!.attributes('style')).toContain('width: 396px')
    expect(slots[1]!.attributes('style')).toContain('left: 404px')
    expect(slots[1]!.attributes('style')).toContain('width: 396px')
    expect(wrapper.find('.module-gutter.is-row').exists()).toBe(true)
    wrapper.unmount()
  })

  it('renders nested top/bottom splits and their gutters', async () => {
    const layout: LayoutNode = {
      type: 'split',
      direction: 'column',
      ratio: 0.5,
      first: leaf('overview'),
      second: leaf('history'),
    }
    const wrapper = mountBoard(layout)
    await flushPromises()
    const slots = wrapper.findAll('.module-slot')
    expect(slots[0]!.attributes('style')).toContain('height: 296px')
    expect(slots[1]!.attributes('style')).toContain('top: 304px')
    expect(wrapper.find('.module-gutter.is-column').exists()).toBe(true)
    wrapper.unmount()
  })

  it('forwards remove requests from the panel chrome', async () => {
    const wrapper = mountBoard()
    await flushPromises()
    await wrapper.find('.module-panel-remove').trigger('click')
    expect(wrapper.emitted('remove')).toEqual([['overview']])
    wrapper.unmount()
  })

  it('shows a drop preview and emits the split side when dropping on another module', async () => {
    const wrapper = mountBoard()
    await flushPromises()
    const handle = wrapper.findAll('.module-drag-handle')[0]!
    handle.element.dispatchEvent(pointer('pointerdown', 30, 30, 1))
    window.dispatchEvent(pointer('pointermove', 30, 30, 1))
    window.dispatchEvent(pointer('pointermove', 700, 300, 1))
    await nextTick()
    expect(document.body.classList.contains('is-module-dragging')).toBe(true)
    expect(wrapper.find('.module-drop-preview').text()).toBe('右侧分栏')
    window.dispatchEvent(pointer('pointerup', 700, 300, 0))
    await flushPromises()
    expect(wrapper.emitted('split')).toEqual([['overview', 'history', 'right']])
    expect(document.body.classList.contains('is-module-dragging')).toBe(false)
    wrapper.unmount()
  })

  it('drops a module into the gutter between two modules', async () => {
    const tree: LayoutNode = {
      type: 'split',
      direction: 'row',
      ratio: 0.5,
      first: leaf('overview'),
      second: {
        type: 'split',
        direction: 'row',
        ratio: 0.5,
        first: leaf('history'),
        second: leaf('calendar'),
      },
    }
    const wrapper = mountBoard(tree)
    await flushPromises()
    const handle = wrapper.findAll('.module-drag-handle')[0]!
    handle.element.dispatchEvent(pointer('pointerdown', 30, 30, 1))
    window.dispatchEvent(pointer('pointermove', 30, 30, 1))
    // 第二个 split 的间隙位于 598~606px。
    window.dispatchEvent(pointer('pointermove', 602, 300, 1))
    await nextTick()
    expect(wrapper.find('.module-insert-preview').text()).toBe('插入此处')
    expect(wrapper.find('.module-drop-preview').exists()).toBe(false)
    window.dispatchEvent(pointer('pointerup', 602, 300, 0))
    await flushPromises()
    expect(wrapper.emitted('insert')).toEqual([['overview', [1]]])
    expect(document.body.classList.contains('is-module-dragging')).toBe(false)
    wrapper.unmount()
  })

  it('cancels a drag with Escape without splitting', async () => {
    const wrapper = mountBoard()
    await flushPromises()
    const handle = wrapper.findAll('.module-drag-handle')[0]!
    handle.element.dispatchEvent(pointer('pointerdown', 30, 30, 1))
    window.dispatchEvent(pointer('pointermove', 30, 30, 1))
    window.dispatchEvent(pointer('pointermove', 700, 300, 1))
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    window.dispatchEvent(pointer('pointerup', 700, 300, 0))
    await flushPromises()
    expect(wrapper.emitted('split')).toBeUndefined()
    expect(document.body.classList.contains('is-module-dragging')).toBe(false)
    wrapper.unmount()
  })

  it('emits a resized ratio after dragging the split gutter', async () => {
    const wrapper = mountBoard()
    await flushPromises()
    const gutter = wrapper.find('.module-gutter')
    gutter.element.dispatchEvent(pointer('pointerdown', 396, 300, 1))
    window.dispatchEvent(pointer('pointermove', 500, 300, 1))
    window.dispatchEvent(pointer('pointerup', 500, 300, 0))
    await flushPromises()
    expect(wrapper.emitted('resize')).toHaveLength(1)
    const [path, ratio] = wrapper.emitted('resize')![0] as [number[], number]
    expect(path).toEqual([])
    expect(ratio).toBeCloseTo(500 / 792, 3)
    wrapper.unmount()
  })

  it('supports keyboard gutter resizing', async () => {
    const wrapper = mountBoard()
    await flushPromises()
    await wrapper.find('.module-gutter').trigger('keydown', { key: 'ArrowRight' })
    const [path, ratio] = wrapper.emitted('resize')![0] as [number[], number]
    expect(path).toEqual([])
    expect(ratio).toBeCloseTo(0.53, 2)
    wrapper.unmount()
  })

  it('maximizes a module above a backdrop and restores it', async () => {
    const wrapper = mountBoard()
    await flushPromises()
    await wrapper.find('.module-maximize-button').trigger('click')
    expect(wrapper.find('.module-maximize-backdrop').exists()).toBe(true)
    expect(wrapper.find('.module-panel.is-maximized').exists()).toBe(true)
    expect(document.body.classList.contains('is-module-maximized')).toBe(true)
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await flushPromises()
    expect(wrapper.find('.module-maximize-backdrop').exists()).toBe(false)
    expect(wrapper.find('.module-panel.is-maximized').exists()).toBe(false)
    expect(document.body.classList.contains('is-module-maximized')).toBe(false)
    wrapper.unmount()
  })
})
