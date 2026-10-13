import { PAGE_SIZE } from '../constants'
import { RAW_FILTER_COLUMN, VALUELESS_OPERATORS } from '../constants/filters'
import type {
  ColumnSpec,
  ConnectionConfig,
  FilterRow,
  FilterRule,
  ResultSet,
  Row,
  SessionView,
  TablePage,
  TableRef,
  Values,
} from '../types'
import type { DatabaseApi } from './DatabaseApi'

const NEW_TITLE = 'New connection'

/**
 * All state and logic of one tab.
 * The methods throw on failure. Components wrap them in `run`.
 */
export class Session {
  id = crypto.randomUUID()
  title = NEW_TITLE
  /** null = not connected. The password is never kept here. */
  config: ConnectionConfig | null = null
  busy = false
  error = ''
  databases: string[] = []
  tables: TableRef[] = []
  table: TableRef | null = null
  view: SessionView = 'content'
  page: TablePage | null = null
  offset = 0
  /** The raw SQL filter that is applied now. */
  filter = ''
  /** The filter rules that are applied now. */
  filters: FilterRule[] = []
  /** The rows of the filter area. They do nothing until `applyFilters` runs. */
  filterRows: FilterRow[] = [this.emptyFilterRow()]
  orderBy = ''
  descending = false
  /** A new row that is not saved yet. It is shown after the last page row. */
  draft: Row | null = null
  sql = ''
  result: ResultSet | null = null
  status = ''
  private api: DatabaseApi

  constructor(api: DatabaseApi) {
    this.api = api
  }

  /** True when rows of the open table can be changed. */
  get editable(): boolean {
    if (!this.page || this.table?.kind !== 'table') {
      return false
    }
    for (const column of this.page.columns) {
      if (column.primaryKey) {
        return true
      }
    }
    return false
  }

  /** True when a filter is applied. The row total then counts only matching rows. */
  get filtered(): boolean {
    return this.filter !== '' || this.filters.length > 0
  }

  /**
   * The one place that handles busy state and errors.
   * Returns true when the work finished without an error.
   */
  async run(work: Promise<unknown>): Promise<boolean> {
    this.busy = true
    this.error = ''
    try {
      await work
      return true
    } catch (error) {
      this.error = String(error)
      return false
    } finally {
      this.busy = false
    }
  }

  async connect(config: ConnectionConfig, title: string): Promise<void> {
    await this.api.connect(this.id, config)
    this.config = { ...config, password: '', ssh: null }
    this.title = title
    this.databases = await this.api.listDatabases(this.id)
    await this.loadTables()
  }

  async disconnect(): Promise<void> {
    try {
      await this.api.disconnect(this.id)
    } finally {
      // Go back to the connection form even when the backend fails.
      this.config = null
      this.title = NEW_TITLE
      this.databases = []
      this.tables = []
      this.result = null
      this.status = ''
      this.clearTable()
    }
  }

  async useDatabase(name: string): Promise<void> {
    await this.api.useDatabase(this.id, name)
    if (this.config) {
      this.config.database = name
    }
    this.clearTable()
    await this.loadTables()
  }

  async loadTables(): Promise<void> {
    this.tables = await this.api.listTables(this.id)
  }

  async selectTable(table: TableRef): Promise<void> {
    this.clearTable()
    this.table = table
    if (this.view === 'query') {
      this.view = 'content'
    }
    await this.loadPage()
  }

  async loadPage(): Promise<void> {
    const table = this.table
    if (!table) {
      return
    }
    const page = await this.api.browse(this.id, {
      table,
      filter: this.filter,
      filters: this.filters,
      orderBy: this.orderBy,
      descending: this.descending,
      limit: PAGE_SIZE,
      offset: this.offset,
    })
    // The user may have picked another table while this one was loading.
    if (this.table === table) {
      this.page = page
      // A filter row made before the columns were known gets the first column now.
      for (const row of this.filterRows) {
        if (row.column === '') {
          row.column = page.columns[0]?.name ?? ''
        }
      }
    }
  }

  async sort(column: string): Promise<void> {
    this.descending = this.orderBy === column && !this.descending
    this.orderBy = column
    this.offset = 0
    await this.loadPage()
  }

