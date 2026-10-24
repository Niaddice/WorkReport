import { afterEach, describe, expect, it, vi } from 'vitest'
import * as Vue from 'vue'
import { createRenderer, h, nextTick, ssrContextKey, type RenderFunction } from 'vue'
import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc'
import CalendarPane from '../src/components/CalendarPane.vue'
import calendarSource from '../src/components/CalendarPane.vue?raw'
import { installHolidayData } from '../src/lib/chinaHolidays'
import { emptyHolidayState } from '../src/lib/holidays'

interface TestNode {
  type: string
  text: string
  props: Record<string, unknown>
  children: TestNode[]
  parent: TestNode | null
}

function node(type: string, text = ''): TestNode {
  return { type, text, props: {}, children: [], parent: null }
}

function insert(child: TestNode, parent: TestNode, anchor: TestNode | null = null) {
  remove(child)
  const index = anchor ? parent.children.indexOf(anchor) : -1
  parent.children.splice(index < 0 ? parent.children.length : index, 0, child)
  child.parent = parent
}

function remove(child: TestNode) {
  if (!child.parent) return
  const index = child.parent.children.indexOf(child)
  if (index >= 0) child.parent.children.splice(index, 1)
  child.parent = null
}

// Exercise the real component's reactive render without introducing a browser
// dependency. The host records the date buttons and their rendered classes.
const renderer = createRenderer<TestNode, TestNode>({
  createElement: (type) => node(type),
  createText: (text) => node('text', text),
  createComment: (text) => node('comment', text),
  setText: (target, text) => {
    target.text = text
  },
  setElementText: (target, text) => {
    target.text = text
    target.children = []
  },
  parentNode: (target) => target.parent,
  nextSibling: (target) => {
    const siblings = target.parent?.children ?? []
    return siblings[siblings.indexOf(target) + 1] ?? null
  },
  insert,
  remove,
  patchProp: (target, key, _previous, value) => {
    target.props[key] = value
  },
  insertStaticContent: (content, parent, anchor) => {
    const target = node('static', content)
    insert(target, parent, anchor)
    return [target, target]
  },
})

// Vitest loads SFCs through Vite's server transform. Compile this same template
// for a client render so updates can be tested on one mounted component.
const descriptor = parse(calendarSource).descriptor
const bindings = compileScript(descriptor, { id: 'calendar-holiday-sync-test' }).bindings
const template = compileTemplate({
  id: 'calendar-holiday-sync-test',
  filename: 'CalendarPane.vue',
  source: descriptor.template!.content,
  compilerOptions: { bindingMetadata: bindings, isCustomElement: (tag) => tag.startsWith('md-') },
})
if (template.errors.length) throw new Error(String(template.errors[0]))
const renderSource = template.code
  .replace(
    /import \{([^}]+)\} from ["']vue["']/g,
    (_match, imports: string) => `const {${imports.replace(/\bas\b/g, ':')}} = Vue`,
  )
  .replace('export function render', 'return function render')
const ClientCalendarPane = Object.assign({}, CalendarPane, {
  render: new Function('Vue', renderSource)(Vue) as RenderFunction,
})

function descendants(root: TestNode): TestNode[] {
  return root.children.flatMap((child) => [child, ...descendants(child)])
}

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
  installHolidayData(emptyHolidayState())
})

describe('mounted calendar holiday updates', () => {
  it('updates holiday labels and makeup-day weekend styling after sync without navigating or losing selection', async () => {
    vi.useFakeTimers({ toFake: ['Date'] })
    vi.setSystemTime(new Date('2026-10-23T12:00:00'))
    vi.stubGlobal('window', { addEventListener: vi.fn(), removeEventListener: vi.fn() })
    const onSelect = vi.fn()
    const onRange = vi.fn()
    const root = node('root')
    const app = renderer.createApp({
      setup: () => () =>
        h(ClientCalendarPane, {
          today: '20261023',
          selected: '20261022',
          entries: [],
          previews: {},
          disabled: false,
          onSelect,
          onRange,
        }),
    })
    app.provide(ssrContextKey, { modules: new Set() })
    app.mount(root)
    try {
      const day = (date: number) =>
        descendants(root).find(
          (target) => target.type === 'button' && String(target.props['aria-label']).startsWith(`2026年10月${date}日 `),
        )!
      const label = (date: number) =>
        day(date).children.find((target) => String(target.props.class).includes('lunar-label'))?.text
      expect(label(10)).toBe('补班')
      expect(day(10).props.class).not.toContain('weekend')
      expect(day(11).props.class).toContain('weekend')
      expect(day(22).props.class).toContain('selected')

      installHolidayData({
        years: [
          {
            year: 2026,
            days: [
              { date: '2026-10-10', name: '调整后的假期', isOffDay: true },
              { date: '2026-10-11', name: '国庆节', isOffDay: false },
            ],
            syncedAt: '2026-10-23T12:00:00+08:00',
            sourceUrl: 'https://raw.githubusercontent.com/NateScarlet/holiday-cn/master/2026.json',
          },
        ],
        failures: [],
      })
      await nextTick()

      expect(label(10)).toBe('调整后的假期')
      expect(day(10).props.class).toContain('weekend')
      expect(label(11)).toBe('补班')
      expect(day(11).props.class).not.toContain('weekend')
      expect(label(1)).toBeUndefined()
      expect(day(22).props.class).toContain('selected')
      expect(day(23).props.class).toContain('is-today')
      expect(onSelect).not.toHaveBeenCalled()
      expect(onRange).not.toHaveBeenCalled()
    } finally {
      app.unmount()
    }
  })
})
