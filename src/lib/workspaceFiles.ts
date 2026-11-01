import { invoke } from '@tauri-apps/api/core'
import type { FileSettings } from './fileSettings'

export type FileAction = 'createFile' | 'createDirectory' | 'rename' | 'delete' | 'restore'
export interface FileRequest {
  action: FileAction
  path: string
  name?: string
  trashId?: string
}
export interface FilePlan {
  token: string
  fileCount: number
  directoryCount: number
  journalCount: number
  settingsChanged: boolean
}
export interface FileChangeResult {
  files: FileSettings
  path: string
}
export interface TrashEntry {
  id: string
  path: string
  name: string
  isDir: boolean
  deletedAt: number
  fileCount: number
  directoryCount: number
}
export const workspaceFilesApi = {
  preview: (workspacePath: string, request: FileRequest) =>
    invoke<FilePlan>('preview_workspace_file_change', { workspacePath, request }),
  apply: (workspacePath: string, request: FileRequest, token: string) =>
    invoke<FileChangeResult>('apply_workspace_file_change', { workspacePath, request, token }),
  trash: (workspacePath: string) => invoke<TrashEntry[]>('list_workspace_trash', { workspacePath }),
  open: (workspacePath: string, path: string, parent = false) =>
    invoke<void>('open_workspace_item', { workspacePath, path, parent }),
}
export function fileNameError(value: string): string {
  const name = value.trim()
  if (!name || name.length > 120) return '请填写有效名称，最多 120 个字符。'
  if (
    /[\x00-\x1f\x7f<>:"/\\|?*]/.test(name) ||
    name.startsWith('.') ||
    /[. ]$/.test(name) ||
    /^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(name)
  )
    return '名称不能包含路径分隔符、隐藏名称、保留名称或特殊字符。'
  return ''
}
