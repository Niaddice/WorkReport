import { afterEach, describe, expect, it, vi } from 'vitest'
import * as Vue from 'vue'
import { computed, createRenderer, h, nextTick, ref, ssrContextKey, type RenderFunction } from 'vue'
import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc'
import dayjs from 'dayjs'
import CalendarPane from '../src/components/CalendarPane.vue'
import calendarSource from '../src/components/CalendarPane.vue?raw'
import { dateKey } from '../src/lib/calendar'

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
  createElement: type => node(type),
  createText: text => node('text', text),
  createComment: text => node('comment', text),
  setText: (target, text) => { target.text = text },
  setElementText: (target, text) => { target.text = text; target.children = [] },
  parentNode: target => target.parent,
  nextSibling: target => {
    const siblings = target.parent?.children ?? []
    return siblings[siblings.indexOf(target) + 1] ?? null
  },
  insert,
  remove,
  patchProp: (target, key, _previous, value) => { target.props[key] = value },
  insertStaticContent: (content, parent, anchor) => {
    const target = node('static', content)
    insert(target, parent, anchor)
    return [target, target]
  },
})

// Vitest loads SFCs through Vite's server transform. Compile this same template
// for a client render so updates can be tested on one mounted component.
const descriptor = parse(calendarSource).descriptor
const bindings = compileScript(descriptor, { id: 'calendar-rollover-test' }).bindings
const template = compileTemplate({
  id: 'calendar-rollover-test', filename: 'CalendarPane.vue', source: descriptor.template!.content,
  compilerOptions: { bindingMetadata: bindings, isCustomElement: tag => tag.startsWith('md-') },
})
if (template.errors.length) throw new Error(String(template.errors[0]))
const renderSource = template.code
  .replace(/import \{([^}]+)\} from ["']vue["']/g, (_match, imports: string) => `const {${imports.replace(/\bas\b/g, ':')}} = Vue`)
  .replace('export function render', 'return function render')
const ClientCalendarPane = Object.assign({}, CalendarPane, { render: new Function('Vue', renderSource)(Vue) as RenderFunction })

function descendants(root: TestNode): TestNode[] {
  return root.children.flatMap(child => [child, ...descendants(child)])
}

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals() })

describe('calendar date rollover', () => {
  it('updates today after midnight without changing the selected report or emitting navigation', async () => {
    vi.useFakeTimers({ toFake: ['Date'] })
    vi.setSystemTime(new Date('2026-10-22T23:59:00'))
    vi.stubGlobal('window', { addEventListener: vi.fn(), removeEventListener: vi.fn() })
    const now = ref(dayjs())
    const today = computed(() => dateKey(now.value))
    const onSelect = vi.fn()
    const onRange = vi.fn()
    const root = node('root')
    const app = renderer.createApp({
      setup: () => () => h(ClientCalendarPane, {
        today: today.value, selected: '20261022', entries: [], previews: {}, disabled: false,
        onSelect, onRange,
      }),
    })
    app.provide(ssrContextKey, { modules: new Set() })
    app.mount(root)
    try {
      const day = (date: number) => descendants(root).find(target => target.type === 'button' && String(target.props['aria-label']).startsWith(`2026年10月${date}日 `))!
      expect(day(22).props.class).toContain('is-today')
      expect(day(22).props.class).toContain('selected')
      expect(day(23).props.class).not.toContain('is-today')

      // App's existing minute/focus clock publishes a new date into this same
      // mounted calendar; the calendar must not hold its startup date forever.
      vi.setSystemTime(new Date('2026-10-23T00:01:00'))
      now.value = dayjs()
      await nextTick()

      expect(day(22).props.class).not.toContain('is-today')
      expect(day(23).props.class).toContain('is-today')
      expect(day(22).props.class).toContain('selected')
      expect(day(23).props.class).not.toContain('selected')
      expect(onSelect).not.toHaveBeenCalled()
      expect(onRange).not.toHaveBeenCalled()
    } finally { app.unmount() }
  })
})
