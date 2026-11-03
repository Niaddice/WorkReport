<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'
import { Editor, EditorContent } from '@tiptap/vue-3'
import { getMarkRange, type Editor as CoreEditor } from '@tiptap/core'
import { TextSelection, type SelectionBookmark } from '@tiptap/pm/state'
import type {
  HeadingLevel,
  MarkdownTableAction,
  MarkdownToolbarAction,
  MarkdownToolbarState,
} from '../lib/markdownToolbar'
import {
  convertTypedMarkdownTable,
  clearRichMarkdownFormat,
  indentRichMarkdown,
  insertRichMarkdownTable,
  insertMarkdownDivider,
  markdownCellContent,
  markdownTableTemplate,
  prepareRichMarkdown,
  richMarkdownExtensions,
  richMarkdownToolbarState,
  richMarkdownSlice,
  selectedRichMarkdown,
  selectionInMarkdownCell,
  setRichMarkdown,
  runMarkdownTableAction,
} from '../lib/richMarkdown'

const props = defineProps<{ content: string; disabled: boolean; sourceKey: string }>()
const emit = defineEmits<{
  edit: [markdown: string, forceHistory?: boolean]
  undo: []
  redo: []
  save: []
  'selection-change': []
  'toolbar-state': [state: MarkdownToolbarState]
  scroll: [event: Event]
}>()
const editor = shallowRef<Editor | null>(null)
const scrollElement = ref<HTMLElement>()
const linkInput = ref<HTMLInputElement>()
const linkOpened = ref(false)
const linkAddress = ref('https://')
const linkText = ref('链接文字')
const linkError = ref('')
const editingLink = ref(false)
const insertingLinkText = ref(false)
let linkSelection: SelectionBookmark | null = null
let linkOriginalSelection = { from: 0, to: 0 }
let linkVersion = 0
let toolbarSignature = ''
let sourceKey = props.sourceKey
let lastPropContent = props.content
let lastEmittedContent: string | undefined
let forceHistory = false
let version = 0

function getToolbarState() {
  return richMarkdownToolbarState(editor.value, props.disabled)
}
function publishToolbar(instance: CoreEditor | null = editor.value) {
  const state = richMarkdownToolbarState(instance, props.disabled)
  const signature = JSON.stringify(state)
  if (signature === toolbarSignature) return
  toolbarSignature = signature
  emit('toolbar-state', state)
}
function historyAvailable() {
  return (
    !props.disabled &&
    !!editor.value &&
    !editor.value.view.dom.closest('[inert]') &&
    !document.querySelector('dialog[open]:not(.is-inline)')
  )
}

function change(run: () => unknown, force = true) {
  if (props.disabled || !editor.value) return
  forceHistory = force
  try {
    return run()
  } finally {
    forceHistory = false
  }
}

function insertMarkdown(text: string) {
  const instance = editor.value
  if (!instance) return
  if (!text) return instance.commands.deleteSelection()
  if (selectionInMarkdownCell(instance)) return instance.commands.insertContent(markdownCellContent(instance, text))
  return instance.commands.insertContent(prepareRichMarkdown(text), { contentType: 'markdown' })
}

onMounted(() => {
  editor.value = new Editor({
    extensions: richMarkdownExtensions(),
    content: prepareRichMarkdown(props.content),
    contentType: 'markdown',
    editable: !props.disabled,
    editorProps: {
      attributes: {
        class: 'markdown-editor journal-prose rich-markdown-document',
        role: 'textbox',
        'aria-label': '日报文本内容',
        'aria-multiline': 'true',
        'data-placeholder': '写下今天的工作…',
        spellcheck: 'false',
      },
      clipboardTextSerializer: (slice) => (editor.value ? richMarkdownSlice(editor.value, slice) : ''),
      handleDOMEvents: {
        beforeinput(_view, event) {
          const input = event as InputEvent
          if (
            !historyAvailable() ||
            input.defaultPrevented ||
            input.isComposing ||
            !['historyUndo', 'historyRedo'].includes(input.inputType)
          )
            return false
          input.preventDefault()
          if (input.inputType === 'historyUndo') emit('undo')
          else emit('redo')
          return true
        },
      },
      handleKeyDown(_view, event) {
        if (event.defaultPrevented || event.isComposing || event.keyCode === 229) return false
        if (historyAvailable() && !event.altKey && (event.metaKey || event.ctrlKey)) {
          const key = event.key.toLowerCase()
          if (key === 'z' || (key === 'y' && event.ctrlKey && !event.metaKey)) {
            event.preventDefault()
            event.stopPropagation()
            if ((key === 'z' && event.shiftKey) || key === 'y') emit('redo')
            else emit('undo')
            return true
          }
        }
        if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's') {
          event.preventDefault()
          emit('save')
          return true
        }
        if (!props.disabled && (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
          event.preventDefault()
          openLink()
          return true
        }
        if (props.disabled || !editor.value || event.key !== 'Enter') return false
        if (editor.value.isActive('table')) {
          // A new paragraph inside a cell cannot round-trip through GFM.
          // Enter, like Tab, advances to the next cell and grows the last row.
          change(() => {
            const instance = editor.value!
            if (!instance.commands.goToNextCell()) instance.chain().addRowAfter().goToNextCell().run()
          })
          return true
        }
        return change(() => convertTypedMarkdownTable(editor.value!)) === true
      },
      handlePaste(_view, event) {
        if (props.disabled || !editor.value) return false
        const text = event.clipboardData?.getData('text/plain') || event.clipboardData?.getData('text/html')
        if (!text) return false
        // Markdown is the persisted format. Prefer its complete source when
        // supplied, including when a website adds an HTML clipboard flavor.
        event.preventDefault()
        change(() => insertMarkdown(text))
        return true
      },
    },
    onTransaction({ transaction, editor: instance }) {
      if (transaction.docChanged) {
        version++
        linkOpened.value = false
        linkSelection = null
      }
      publishToolbar(instance)
    },
    onUpdate({ editor: instance }) {
      lastEmittedContent = instance.getMarkdown()
      emit('edit', lastEmittedContent, forceHistory || undefined)
    },
    onSelectionUpdate({ editor: instance }) {
      if (
        linkOpened.value &&
        (instance.state.selection.from !== linkOriginalSelection.from ||
          instance.state.selection.to !== linkOriginalSelection.to)
      ) {
        linkOpened.value = false
        linkSelection = null
      }
      emit('selection-change')
    },
  })
  publishToolbar()
})