  /**
   * Applies all filter rows that are ticked and complete. Loads the first page.
   * Normal rows go to `filters`. Raw SQL rows are joined with AND into `filter`.
   */
  async applyFilters(): Promise<void> {
    const filters: FilterRule[] = []
    const raw: string[] = []
    for (const row of this.filterRows) {
      const valueless = VALUELESS_OPERATORS.includes(row.operator)
      if (!row.enabled) {
        continue
      }
      if (row.column === RAW_FILTER_COLUMN) {
        if (row.value.trim()) {
          raw.push(`(${row.value.trim()})`)
        }
      } else if (row.column && (valueless || row.value !== '')) {
        filters.push({
          column: row.column,
          operator: row.operator,
          value: valueless ? '' : row.value,
        })
      }
    }
    this.filters = filters
    this.filter = raw.join(' AND ')
    this.offset = 0
    await this.loadPage()
  }

  /** Adds an empty filter row below the row at `index`. */
  addFilterRow(index: number): void {
    this.filterRows.splice(index + 1, 0, this.emptyFilterRow())
  }

  /** Removes a filter row and applies the rest. There is always at least one row. */
  async removeFilterRow(index: number): Promise<void> {
    this.filterRows.splice(index, 1)
    if (this.filterRows.length === 0) {
      this.filterRows.push(this.emptyFilterRow())
    }
    await this.applyFilters()
  }

  async turnPage(direction: 1 | -1): Promise<void> {
    const next = this.offset + direction * PAGE_SIZE
    if (!this.page || next < 0 || next >= this.page.total) {
      return
    }
    this.offset = next
    await this.loadPage()
  }

  async updateCell(rowIndex: number, columnIndex: number, value: string | null): Promise<void> {
    if (!this.page || !this.table) {
      return
    }
    if (this.draft && rowIndex === this.page.rows.length) {
      this.draft[columnIndex] = value
      return
    }
    const changes: Values = { [this.page.columns[columnIndex].name]: value }
    const count = await this.api.updateRow(
      this.id,
      this.table,
      this.keyOf(this.page.rows[rowIndex]),
      changes,
    )
    this.status = `${this.plural(count, 'row')} updated`
    await this.loadPage()
  }

  addDraft(): void {
    if (this.page && !this.draft) {
      this.draft = new Array<string | null>(this.page.columns.length).fill(null)
    }
  }

  discardDraft(): void {
    this.draft = null
  }

  /** Sends only the columns the user set. The database fills in the rest. */
  async saveDraft(): Promise<void> {
    if (!this.page || !this.table || !this.draft) {
      return
    }
    const values: Values = {}
    for (const [index, column] of this.page.columns.entries()) {
      if (this.draft[index] !== null) {
        values[column.name] = this.draft[index]
      }
    }
    const count = await this.api.insertRow(this.id, this.table, values)
    this.draft = null
    this.status = `${this.plural(count, 'row')} added`
    await this.loadPage()
  }

  async deleteRows(indexes: number[]): Promise<void> {
    if (!this.page || !this.table) {
      return
    }
    const keys: Values[] = []
    for (const index of indexes) {
      if (index < this.page.rows.length) {
        keys.push(this.keyOf(this.page.rows[index]))
      } else {
        // The draft row is not in the database yet.
        this.draft = null
      }
    }
    if (keys.length === 0) {
      return
    }
    const count = await this.api.deleteRows(this.id, this.table, keys)
    this.status = `${this.plural(count, 'row')} deleted`
    await this.loadPage()
  }

  async addColumn(spec: ColumnSpec): Promise<void> {
    if (!this.table) {
      return
    }
    await this.api.addColumn(this.id, this.table, spec)
    this.status = `Column "${spec.name}" added`
    await this.loadPage()
  }

  async dropColumn(name: string): Promise<void> {
    if (!this.table) {
      return
    }
    await this.api.dropColumn(this.id, this.table, name)
    if (this.orderBy === name) {
      this.orderBy = ''
    }
    this.status = `Column "${name}" dropped`
    await this.loadPage()
  }

