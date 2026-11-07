import { DRIVER_LABELS } from '../constants'
import {
  CHANGED_TABLES,
  COLUMN_CHANGE_ACTIONS,
  DATA_LOSS_ACTIONS,
  ONLY_IN_SOURCE,
  ONLY_IN_TARGET,
  SYNC_RUN_TITLE,
} from '../constants/schemaSync'
import type { SchemaChange, SyncSql, TableRef } from '../types'
import type { DatabaseApi } from './DatabaseApi'
import type { DiscardPrompt } from './DiscardPrompt'
import type { Session } from './Session'

/** The differences of one table in the Compare view. */
export interface TableChanges {
  label: string
  changes: SchemaChange[]
}

/** One group of the Compare view, like "Only in source". */
export interface ChangeGroup {
  title: string
  tables: TableChanges[]
}

/**
 * The state of the Compare view of one tab: the two tabs, their differences, the picked
 * differences and the SQL for them. The SQL never runs without the user's yes.
 * The methods throw on failure. The view wraps them in `session.run`.
 */
export class SchemaSync {
  sourceId = ''
  targetId = ''
  /** null = not compared yet. */
  changes: SchemaChange[] | null = null
  /** The ids of the differences that go into the SQL. */
  picked: string[] = []
  /** null = no SQL for the picked differences yet. */
  sql: SyncSql | null = null
  /** The databases of the two tabs at the compare. A tab can switch its database later. */
  private databases = ['', '']
  private api: DatabaseApi
  private prompt: DiscardPrompt

  constructor(api: DatabaseApi, prompt: DiscardPrompt) {
    this.api = api
    this.prompt = prompt
  }

  /** The text to show when the two tabs cannot be compared. '' when they can. */
  static problem(source: Session | undefined, target: Session | undefined): string {
    if (!source?.config || !target?.config) {
      return 'Pick a source and a target tab. Both must be connected.'
    }
    if (source.id === target.id) {
      return 'Pick two different tabs.'
    }
    const from = source.config.driver
    const to = target.config.driver
    if (from !== to) {
      return (
        `Both tabs must use the same kind of database. ` +
        `The source is ${DRIVER_LABELS[from]} and the target is ${DRIVER_LABELS[to]}.`
      )
    }
    return ''
  }

  /** The name of a table, with its schema in front when it has one. */
  static label(table: TableRef): string {
    return table.schema ? `${table.schema}.${table.name}` : table.name
  }

  /** Compares the two tabs, picks all differences and makes their SQL. */
  async compare(source: Session | undefined, target: Session | undefined): Promise<void> {
    const problem = SchemaSync.problem(source, target)
    if (problem || !source || !target) {
      throw problem
    }
    this.sourceId = source.id
    this.targetId = target.id
    this.changes = null
    this.sql = null
    this.databases = [SchemaSync.database(source), SchemaSync.database(target)]
    const changes = await this.api.compareSchemas(source.id, target.id)
    this.changes = changes
    this.picked = []
    for (const change of changes) {
      this.picked.push(change.id)
    }
    if (changes.length > 0) {
      await this.makeSql()
    }
  }

  /** Picks another source or target tab. The old differences and SQL do not fit it. */
  pickTabs(sourceId: string, targetId: string): void {
    this.sourceId = sourceId
    this.targetId = targetId
    this.clear()
  }

  /** Forgets the differences and the SQL, like after a switch to another database. */
  clear(): void {
    this.changes = null
    this.picked = []
    this.sql = null
  }

  isPicked(change: SchemaChange): boolean {
    return this.picked.includes(change.id)
  }

  /** Picks or unpicks one difference. The old SQL no longer fits, so it goes. */
  toggle(change: SchemaChange): void {
    if (this.isPicked(change)) {
      this.picked = this.picked.filter(this.otherThan(change.id))
    } else {
      this.picked.push(change.id)
    }
    this.sql = null
  }

  /** Picks all differences of `changes`, or none of them when all are picked now. */
  toggleAll(changes: SchemaChange[]): void {
    const all = changes.every(this.isPicked, this)
    for (const change of changes) {
      if (this.isPicked(change) === all) {
        this.toggle(change)
      }
    }
    this.sql = null
  }

  /** True when every difference of `changes` is picked. */
  allPicked(changes: SchemaChange[]): boolean {
    return changes.length > 0 && changes.every(this.isPicked, this)
  }

  /** Makes the SQL of the picked differences. Nothing runs. */
  async makeSql(): Promise<void> {
    this.sql = await this.api.schemaSyncSql(this.sourceId, this.targetId, this.picked)
  }

