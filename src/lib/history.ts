import { invoke } from '@tauri-apps/api/core'
import type { FileFormat } from './workspace'

export interface HistoryVersion {
  id: string
  /** 版本所属的日报日期，yyyymmdd。 */
  date: string
  /** 保存时间，毫秒时间戳。 */
  savedAt: number
  format: FileFormat
  bytes: number
}

export const historyApi = {
  /** 不传日期时返回全部日期，按日期倒序、同日按时间倒序。 */
  list: (date?: string) => invoke<HistoryVersion[]>('list_entry_history', { date: date ?? null }),
  read: (date: string, id: string) => invoke<string>('read_entry_history', { date, id }),
}
