import type { ChangeAction } from '../types'

/** The short words in front of each difference in the Compare view. */
export const CHANGE_LABELS: Record<ChangeAction, string> = {
  createTable: 'New table',
  dropTable: 'Drop table',
  addColumn: 'Add column',
  dropColumn: 'Drop column',
  alterColumn: 'Change column',
  addIndex: 'Add index',
  dropIndex: 'Drop index',
  alterIndex: 'Change index',
  addForeignKey: 'Add foreign key',
  dropForeignKey: 'Drop foreign key',
}

/** The differences that delete data in the target. The run confirm names them. */
export const DATA_LOSS_ACTIONS: readonly ChangeAction[] = ['dropTable', 'dropColumn']

/** The differences that change a column. A new type can lose data that does not fit. */
export const COLUMN_CHANGE_ACTIONS: readonly ChangeAction[] = ['alterColumn']

/** The titles of the three groups of the Compare view. */
export const ONLY_IN_SOURCE = 'Only in source'
export const ONLY_IN_TARGET = 'Only in target'
export const CHANGED_TABLES = 'Changed tables'

export const SYNC_RUN_TITLE = 'Run on target'