  /** The SQL as one text, one statement after the other. */
  get script(): string {
    const statements = this.sql?.statements ?? []
    return statements.length === 0 ? '' : `${statements.join(';\n\n')};\n`
  }

  /** The differences in three groups: new tables, dropped tables, and changed tables. */
  groups(): ChangeGroup[] {
    const created: TableChanges[] = []
    const dropped: TableChanges[] = []
    const changed: TableChanges[] = []
    for (const change of this.changes ?? []) {
      const label = SchemaSync.label(change.table)
      if (change.action === 'createTable') {
        created.push({ label, changes: [change] })
      } else if (change.action === 'dropTable') {
        dropped.push({ label, changes: [change] })
      } else if (changed.at(-1)?.label === label) {
        changed.at(-1)?.changes.push(change)
      } else {
        // The backend sorts by table, so the changes of one table come together.
        changed.push({ label, changes: [change] })
      }
    }
    const groups: ChangeGroup[] = [
      { title: ONLY_IN_SOURCE, tables: created },
      { title: ONLY_IN_TARGET, tables: dropped },
      { title: CHANGED_TABLES, tables: changed },
    ]
    return groups.filter(SchemaSync.hasTables)
  }

  /**
   * True when the SQL fits the two tabs as they are now: the same tabs, on the same databases.
   * Else it must never be used, because it would change another database.
   */
  fits(source: Session | undefined, target: Session | undefined): boolean {
    return (
      source?.id === this.sourceId &&
      target?.id === this.targetId &&
      SchemaSync.database(source) === this.databases[0] &&
      SchemaSync.database(target) === this.databases[1]
    )
  }

  /** Puts the SQL into the Query view of the target tab. The user runs it there. */
  openInQuery(source: Session, target: Session): void {
    this.checkFits(source, target)
    target.openInQuery(this.script)
  }

  /**
   * Asks first, then runs the SQL on the target tab and compares again.
   * Gives false when the user said no.
   */
  async runOnTarget(source: Session, target: Session): Promise<boolean> {
    const statements = this.sql?.statements ?? []
    if (statements.length === 0) {
      throw 'There is no SQL to run. Pick differences and make the SQL first.'
    }
    this.checkFits(source, target)
    if (!(await this.prompt.ask(this.runQuestion(target), SYNC_RUN_TITLE))) {
      return false
    }
    // The backend builds the SQL again and runs it only when it is still this SQL.
    await this.api.runSchemaSync(source.id, target.id, this.picked, statements)
    await target.loadTables()
    await this.compare(source, target)
    return true
  }

  /** The confirm text of a run. It names what deletes data. */
  private runQuestion(target: Session): string {
    const count = this.sql?.statements.length ?? 0
    const lines = [`Run ${count} ${count === 1 ? 'statement' : 'statements'} on "${target.title}"?`]
    let tables = 0
    let columns = 0
    let changed = 0
    for (const change of this.changes ?? []) {
      if (!this.isPicked(change)) {
        continue
      }
      if (COLUMN_CHANGE_ACTIONS.includes(change.action)) {
        changed += 1
      } else if (DATA_LOSS_ACTIONS.includes(change.action)) {
        if (change.action === 'dropTable') {
          tables += 1
        } else {
          columns += 1
        }
      }
    }
    if (tables + columns > 0) {
      lines.push(
        `It drops ${SchemaSync.count(tables, 'table')} and ${SchemaSync.count(columns, 'column')}, ` +
          'with all their data. This cannot be undone.',
      )
    }
    if (changed > 0) {
      lines.push(
        `It changes ${SchemaSync.count(changed, 'column')}. ` +
          'Values that do not fit a new type can be cut or lost, or the change fails.',
      )
    }
    if (target.config?.driver === 'mysql') {
      lines.push('MySQL cannot undo a schema change. When one fails, the ones before it stay.')
    }
    return lines.join('\n\n')
  }

  private checkFits(source: Session, target: Session): void {
    if (!this.fits(source, target)) {
      throw 'The SQL was made for other tabs or databases. Compare again.'
    }
  }

  private static database(tab: Session | undefined): string {
    return tab?.config?.database ?? ''
  }

  private static count(amount: number, noun: string): string {
    return `${amount} ${noun}${amount === 1 ? '' : 's'}`
  }

  private static hasTables(group: ChangeGroup): boolean {
    return group.tables.length > 0
  }

  /** A filter that keeps every id but `id`. */
  private otherThan(id: string): (picked: string) => boolean {
    return function keep(picked: string): boolean {
      return picked !== id
    }
  }
}
