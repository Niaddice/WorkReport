import { watch, type Ref } from 'vue'
import {
  createScrollSyncGuard,
  elementScrollPosition,
  markdownPreviewScrollTop,
  type MarkdownScrollPosition,
} from './markdownScroll'

type Viewport = Readonly<Ref<HTMLElement | undefined>>
interface SplitScrollOptions {
  editor: Viewport
  preview: Viewport
  enabled: () => boolean
  sourceKey: () => string
  readEditor?: (element: HTMLElement, key: string) => MarkdownScrollPosition
  readPreview?: (element: HTMLElement, key: string) => MarkdownScrollPosition
  editorTop?: (element: HTMLElement, position: MarkdownScrollPosition) => number
  previewTop?: (element: HTMLElement, position: MarkdownScrollPosition) => number
}

/** 同一日报内两栏共用滚动位置；同步产生的事件不会反向覆盖来源位置。 */
export function useSplitPreviewScroll(options: SplitScrollOptions) {
  const guard = createScrollSyncGuard()
  let position: MarkdownScrollPosition | null = null
  function apply(element: HTMLElement | undefined, editor: boolean) {
    if (!element?.clientHeight || !position || position.sourceKey !== options.sourceKey()) return
    const calculate = editor ? options.editorTop : options.previewTop
    const top = calculate
      ? calculate(element, position)
      : markdownPreviewScrollTop(position, [], element.scrollHeight - element.clientHeight)
    guard.apply(element, top)
  }
  function synchronize() {
    if (!options.enabled() || !position) return
    apply(position.origin === 'editor' ? options.preview.value : options.editor.value, position.origin !== 'editor')
  }
  function captureEditor() {
    const element = options.editor.value
    if (!element?.clientHeight) return
    position = { ...(options.readEditor ?? elementScrollPosition)(element, options.sourceKey()), origin: 'editor' }
    synchronize()
  }
  function onScroll(event: Event) {
    const element = event.currentTarget as HTMLElement
    const editor = element === options.editor.value
    if (!editor && (element !== options.preview.value || !options.enabled())) return
    if (!element.clientHeight || !guard.accept(element)) return
    const read = editor ? options.readEditor : options.readPreview
    position = {
      ...(read ?? elementScrollPosition)(element, options.sourceKey()),
      origin: editor ? 'editor' : 'preview',
    }
    synchronize()
  }
  function refresh() {
    if (!options.enabled()) return
    if (!position) captureEditor()
    apply(options.editor.value, true)
    apply(options.preview.value, false)
  }
  watch(
    options.enabled,
    (enabled) => {
      if (enabled) captureEditor()
    },
    { flush: 'post' },
  )
  watch([options.editor, options.preview], refresh, { flush: 'post' })
  watch(
    options.sourceKey,
    () => {
      position = null
      if (options.editor.value) guard.apply(options.editor.value, 0)
      if (options.preview.value) guard.apply(options.preview.value, 0)
    },
    { flush: 'post' },
  )
  return { onScroll, captureEditor, refresh }
}