watch(
  () => [props.sourceKey, props.content] as const,
  ([key, content]) => {
    const instance = editor.value
    const changedScope = key !== sourceKey
    if (!changedScope && (content === lastPropContent || content === lastEmittedContent)) {
      lastPropContent = content
      return
    }
    sourceKey = key
    lastPropContent = content
    lastEmittedContent = undefined
    linkOpened.value = false
    linkSelection = null
    if (instance) setRichMarkdown(instance, content, changedScope)
    if (changedScope && scrollElement.value) scrollElement.value.scrollTop = 0
  },
)
watch(
  () => props.disabled,
  (disabled) => {
    editor.value?.setEditable(!disabled, false)
    if (disabled) linkOpened.value = false
    publishToolbar()
  },
)
onBeforeUnmount(() => editor.value?.destroy())

function focus() {
  editor.value?.commands.focus(undefined, { scrollIntoView: false })
}

function closeLink() {
  linkOpened.value = false
  linkSelection = null
  focus()
}

function openLink() {
  const instance = editor.value
  if (!instance || props.disabled || !getToolbarState().enabled.link) return
  const selection = instance.state.selection
  const { from, to, $from, empty } = selection
  const range = getMarkRange($from, instance.schema.marks.link!)
  editingLink.value = instance.isActive('link')
  insertingLinkText.value = empty && !editingLink.value
  linkSelection = (
    empty && editingLink.value && range ? TextSelection.create(instance.state.doc, range.from, range.to) : selection
  ).getBookmark()
  linkOriginalSelection = { from, to }
  linkVersion = version
  linkText.value = instance.state.doc.textBetween(from, to, ' ') || '链接文字'
  linkAddress.value = String(instance.getAttributes('link').href || 'https://')
  linkError.value = ''
  linkOpened.value = true
  void nextTick(() => {
    linkInput.value?.focus()
    linkInput.value?.select()
  })
}

function saveLink() {
  const instance = editor.value
  const bookmark = linkSelection
  if (!instance || props.disabled || !bookmark || linkVersion !== version) return
  const href = linkAddress.value.trim()
  if (!/^(https?:\/\/[^\s]+|mailto:[^\s]+|tel:[+\d][\d\s-]*)$/i.test(href)) {
    linkError.value = '请输入有效的链接地址'
    return
  }
  const selection = bookmark.resolve(instance.state.doc)
  linkOpened.value = false
  linkSelection = null
  change(() => {
    const chain = instance
      .chain()
      .setTextSelection({ from: selection.from, to: selection.to })
      .focus(undefined, { scrollIntoView: false })
    const { empty } = selection
    if (empty)
      chain
        .insertContent({
          type: 'text',
          text: linkText.value.trim() || href,
          marks: [{ type: 'link', attrs: { href } }],
        })
        .run()
    else chain.setLink({ href }).run()
  })
  focus()
}

function removeLink() {
  const instance = editor.value
  const bookmark = linkSelection
  if (!instance || props.disabled || !bookmark || linkVersion !== version) return
  const selection = bookmark.resolve(instance.state.doc)
  linkOpened.value = false
  linkSelection = null
  change(() =>
    instance
      .chain()
      .setTextSelection({ from: selection.from, to: selection.to })
      .focus(undefined, { scrollIntoView: false })
      .unsetLink()
      .run(),
  )
}

