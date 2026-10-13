import type { FilterOperator } from '../types'

/** The order here is the order of the entries in the operator select. */
export const FILTER_OPERATOR_LABELS: Record<FilterOperator, string> = {
  eq: '=',
  neq: '≠',
  lt: '<',
  gt: '>',
  lte: '≤',
  gte: '≥',
  contains: 'contains',
  notContains: 'does not contain',
  startsWith: 'starts with',
  endsWith: 'ends with',
  in: 'IN (a, b, c)',
  isNull: 'IS NULL',
  isNotNull: 'IS NOT NULL',
}

/** These operators need no value. */
export const VALUELESS_OPERATORS: FilterOperator[] = ['isNull', 'isNotNull']

/** A filter row with this as its column holds a raw SQL WHERE body. */
export const RAW_FILTER_COLUMN = '__raw_sql__'
