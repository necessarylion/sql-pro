import type { Driver, PlanProblem } from '../types'

/** The badge text of each plan problem. */
export const PLAN_PROBLEM_LABELS: Record<PlanProblem, string> = {
  fullScan: 'Full scan',
  filesort: 'Filesort',
  tempTable: 'Temp table',
  rowEstimate: 'Rows off',
}

/** The tooltip of each plan problem badge. */
export const PLAN_PROBLEM_HINTS: Record<PlanProblem, string> = {
  fullScan: 'Reads every row of the table. An index can help.',
  filesort: 'Sorts the rows itself, because no index gives the order.',
  tempTable: 'Writes rows to a temporary table or to disk.',
  rowEstimate:
    'The real row count is 10 times or more off the estimate. Fresh table statistics can help.',
}

/** Databases that give no real times, so Analyze is not offered. */
export const NO_ANALYZE_DRIVERS: Driver[] = ['sqlite']

/** The status line after Explain. */
export const PLAN_STATUS = 'Query plan. The statement did not run.'

/** The status line after Analyze. */
export const ANALYZE_STATUS = 'Query plan with real times. The read ran once, then was rolled back.'

/** The plan tree: the narrowest the step column gets, in px. Longer names are cut. */
export const PLAN_STEP_MIN = 240

/** The plan tree: the width of each number column, in px. */
export const PLAN_NUMBER_WIDTH = 76

/** The plan tree: the gap between the columns, in px. Same as `gap` in PlanView.vue. */
export const PLAN_COLUMN_GAP = 8
