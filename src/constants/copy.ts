import type { CopyFormat } from '../types'

/** One item of the "Copy Rows As" submenu. */
export interface CopyFormatItem {
  format: CopyFormat
  label: string
}

/** The "Copy Rows As" items. Each inner list is one group; a line goes between groups. */
export const COPY_FORMAT_GROUPS: CopyFormatItem[][] = [
  [{ format: 'text', label: 'Plain Text' }],
  [
    { format: 'json', label: 'JSON' },
    { format: 'html', label: 'HTML' },
  ],
  [{ format: 'markdown', label: 'Markdown Table' }],
  [
    { format: 'csv', label: 'CSV' },
    { format: 'csvHeader', label: 'CSV with Header' },
  ],
  [{ format: 'insert', label: 'INSERT Statement' }],
]

/** The width the "Copy Rows As" submenu gets. It opens to the left when there is no room on the right. */
export const COPY_SUBMENU_WIDTH = 160

/** JSON is indented by this many spaces. */
export const COPY_JSON_INDENT = 2
