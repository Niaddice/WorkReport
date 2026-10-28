// @vitest-environment happy-dom
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { computed, nextTick, ref } from 'vue'
import dayjs from 'dayjs'
import ModuleBoard from '../src/components/ModuleBoard.vue'
import { resizeLayoutLocally } from '../src/modules/layout'
import { MODULE_CONTEXT } from '../src/modules/context'
import type { LayoutMetrics, LayoutNode, ModuleId } from '../src/modules/types'

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

function mountBoard(layout: LayoutNode = rowLayout, width = 800, height = 600, stubs: Record<string, boolean> = {}) {
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
    global: { provide: { [MODULE_CONTEXT as symbol]: fakeContext() as never }, stubs },
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
  it('activates the calendar from its content, keyboard focus and rail reveal like every other module', async () => {
    const wrapper = mountBoard({ ...rowLayout, first: leaf('calendar'), second: leaf('overview') })
    const activeIds = () =>
      wrapper.findAll('.module-panel.is-active').map((panel) => panel.attributes('data-module-id'))
    try {
      await flushPromises()
      await wrapper.get('[data-module-id="overview"] .module-panel-body').trigger('pointerdown')
      expect(activeIds()).toEqual(['overview'])
      await wrapper.get('[data-module-id="calendar"] .calendar-toolbar').trigger('pointerdown')
      expect(activeIds()).toEqual(['calendar'])

      await wrapper.get('[data-module-id="overview"] .module-panel-body').trigger('click')
      expect(activeIds()).toEqual(['overview'])
      wrapper.get<HTMLElement>('[data-module-id="calendar"] .module-maximize-button').element.focus()
      await nextTick()
      expect(activeIds()).toEqual(['calendar'])

      wrapper.vm.reveal('overview')
      await nextTick()
      wrapper.vm.reveal('calendar')
      await nextTick()
      expect(activeIds()).toEqual(['calendar'])
      expect(document.activeElement).toBe(wrapper.get('[data-module-id="calendar"]').element)
    } finally {
      wrapper.unmount()
    }
  })

  it('keeps one active module across pointer, keyboard, rail focus and layout changes', async () => {
    const wrapper = mountBoard()
    const activeIds = () =>
      wrapper.findAll('.module-panel.is-active').map((panel) => panel.attributes('data-module-id'))
    try {
      await flushPromises()
      expect(activeIds()).toEqual(['overview'])
      await wrapper.get('[data-module-id="history"] .module-panel-body').trigger('pointerdown')
      expect(activeIds()).toEqual(['history'])

      wrapper.get<HTMLElement>('[data-module-id="overview"] .module-maximize-button').element.focus()
      await nextTick()
      expect(activeIds()).toEqual(['overview'])
      wrapper.vm.reveal('history')
      await nextTick()
      expect(activeIds()).toEqual(['history'])

      await wrapper.setProps({ layout: { ...rowLayout, first: leaf('history'), second: leaf('overview') } })
      expect(activeIds()).toEqual(['history'])
      await wrapper.setProps({ layout: leaf('overview') })
      expect(activeIds()).toEqual(['overview'])
      await wrapper.setProps({ layout: rowLayout })
      wrapper.vm.reveal('history')
      await nextTick()
      await wrapper.setProps({ workspacePath: '/tmp/another-workspace' })
      expect(activeIds()).toEqual(['overview'])
    } finally {
      wrapper.unmount()
    }
  })

  it('positions modules absolutely from the split tree without scrolling', async () => {
    const wrapper = mountBoard()
    await flushPromises()
    const slots = wrapper.findAll('.module-slot')
    expect(slots).toHaveLength(2)
    expect(slots[0]!.attributes('style')).toContain('left: 0px')
    expect(parseFloat((slots[0]!.element as HTMLElement).style.width)).toBeCloseTo(398.4)
    expect(parseFloat((slots[1]!.element as HTMLElement).style.left)).toBeCloseTo(401.6)
    expect(parseFloat((slots[1]!.element as HTMLElement).style.width)).toBeCloseTo(398.4)
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
    expect(parseFloat((slots[0]!.element as HTMLElement).style.height)).toBeCloseTo(298.4)
    expect(parseFloat((slots[1]!.element as HTMLElement).style.top)).toBeCloseTo(301.6)
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

  it.each(['.module-panel-header', '.module-panel-icon', '.module-panel-actions'])(
    'previews and applies a split when dragging from %s',
    async (selector) => {
      const wrapper = mountBoard()
      try {
        await flushPromises()
        const source = wrapper.get(`[data-module-id="overview"] ${selector}`)
        source.element.dispatchEvent(pointer('pointerdown', 150, 20, 1))
        window.dispatchEvent(pointer('pointermove', 700, 300, 1))
        await nextTick()
        expect(document.body.classList.contains('is-module-dragging')).toBe(true)
        expect(wrapper.find('.module-drop-preview').text()).toBe('右侧分栏')
        window.dispatchEvent(pointer('pointerup', 700, 300, 0))
        await flushPromises()
        expect(wrapper.emitted('split')).toEqual([['overview', 'history', 'right']])
        expect(document.body.classList.contains('is-module-dragging')).toBe(false)
      } finally {
        wrapper.unmount()
      }
    },
  )

  it('keeps a header click or slight pointer movement from starting a module drag', async () => {
    const wrapper = mountBoard()
    try {
      await flushPromises()
      wrapper
        .get('[data-module-id="overview"] .module-panel-header')
        .element.dispatchEvent(pointer('pointerdown', 150, 20, 1))
      window.dispatchEvent(pointer('pointermove', 152, 21, 1))
      await nextTick()
      expect(document.body.classList.contains('is-module-dragging')).toBe(false)
      expect(wrapper.find('.module-drop-preview').exists()).toBe(false)
      window.dispatchEvent(pointer('pointerup', 152, 21, 0))
      window.dispatchEvent(pointer('pointermove', 700, 300, 1))
      window.dispatchEvent(pointer('pointerup', 700, 300, 0))
      await flushPromises()
      expect(wrapper.emitted('split')).toBeUndefined()
    } finally {
      wrapper.unmount()
    }
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
    const header = wrapper.findAll('.module-panel-header')[0]!
    header.element.dispatchEvent(pointer('pointerdown', 30, 30, 1))
    window.dispatchEvent(pointer('pointermove', 30, 30, 1))
    // 第二个 split 的间隙位于 599.2~602.4px。
    window.dispatchEvent(pointer('pointermove', 601, 300, 1))
    await nextTick()
    expect(wrapper.find('.module-insert-preview').text()).toBe('插入此处')
    expect(wrapper.find('.module-drop-preview').exists()).toBe(false)
    window.dispatchEvent(pointer('pointerup', 601, 300, 0))
    await flushPromises()
    expect(wrapper.emitted('insert')).toEqual([['overview', [1]]])
    expect(document.body.classList.contains('is-module-dragging')).toBe(false)
    wrapper.unmount()
  })

  it('cancels a drag with Escape without splitting', async () => {
    const wrapper = mountBoard()
    await flushPromises()
    const header = wrapper.findAll('.module-panel-header')[0]!
    header.element.dispatchEvent(pointer('pointerdown', 30, 30, 1))
    window.dispatchEvent(pointer('pointermove', 30, 30, 1))
    window.dispatchEvent(pointer('pointermove', 700, 300, 1))
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    window.dispatchEvent(pointer('pointerup', 700, 300, 0))
    await flushPromises()
    expect(wrapper.emitted('split')).toBeUndefined()
    expect(document.body.classList.contains('is-module-dragging')).toBe(false)
    wrapper.unmount()
  })

  it.each(['row', 'column'] as const)('resizes a %s split from its narrow gutter', async (direction) => {
    const wrapper = mountBoard({ ...rowLayout, direction })
    try {
      await flushPromises()
      const gutter = wrapper.get<HTMLElement>('.module-gutter')
      expect(parseFloat(gutter.element.style[direction === 'row' ? 'width' : 'height'])).toBeCloseTo(3.2)
      gutter.element.dispatchEvent(pointer('pointerdown', 400, 300, 1))
      const [x, y] = direction === 'row' ? [500, 300] : [400, 400]
      window.dispatchEvent(pointer('pointermove', x!, y!, 1))
      await nextTick()
      expect(document.body.classList.contains('is-module-resizing')).toBe(true)
      window.dispatchEvent(pointer('pointerup', x!, y!, 0))
      await flushPromises()
      expect(wrapper.emitted('resize')).toHaveLength(1)
      const [path, ratio] = wrapper.emitted('resize')![0] as [number[], number]
      expect(path).toEqual([])
      expect(ratio).toBeCloseTo(direction === 'row' ? 500 / 796.8 : 400 / 596.8, 3)
      expect(document.body.classList.contains('is-module-resizing')).toBe(false)
    } finally {
      wrapper.unmount()
    }
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

  it.each(['left', 'right'] as const)(
    'keeps remote modules fixed through preview and commit at the %s side of a stack',
    async (side) => {
      const stack: LayoutNode = {
        type: 'split',
        direction: 'column',
        ratio: 0.5,
        first: leaf('editor'),
        second: leaf('history'),
      }
      const layout: LayoutNode =
        side === 'left'
          ? {
              type: 'split',
              direction: 'row',
              ratio: 0.3,
              first: leaf('calendar'),
              second: { type: 'split', direction: 'row', ratio: 0.6, first: stack, second: leaf('overview') },
            }
          : {
              type: 'split',
              direction: 'row',
              ratio: 0.7,
              first: { type: 'split', direction: 'row', ratio: 0.4, first: leaf('calendar'), second: stack },
              second: leaf('overview'),
            }
      const wrapper = mountBoard(layout, 800, 600, { EditorModule: true })
      const far = side === 'left' ? 'overview' : 'calendar'
      const placements = () =>
        new Map(
          wrapper
            .findAll('.module-slot')
            .map((slot) => [slot.get('.module-panel').attributes('data-module-id'), slot.attributes('style')]),
        )
      const farRect = () => wrapper.get(`[data-module-id="${far}"]`).element.parentElement!.style
      const rect = () =>
        Object.fromEntries(
          ['left', 'top', 'width', 'height'].map((key) => [key, parseFloat(farRect().getPropertyValue(key))]),
        )
      try {
        await flushPromises()
        const before = rect()
        const gutter = wrapper.findAll<HTMLElement>('.module-gutter')[0]!
        const start = parseFloat(gutter.element.style.left) + 1.6
        gutter.element.dispatchEvent(pointer('pointerdown', start, 300, 1))
        window.dispatchEvent(pointer('pointermove', 400, 300, 1))
        await nextTick()
        for (const key of ['left', 'top', 'width', 'height']) expect(rect()[key]).toBeCloseTo(before[key]!, 9)
        const preview = placements()
        window.dispatchEvent(pointer('pointerup', 400, 300, 0))
        const [path, ratio, metrics] = wrapper.emitted('resize')![0] as [number[], number, LayoutMetrics]
        expect(metrics).toEqual({ width: 800, height: 600, gap: 3.2 })
        await wrapper.setProps({ layout: resizeLayoutLocally(layout, path, ratio, metrics) })
        await flushPromises()
        expect(placements()).toEqual(preview)
        for (const key of ['left', 'top', 'width', 'height']) expect(rect()[key]).toBeCloseTo(before[key]!, 9)
      } finally {
        wrapper.unmount()
      }
    },
  )

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

  it.each(['button', 'Escape', 'backdrop'])(
    'animates the full module to maximum and back to its slot via %s',
    async (action) => {
      const wrapper = mountBoard()
      const normal = new DOMRect(0, 0, 398.4, 600)
      const expanded = new DOMRect(20, 20, 760, 560)
      const frames: { keyframes: Keyframe[]; finish: () => void }[] = []
      try {
        await flushPromises()
        const panel = wrapper.get<HTMLElement>('[data-module-id="overview"]').element
        panel.getBoundingClientRect = () => (panel.classList.contains('is-maximized') ? expanded : normal)
        panel.parentElement!.getBoundingClientRect = () => normal
        const button = wrapper.get<HTMLElement>('[data-module-id="overview"] .module-maximize-button').element
        button.getBoundingClientRect = () => new DOMRect(340, 5, 24, 24)
        panel.animate = ((keyframes: Keyframe[]) => {
          let finish!: () => void
          let reject!: (reason: unknown) => void
          const finished = new Promise<void>((resolve, fail) => {
            finish = resolve
            reject = fail
          })
          frames.push({ keyframes, finish })
          return { finished, cancel: () => reject(new DOMException('Cancelled', 'AbortError')) } as unknown as Animation
        }) as typeof panel.animate

        await wrapper.get('[data-module-id="overview"] .module-maximize-button').trigger('click')
        expect(frames).toHaveLength(1)
        const transform = `translate(-20px, -20px) scale(${398.4 / 760}, ${600 / 560})`
        expect(frames[0]!.keyframes[0]).toMatchObject({ transform, opacity: 1 })
        frames[0]!.finish()
        wrapper
          .get('.module-maximize-backdrop')
          .element.getAnimations()
          .forEach((animation) => animation.finish())
        await flushPromises()

        if (action === 'button') await wrapper.get('.is-maximized .module-maximize-button').trigger('click')
        else if (action === 'backdrop') await wrapper.get('.module-maximize-backdrop').trigger('click')
        else window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
        await nextTick()
        expect(wrapper.find('.module-panel.is-maximized').exists()).toBe(true)
        expect(frames).toHaveLength(2)
        expect(frames[1]!.keyframes[1]).toMatchObject({ transform, opacity: 1 })
        frames[1]!.finish()
        wrapper
          .get('.module-maximize-backdrop')
          .element.getAnimations()
          .forEach((animation) => animation.finish())
        await flushPromises()
        expect(wrapper.find('.module-panel.is-maximized').exists()).toBe(false)
        expect(panel.getBoundingClientRect()).toEqual(normal)
        expect(frames).toHaveLength(2)
        expect(wrapper.emitted('resize')).toBeUndefined()
      } finally {
        wrapper.unmount()
      }
    },
  )

  it('dismisses a maximized module help popover before Escape restores the module', async () => {
    const wrapper = mountBoard()
    try {
      await flushPromises()
      await wrapper.get('.module-maximize-button').trigger('click')
      await wrapper.get('.module-panel.is-maximized .module-help-button').trigger('click')
      await flushPromises()
      const popup = document.querySelector<HTMLElement>('.module-help-popover')!
      expect(popup).not.toBeNull()
      popup.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
      await flushPromises()
      expect(document.querySelector('.module-help-popover')).toBeNull()
      expect(wrapper.find('.module-panel.is-maximized').exists()).toBe(true)
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
      await flushPromises()
      expect(wrapper.find('.module-panel.is-maximized').exists()).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })
})
