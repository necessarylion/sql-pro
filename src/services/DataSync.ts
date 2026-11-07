import { DATA_RUN_TITLE } from '../constants/schemaSync'
import type { DataDiff, DataKinds, TableRef } from '../types'
import type { DatabaseApi } from './DatabaseApi'
import type { DiscardPrompt } from './DiscardPrompt'
import { SchemaSync } from './SchemaSync'
import type { Session } from './Session'

/**
 * The Data mode of the Compare view: one table in two tabs, the rows that differ, and the
 * SQL that makes the target rows like the source rows. The SQL never runs without the
 * user's yes. The methods throw on failure. The view wraps them in `session.run`.
 */
export class DataSync {
  sourceId = ''
  targetId = ''
  /** The label of the picked table, like `public.users`. '' = none. */
  tableLabel = ''
  /** null = not compared yet. */
  diff: DataDiff | null = null
  kinds: DataKinds = { insert: true, update: true, delete: true }
  /** The table and the databases of the two tabs at the compare. */
  private table: TableRef | null = null
  private databases = ['', '']
  private api: DatabaseApi
  private prompt: DiscardPrompt

  constructor(api: DatabaseApi, prompt: DiscardPrompt) {
    this.api = api
    this.prompt = prompt
  }

  /** The tables (no views) that both tabs have, by schema and name. */
  static tables(source: Session | undefined, target: Session | undefined): TableRef[] {
    const theirs = new Set<string>()
    for (const table of target?.tables ?? []) {
      theirs.add(SchemaSync.label(table))
    }
    const both: TableRef[] = []
    for (const table of source?.tables ?? []) {
      if (table.kind !== 'view' && theirs.has(SchemaSync.label(table))) {
        both.push(table)
      }
    }
    return both
  }

  /** Picks another source or target tab. The old result does not fit it. */
  pickTabs(sourceId: string, targetId: string): void {
    this.sourceId = sourceId
    this.targetId = targetId
    this.clear()
  }

  /** Picks another table. The old result does not fit it. */
  pickTable(label: string): void {
    this.tableLabel = label
    this.clear()
  }

  /** Forgets the result, like after a switch to another database. */
  clear(): void {
    this.diff = null
    this.table = null
  }

  /** Compares the rows of the picked table in the two tabs. */
  async compare(source: Session | undefined, target: Session | undefined): Promise<void> {
    const problem = SchemaSync.problem(source, target)
    if (problem || !source || !target) {
      throw problem
    }
    const table = DataSync.tables(source, target).find(this.isPicked, this)
    if (!table) {
      throw 'Pick a table that both tabs have.'
    }
    this.sourceId = source.id
    this.targetId = target.id
    this.clear()
    this.databases = [DataSync.database(source), DataSync.database(target)]
    this.diff = await this.api.compareData(source.id, target.id, table)
    this.table = table
  }

  /** Puts a kind in or out of the SQL. The result stays. */
  toggle(kind: keyof DataKinds): void {
    this.kinds = { ...this.kinds, [kind]: !this.kinds[kind] }
  }

  /** The statements of the ticked kinds, in the order the backend runs them. */
  get statements(): string[] {
    const diff = this.diff
    if (!diff || diff.limited) {
      return []
    }
    const statements: string[] = []
    if (this.kinds.delete) {
      statements.push(...diff.deletes)
    }
    if (this.kinds.update) {
      statements.push(...diff.updates)
    }
    if (this.kinds.insert) {
      statements.push(...diff.inserts)
    }
    return statements
  }

  /** The SQL as one text, one statement per line. */
  get script(): string {
    const statements = this.statements
    return statements.length === 0 ? '' : `${statements.join(';\n')};\n`
  }

  /** True when the result fits the two tabs as they are now: same tabs, same databases. */
  fits(source: Session | undefined, target: Session | undefined): boolean {
    return (
      this.table !== null &&
      source?.id === this.sourceId &&
      target?.id === this.targetId &&
      DataSync.database(source) === this.databases[0] &&
      DataSync.database(target) === this.databases[1]
    )
  }

  /** Puts the SQL into the Query view of the target tab. The user runs it there. */
  openInQuery(source: Session, target: Session): void {
    this.checkFits(source, target)
    target.openInQuery(this.script)
  }

  /** Asks first, then runs the SQL on the target tab and compares again. False when the user said no. */
  async runOnTarget(source: Session, target: Session): Promise<boolean> {
    const statements = this.statements
    if (statements.length === 0 || !this.table) {
      throw 'There is no SQL to run.'
    }
    this.checkFits(source, target)
    if (!(await this.prompt.ask(this.runQuestion(target), DATA_RUN_TITLE))) {
      return false
    }
    // The backend compares again and runs only when it is still this SQL.
    await this.api.runDataSync(
      source.id,
      target.id,
      this.table,
      this.kinds,
      statements,
      this.diff?.database ?? null,
    )
    await this.compare(source, target)
    return true
  }

  /** The confirm text of a run. It names how many rows change. */
  runQuestion(target: Session): string {
    const diff = this.diff
    const parts: string[] = []
    if (diff && this.kinds.delete && diff.deleteRows > 0) {
      parts.push(`deletes ${DataSync.rows(diff.deleteRows)}`)
    }
    if (diff && this.kinds.update && diff.updateRows > 0) {
      parts.push(`updates ${DataSync.rows(diff.updateRows)}`)
    }
    if (diff && this.kinds.insert && diff.insertRows > 0) {
      parts.push(`inserts ${DataSync.rows(diff.insertRows)}`)
    }
    return (
      `Run the SQL on "${target.title}" (${this.tableLabel})?\n\n` +
      `It ${parts.join(', ')}. This cannot be undone.`
    )
  }

  private isPicked(table: TableRef): boolean {
    return SchemaSync.label(table) === this.tableLabel
  }

  private checkFits(source: Session, target: Session): void {
    if (!this.fits(source, target)) {
      throw 'The SQL was made for other tabs or databases. Compare again.'
    }
  }

  private static database(tab: Session | undefined): string {
    return tab?.config?.database ?? ''
  }

  private static rows(amount: number): string {
    return `${amount} ${amount === 1 ? 'row' : 'rows'}`
  }
}