function apply(action: MarkdownToolbarAction) {
  const instance = editor.value
  if (!instance || props.disabled || !getToolbarState().enabled[action]) return
  if (action === 'link') return openLink()
  linkOpened.value = false
  change(() => {
    const chain = instance.chain().focus(undefined, { scrollIntoView: false })
    switch (action) {
      case 'heading':
        return chain.toggleHeading({ level: 2 }).run()
      case 'bold':
        return chain.toggleBold().run()
      case 'italic':
        return chain.toggleItalic().run()
      case 'strike':
        return chain.toggleStrike().run()
      case 'bulletList':
        return chain.toggleBulletList().run()
      case 'orderedList':
        return chain.toggleOrderedList().run()
      case 'taskList':
        return chain.toggleTaskList().run()
      case 'quote':
        return chain.toggleBlockquote().run()
      case 'inlineCode':
        return chain.toggleCode().run()
      case 'codeBlock':
        return chain.toggleCodeBlock().run()
      case 'divider':
        instance.commands.focus(undefined, { scrollIntoView: false })
        return insertMarkdownDivider(instance)
      case 'table':
        return instance.isActive('table')
          ? runMarkdownTableAction(instance, 'addRowAfter')
          : chain.insertContent([markdownTableTemplate(), { type: 'paragraph' }]).run()
      case 'clearFormat':
        instance.commands.focus(undefined, { scrollIntoView: false })
        return clearRichMarkdownFormat(instance)
      case 'indent':
      case 'outdent':
        instance.commands.focus(undefined, { scrollIntoView: false })
        return indentRichMarkdown(instance, action === 'outdent')
    }
  })
}

function setHeading(level: HeadingLevel | 0) {
  const instance = editor.value
  if (!instance || !getToolbarState().enabled.heading || ![0, 1, 2, 3, 4, 5, 6].includes(level)) return
  change(() => {
    const chain = instance.chain().focus(undefined, { scrollIntoView: false })
    return level ? chain.setHeading({ level }).run() : chain.setParagraph().run()
  })
}
function insertTable(rows: number, columns: number) {
  const instance = editor.value
  if (!instance || !getToolbarState().enabled.table || getToolbarState().inTable) return
  change(() => {
    instance.commands.focus(undefined, { scrollIntoView: false })
    return insertRichMarkdownTable(instance, rows, columns)
  })
}
function tableAction(action: MarkdownTableAction) {
  const instance = editor.value
  if (!instance || props.disabled || !getToolbarState().tableEnabled[action]) return
  change(() => {
    instance.commands.focus(undefined, { scrollIntoView: false })
    return runMarkdownTableAction(instance, action)
  })
}

function replaceSelection(text: string) {
  if (props.disabled) return
  linkOpened.value = false
  change(() => insertMarkdown(text))
}
function getSelection() {
  const selection = editor.value?.state.selection
  return { from: selection?.from ?? 0, to: selection?.to ?? 0 }
}

defineExpose({
  focus,
  restoreFocus: () => {
    if (!props.disabled) editor.value?.view.focus()
  },
  apply,
  getToolbarState,
  setHeading,
  insertTable,
  tableAction,
  replaceSelection,
  getScrollElement: () => scrollElement.value ?? null,
  getEditableElement: () => editor.value?.view.dom ?? null,
  getMarkdown: () => editor.value?.getMarkdown() ?? props.content,
  getSelectedMarkdown: () => (editor.value ? selectedRichMarkdown(editor.value) : ''),
  getSelection,
  selectAll: () => editor.value?.commands.selectAll(),
  transactionVersion: () => version,
})
</script>

<template>
  <div ref="scrollElement" class="rich-markdown-scroll" @scroll.passive="emit('scroll', $event)">
    <EditorContent :editor="editor ?? undefined" />
    <form
      v-if="linkOpened"
      class="rich-markdown-link-editor"
      aria-label="编辑链接"
      @submit.prevent="saveLink"
      @keydown.esc.prevent="closeLink"
    >
      <label
        >链接地址<input
          ref="linkInput"
          v-model="linkAddress"
          type="text"
          aria-label="链接地址"
          autocomplete="off"
          spellcheck="false"
      /></label>
      <label v-if="insertingLinkText">链接文字<input v-model="linkText" type="text" aria-label="链接文字" /></label>
      <span v-if="linkError" role="alert">{{ linkError }}</span>
      <div>
        <button v-if="editingLink" type="button" class="rich-markdown-unlink" @click="removeLink">移除链接</button
        ><button type="button" @click="closeLink">取消</button
        ><button type="submit">{{ editingLink ? '保存链接' : '插入链接' }}</button>
      </div>
    </form>
  </div>
</template>
