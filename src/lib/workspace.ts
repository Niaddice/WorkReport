import { invoke } from '@tauri-apps/api/core'

export type FileFormat = 'md' | 'txt'
export interface Workspace { path: string; name: string }
export interface Entry { date: string; filename: string; format: FileFormat }
export interface Preview { date: string; preview: string }
export interface Document { date: string; filename: string; format: FileFormat; content: string; revision: string; backupWarning?: string }
export const api = {
  recent: () => invoke<string | null>('recent_workspace'),
  open: (path: string) => invoke<Workspace>('open_workspace', { path }),
  openDirectory: () => invoke<void>('open_workspace_directory'),
  list: () => invoke<Entry[]>('list_entries'),
  backupWarning: () => invoke<string | null>('database_backup_warning'),
  retryBackup: (workspacePath: string) => invoke<unknown>('retry_database_backup', { workspacePath }),
  previews: (dates: string[]) => invoke<Preview[]>('list_previews', { dates }),
  read: (date: string, format: FileFormat = 'md') => invoke<Document>('read_entry', { date, format }),
  create: (date: string, format: FileFormat, content: string) => invoke<Document>('create_entry', { date, format, content }),
  save: (date: string, content: string, expectedRevision: string, format: FileFormat = 'md') => invoke<Document>('save_entry', { date, content, expectedRevision, format }),
  switchFormat: (date: string, from: FileFormat, to: FileFormat, expectedRevision: string) => invoke<Document>('switch_entry_format', { date, from, to, expectedRevision }),
  copy: (content: string) => invoke<void>('copy_content', { content }),
}
