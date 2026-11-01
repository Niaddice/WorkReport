import type { MarkdownAction } from './markdownEditing'

export type MarkdownToolbarAction = MarkdownAction | 'clearFormat' | 'indent' | 'outdent'
export type HeadingLevel = 1 | 2 | 3 | 4 | 5 | 6
export type MarkdownTableAction =
  | 'addRowBefore'
  | 'addRowAfter'
  | 'deleteRow'
  | 'addColumnBefore'
  | 'addColumnAfter'
  | 'deleteColumn'
  | 'deleteTable'
  | 'alignLeft'
  | 'alignCenter'
  | 'alignRight'

export interface MarkdownToolbarState {
  active: Partial<Record<MarkdownToolbarAction, boolean>>
  enabled: Partial<Record<MarkdownToolbarAction, boolean>>
  headingLevel: HeadingLevel | 0 | null
  inTable: boolean
  tableEnabled: Partial<Record<MarkdownTableAction, boolean>>
  tableAlignment: 'left' | 'center' | 'right' | null
}
