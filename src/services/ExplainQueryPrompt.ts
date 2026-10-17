import { DRIVER_LABELS } from '../constants'
import type { Driver, QueryStat } from '../types'
import { QueryStatList } from './QueryStatList'

/** How each server writes the values it took out of a query. SQLite has no query stats. */
const PLACEHOLDER_NOTES: Record<Driver, string> = {
  mysql:
    'The server put a ? in the place of each real value. So the ? marks are placeholders, not part of the real query.',
  postgres:
    'The server put $1, $2 and so on in the place of the real values. They are placeholders, not part of the real query.',
  mssql:
    'The server may keep the real values, or show them as parameters like @P1. Parameters are placeholders for real values.',
  sqlite: '',
}

/** The question for the AI chat: what this query does, how the server runs it, and whether it can be faster. */
export class ExplainQueryPrompt {
  static build(stat: QueryStat, driver: Driver, database: string): string {
    const lines = [
      `Explain this query from my ${DRIVER_LABELS[driver]} database "${stat.database ?? database}": what it does, how the server runs it, and whether it can be made faster.`,
      '',
      'The query, as the server keeps it:',
      stat.query,
      '',
    ]
    const note = PLACEHOLDER_NOTES[driver]
    if (note) {
      lines.push(note, '')
    }
    lines.push('Its stats:', ...ExplainQueryPrompt.stats(stat), '')
    lines.push(
      'Please do this:',
      '1. Check the plan with EXPLAIN. Put sample values in the place of the placeholders. Take them from the table data when you can.',
      '2. Look at the structure and the indexes of the tables it uses.',
      '3. Explain in plain words what costs the most time.',
      '4. Suggest concrete fixes, if there are any: new indexes as CREATE INDEX SQL, and rewrites of the query. If the query is already fine, say so plainly.',
      '',
      'Do not change anything. Only read. Do not run the CREATE INDEX or any other change.',
    )
    return lines.join('\n')
  }

  private static stats(stat: QueryStat): string[] {
    const lines = [
      `- Runs: ${stat.runs}`,
      `- Total time: ${QueryStatList.duration(stat.totalMs)}`,
      `- Average time: ${QueryStatList.duration(stat.averageMs)}`,
    ]
    if (stat.maxMs !== null) {
      lines.push(`- Max time: ${QueryStatList.duration(stat.maxMs)}`)
    }
    if (stat.rows !== null) {
      lines.push(`- Rows returned (all runs): ${stat.rows}`)
    }
    if (stat.lastRun !== null) {
      lines.push(`- Last run: ${stat.lastRun}`)
    }
    return lines
  }
}
