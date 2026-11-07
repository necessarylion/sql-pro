import type { ExportFormat } from '../types'

/** The name the UI shows for each export format, in the order the export dialog and the row menu show them. */
export const EXPORT_FORMAT_LABELS: Record<ExportFormat, string> = {
  sql: 'SQL',
  csv: 'CSV',
  json: 'JSON',
  xlsx: 'Excel',
}