  /** `name` is the current name of the column. `spec` is how it should look after. */
  async alterColumn(name: string, spec: ColumnSpec): Promise<void> {
    if (!this.table) {
      return
    }
    try {
      await this.api.alterColumn(this.id, this.table, name, spec)
      if (this.orderBy === name) {
        this.orderBy = spec.name
      }
      this.status = `Column "${spec.name}" changed`
    } finally {
      // Reload even after an error. A part of the change may be done already.
      await this.loadPage()
    }
  }

  /** Removes a table or a view. The backend picks DROP TABLE or DROP VIEW. */
  async dropTable(table: TableRef): Promise<void> {
    await this.api.dropTable(this.id, table)
    if (this.isOpen(table)) {
      this.clearTable()
    }
    this.status = `${table.kind === 'view' ? 'View' : 'Table'} "${table.name}" dropped`
    await this.loadTables()
  }

  /** Deletes all rows of a table. */
  async truncateTable(table: TableRef): Promise<void> {
    await this.api.truncateTable(this.id, table)
    this.status = `Table "${table.name}" truncated`
    if (this.isOpen(table)) {
      this.offset = 0
      await this.loadPage()
    }
  }

  /** The file name we offer for a backup made at `now`, like `shop-2026-01-31-0905.sql`. */
  backupName(now: Date): string {
    let name = this.config?.database ?? ''
    if (this.config?.driver === 'sqlite') {
      // The file name without its folder and its ending.
      name = (name.split(/[\\/]/).pop() ?? '').replace(/\.[^.]*$/, '')
    }
    const day = `${now.getFullYear()}-${this.two(now.getMonth() + 1)}-${this.two(now.getDate())}`
    const time = `${this.two(now.getHours())}${this.two(now.getMinutes())}`
    return `${name || 'backup'}-${day}-${time}.sql`
  }

  /** Writes a SQL dump of the current database to the file at `path`. */
  async backup(path: string): Promise<void> {
    await this.api.dumpDatabase(this.id, path)
    this.status = `Backup saved to ${path}`
  }

  /** The table list is replaced on refresh, so compare by name and not by object. */
  isOpen(table: TableRef): boolean {
    return this.table?.name === table.name && this.table.schema === table.schema
  }

  /** Shows the last result that has columns. Adds up the affected rows of the rest. */
  async runQuery(sql: string): Promise<void> {
    const started = performance.now()
    const sets = await this.api.runQuery(this.id, sql)
    const seconds = ((performance.now() - started) / 1000).toFixed(2)

    let result: ResultSet | null = null
    let affected = 0
    for (const set of sets) {
      if (set.columns.length > 0) {
        result = set
      } else {
        affected += set.affected
      }
    }
    this.result = result
    if (result) {
      const cut = result.truncated ? ' (truncated)' : ''
      const changed = affected > 0 ? `, ${this.plural(affected, 'row')} affected` : ''
      this.status = `${this.plural(result.rows.length, 'row')}${cut}${changed} in ${seconds}s`
    } else {
      this.status = `OK, ${this.plural(affected, 'row')} affected in ${seconds}s`
    }
    // The query may have created or dropped tables.
    await this.loadTables()
  }

  /** The primary key columns of a row, with their current values. */
  private keyOf(row: Row): Values {
    const key: Values = {}
    for (const [index, column] of (this.page?.columns ?? []).entries()) {
      if (column.primaryKey) {
        key[column.name] = row[index]
      }
    }
    return key
  }

  private clearTable(): void {
    this.table = null
    this.page = null
    this.draft = null
    this.offset = 0
    this.filter = ''
    this.filters = []
    this.filterRows = [this.emptyFilterRow()]
    this.orderBy = ''
    this.descending = false
  }

  /** A new filter row on the first column of the table. */
  private emptyFilterRow(): FilterRow {
    return {
      enabled: true,
      column: this.page?.columns[0]?.name ?? '',
      operator: 'eq',
      value: '',
    }
  }

  /** A number with two digits, like 05. */
  private two(value: number): string {
    return String(value).padStart(2, '0')
  }

  private plural(count: number, word: string): string {
    return `${count.toLocaleString('en-US')} ${word}${count === 1 ? '' : 's'}`
  }
}
