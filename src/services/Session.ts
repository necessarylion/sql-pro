import {
  DELETE_ROWS_TITLE,
  OBJECT_KIND_LABELS,
  PAGE_SIZE,
  QUERY_CANCELLED_STATUS,
  QUERY_RESULT_NAME,
} from '../constants'
import { DIAGRAM_SAVED_STATUS_MS } from '../constants/diagram'
import { RAW_FILTER_COLUMN, VALUELESS_OPERATORS } from '../constants/filters'
import { ACCESS_LABELS } from '../constants/users'
import type {
  ColumnInfo,
  ColumnSpec,
  ConnectionConfig,
  CsvImport,
  CsvPreview,
  DbObject,
  DbUser,
  Diagram,
  ExportFormat,
  ExportRequest,
  RowsExport,
  GrantableAccess,
  NewForeignKey,
  NewIndex,
  NewUser,
  ObjectDefinition,
  FilterRow,
  FilterRule,
  ResultSet,
  Row,
  RowUpdate,
  SessionView,
  TableKeys,
  TablePage,
  TableRef,
  Values,
} from '../types'
import type { DatabaseApi } from './DatabaseApi'
import type { DiscardPrompt } from './DiscardPrompt'
import { Environment } from './Environment'
import { TableDraft } from './TableDraft'
import { TableSelection } from './TableSelection'
import type { ColumnDraft } from './TableDraft'

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
  /** The environment of the open connection, like `dev` or `prod`. '' = none. */
  environment = ''
  busy = false
  /** True while a query or a page load runs that the user may cancel. */
  cancellable = false
  /** True after the user asked to cancel. The run then ends with a short status, not an error. */
  cancelling = false
  error = ''
  databases: string[] = []
  tables: TableRef[] = []
  /** The triggers and routines of the current database. */
  objects: DbObject[] = []
  table: TableRef | null = null
  /** The SQL of a view, trigger or routine. It is shown in the place of a table. */
  definition: ObjectDefinition | null = null
  view: SessionView = 'content'
  page: TablePage | null = null
  /** The indexes and foreign keys of the open table. Loaded only for the Structure view. */
  keys: TableKeys | null = null
  offset = 0
  /** The raw SQL filter that is applied now. */
  filter = ''
  /** The filter rules that are applied now. */
  filters: FilterRule[] = []
  /** The rows of the filter area. They do nothing until `applyFilters` runs. */
  filterRows: FilterRow[] = [this.emptyFilterRow()]
  orderBy = ''
  descending = false
  /**
   * Cell edits that are not saved yet. One entry per changed row.
   * The name of an entry is the primary key of the row as it was loaded.
   */
  edits: Record<string, RowUpdate> = {}
  /** New rows that are not saved yet. They are shown after the last page row. */
  newRows: Row[] = []
  /**
   * Loaded rows that are marked as deleted. They are deleted on save.
   * The name of an entry is the primary key of the row. The value is that key.
   */
  deletes: Record<string, Values> = {}
  sql = ''
  result: ResultSet | null = null
  status = ''
  /** The accounts of the server, with their access on the current database. */
  users: DbUser[] = []
  /** True when the logged-in account may manage users. The Users view needs it. */
  canManageUsers = false
  /** What the Diagram view shows. null = not loaded. */
  diagram: Diagram | null = null
  /** Goes up by one on each press of the export shortcut. The workspace then opens the export dialog. */
  exportAsked = 0
  /** The timer that removes the "Diagram saved" status. */
  private statusTimer: ReturnType<typeof setTimeout> | undefined
  private api: DatabaseApi
  private prompt: DiscardPrompt

  constructor(api: DatabaseApi, prompt: DiscardPrompt) {
    this.api = api
    this.prompt = prompt
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

  /** The number of changed cells, plus the new rows, plus the rows marked as deleted. */
  get pendingCount(): number {
    let count = this.newRows.length + Object.keys(this.deletes).length
    for (const update of Object.values(this.edits)) {
      count += Object.keys(update.changes).length
    }
    return count
  }

  /** Like "4 unsaved changes". */
  get unsavedText(): string {
    return this.plural(this.pendingCount, 'unsaved change')
  }

  /** The rows the grid shows: the page rows with their pending edits, then the new rows. */
  get shownRows(): Row[] {
    if (!this.page) {
      return []
    }
    const rows: Row[] = []
    const noEdits = Object.keys(this.edits).length === 0
    for (const row of this.page.rows) {
      const changes = noEdits ? undefined : this.edits[this.rowId(row)]?.changes
      if (!changes) {
        rows.push(row)
        continue
      }
      const shown = [...row]
      for (const [index, column] of this.page.columns.entries()) {
        if (column.name in changes) {
          shown[index] = changes[column.name]
        }
      }
      rows.push(shown)
    }
    return [...rows, ...this.newRows]
  }

  /** The cells with a pending edit: row number on the page -> column numbers. */
  get changedCells(): Record<number, number[]> {
    const cells: Record<number, number[]> = {}
    if (!this.page || Object.keys(this.edits).length === 0) {
      return cells
    }
    for (const [rowIndex, row] of this.page.rows.entries()) {
      const changes = this.edits[this.rowId(row)]?.changes
      if (!changes) {
        continue
      }
      cells[rowIndex] = []
      for (const [index, column] of this.page.columns.entries()) {
        if (column.name in changes) {
          cells[rowIndex].push(index)
        }
      }
    }
    return cells
  }

  /** The page rows that are marked as deleted, as row numbers on the page. */
  get deletedRows(): number[] {
    const rows: number[] = []
    if (!this.page || Object.keys(this.deletes).length === 0) {
      return rows
    }
    for (const [index, row] of this.page.rows.entries()) {
      if (this.rowId(row) in this.deletes) {
        rows.push(index)
      }
    }
    return rows
  }

  /**
   * The one place that asks before unsaved changes are lost.
   * Every method that loads or removes the page calls this first.
   * Returns true when the page may go. The unsaved changes are dropped then.
   */
  async mayDiscard(): Promise<boolean> {
    const count = this.pendingCount
    if (count === 0) {
      return true
    }
    const sure = await this.prompt.ask(
      `You have ${this.unsavedText} in "${this.table?.name}". Discard ${count === 1 ? 'it' : 'them'}?`,
    )
    if (sure) {
      this.discardChanges()
    }
    return sure
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
      return !this.endCancelled()
    } catch (error) {
      if (!this.endCancelled()) {
        this.error = String(error)
      }
      return false
    } finally {
      this.busy = false
      this.cancellable = false
      this.cancelling = false
    }
  }

  /**
   * Stops the running query or page load. The backend stops it on the server.
   * Does nothing when nothing that can be cancelled runs.
   */
  async cancel(): Promise<void> {
    if (!this.busy || !this.cancellable || this.cancelling) {
      return
    }
    this.cancelling = true
    try {
      await this.api.cancelQuery(this.id)
    } catch (error) {
      this.cancelling = false
      this.error = String(error)
    }
  }

  /**
   * True when the user cancelled the work that just ended. Then it shows the short status.
   * A cancelled query may still end without an error: MySQL ends a cancelled SLEEP like that.
   */
  private endCancelled(): boolean {
    if (this.cancelling) {
      this.status = QUERY_CANCELLED_STATUS
    }
    return this.cancelling
  }

  async connect(config: ConnectionConfig, title: string, environment = ''): Promise<void> {
    await this.api.connect(this.id, config)
    await this.setUp(config, title, environment)
  }

  /**
   * Connects this new tab as a copy of the connection of `source`.
   * The backend copies the password. The frontend does not have it.
   */
  async cloneFrom(source: Session): Promise<void> {
    if (!source.config) {
      return
    }
    await this.api.cloneConnection(source.id, this.id)
    try {
      await this.setUp(source.config, source.title, source.environment)
    } catch (error) {
      // Do not leave a connection open that no tab shows.
      await this.disconnect()
      throw error
    }
  }

  async disconnect(): Promise<void> {
    if (!(await this.mayDiscard())) {
      return
    }
    try {
      await this.api.disconnect(this.id)
    } finally {
      // Go back to the connection form even when the backend fails.
      this.config = null
      this.title = NEW_TITLE
      this.environment = ''
      this.databases = []
      this.tables = []
      this.objects = []
      this.result = null
      this.status = ''
      this.diagram = null
      this.clearTable()
      this.setUsers([], false)
    }
  }

  async useDatabase(name: string): Promise<void> {
    if (!(await this.mayDiscard())) {
      return
    }
    await this.api.useDatabase(this.id, name)
    if (this.config) {
      this.config.database = name
    }
    this.clearTable()
    await this.loadTables()
    // Access is per database, so the list is different now.
    await this.loadUsers()
    // The old diagram is of the other database.
    this.diagram = null
    if (this.view === 'diagram') {
      await this.loadDiagram()
    }
  }

  /** Loads the tables and views, then the triggers and routines. */
  async loadTables(): Promise<void> {
    this.tables = await this.api.listTables(this.id)
    await this.loadObjects()
  }

  /**
   * Loads the triggers and routines. It never throws.
   * The list is extra. An account that may not read it still sees its tables.
   */
  async loadObjects(): Promise<void> {
    try {
      this.objects = await this.api.listObjects(this.id)
    } catch {
      this.objects = []
    }
  }

  /** Shows the CREATE statement of a view, trigger or routine. It replaces the open table. */
  async showDefinition(object: DbObject): Promise<void> {
    if (!(await this.mayDiscard())) {
      return
    }
    this.clearTable()
    if (this.view !== 'content' && this.view !== 'structure') {
      this.view = 'content'
    }
    const sql = await this.api.objectDefinition(this.id, object)
    this.definition = { object, sql }
  }

  /** Puts the shown definition into the Query view, so it can be changed and run there. */
  editDefinition(): void {
    if (this.definition) {
      this.sql = this.definition.sql
      this.view = 'query'
    }
  }

  /** A view as an object, for `showDefinition`. */
  static viewObject(view: TableRef): DbObject {
    return { kind: 'view', schema: view.schema, name: view.name, table: null, args: null }
  }

  /** True when the definition of `object` is shown now. */
  shows(object: DbObject): boolean {
    const shown = this.definition?.object
    return (
      shown?.kind === object.kind &&
      shown.name === object.name &&
      shown.schema === object.schema &&
      shown.table === object.table &&
      shown.args === object.args
    )
  }

  /** Drops a trigger or routine. The list is loaded again, also after an error. */
  async dropObject(object: DbObject): Promise<void> {
    try {
      await this.api.dropObject(this.id, object)
    } finally {
      await this.loadObjects()
    }
    if (this.shows(object)) {
      this.definition = null
    }
    this.status = `${OBJECT_KIND_LABELS[object.kind]} "${object.name}" dropped`
  }

  async selectTable(table: TableRef): Promise<void> {
    if (!(await this.mayDiscard())) {
      return
    }
    this.clearTable()
    this.table = table
    if (this.view === 'query' || this.view === 'users' || this.view === 'diagram') {
      this.view = 'content'
    }
    await this.loadPage()
  }

  /** Loads the rows again. */
  async reload(): Promise<void> {
    if (await this.mayDiscard()) {
      await this.loadPage()
    }
  }

  /** Loads again what the current view shows. The Query view has nothing to load. */
  async refresh(): Promise<void> {
    if (this.view === 'users') {
      await this.loadUsers()
    } else if (this.view === 'diagram') {
      await this.loadDiagram()
    } else if (this.view !== 'query') {
      await this.reload()
    }
  }

  /**
   * Does not ask about unsaved changes. Callers from outside use `reload`.
   * `total` is the known row count. Pass it only for a page turn: then the backend
   * skips the count. Every other load counts again, so the total stays fresh.
   */
  private async loadPage(total?: number): Promise<void> {
    const table = this.table
    if (!table) {
      return
    }
    this.cancellable = true
    const page = await this.api.browse(this.id, {
      table,
      filter: this.filter,
      filters: this.filters,
      orderBy: this.orderBy,
      descending: this.descending,
      limit: PAGE_SIZE,
      offset: this.offset,
      total,
    })
    this.cancellable = false
    // The user may have picked another table while this one was loading.
    if (this.table === table) {
      this.page = page
      this.dropLostEdits()
      // A filter row made before the columns were known gets the first column now.
      for (const row of this.filterRows) {
        if (row.column === '') {
          row.column = page.columns[0]?.name ?? ''
        }
      }
      // A column change can change the indexes too. Other views do not need them.
      this.keys = null
      if (this.view === 'structure') {
        await this.loadKeys()
      }
    }
  }

  /** Loads the indexes and foreign keys of the open table. A view has none. */
  async loadKeys(): Promise<void> {
    const table = this.table
    if (!table || table.kind !== 'table') {
      this.keys = null
      return
    }
    const keys = await this.api.tableKeys(this.id, table)
    // The user may have picked another table while this one was loading.
    if (this.table === table) {
      this.keys = keys
    }
  }

  /** SQLite cannot add or drop a foreign key of an existing table. */
  get canChangeForeignKeys(): boolean {
    return this.config?.driver !== 'sqlite'
  }

  async addIndex(index: NewIndex): Promise<void> {
    const name = index.name.trim()
    if (!this.table || name === '') {
      return
    }
    await this.api.addIndex(this.id, this.table, { ...index, name })
    this.status = `Index "${name}" added`
    await this.loadKeys()
  }

  async dropIndex(name: string): Promise<void> {
    if (!this.table) {
      return
    }
    await this.api.dropIndex(this.id, this.table, name)
    this.status = `Index "${name}" dropped`
    await this.loadKeys()
  }

  async addForeignKey(key: NewForeignKey): Promise<void> {
    const name = key.name.trim()
    if (!this.table || name === '') {
      return
    }
    await this.api.addForeignKey(this.id, this.table, { ...key, name })
    this.status = `Foreign key "${name}" added`
    await this.loadKeys()
  }

  async dropForeignKey(name: string): Promise<void> {
    if (!this.table) {
      return
    }
    await this.api.dropForeignKey(this.id, this.table, name)
    this.status = `Foreign key "${name}" dropped`
    await this.loadKeys()
  }

  async sort(column: string): Promise<void> {
    if (!(await this.mayDiscard())) {
      return
    }
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
    if (!(await this.mayDiscard())) {
      return
    }
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
    if (!(await this.mayDiscard())) {
      return
    }
    this.filterRows.splice(index, 1)
    if (this.filterRows.length === 0) {
      this.filterRows.push(this.emptyFilterRow())
    }
    await this.applyFilters()
  }

  async turnPage(direction: 1 | -1): Promise<void> {
    const next = this.offset + direction * PAGE_SIZE
    if (!this.page || next < 0 || next >= this.page.total || !(await this.mayDiscard())) {
      return
    }
    this.offset = next
    await this.loadPage(this.page.total)
  }

  /**
   * Keeps a cell edit as a pending change. Nothing is sent before `saveChanges`.
   * Setting a cell back to its loaded value removes the pending change.
   */
  updateCell(rowIndex: number, columnIndex: number, value: string | null): void {
    if (!this.page) {
      return
    }
    const row = this.page.rows[rowIndex]
    if (!row) {
      const newRow = this.newRows[rowIndex - this.page.rows.length]
      if (newRow) {
        newRow[columnIndex] = value
      }
      return
    }
    const id = this.rowId(row)
    // A row that is marked as deleted cannot be edited.
    if (id in this.deletes) {
      return
    }
    const name = this.page.columns[columnIndex].name
    const update = this.edits[id] ?? { key: this.keyOf(row), changes: {} }
    if (value === row[columnIndex]) {
      delete update.changes[name]
    } else {
      update.changes[name] = value
    }
    if (Object.keys(update.changes).length > 0) {
      this.edits[id] = update
    } else {
      delete this.edits[id]
    }
  }

  /** Adds an empty new row below the last row. */
  addRow(): void {
    if (this.page) {
      this.newRows.push(new Array<string | null>(this.page.columns.length).fill(null))
    }
  }

  /** Drops all pending edits, all new rows and all delete marks. */
  discardChanges(): void {
    this.edits = {}
    this.newRows = []
    this.deletes = {}
  }

  /**
   * Sends all pending edits, new rows and deletes in one transaction.
   * Asks first when rows will be deleted. On "no" nothing is sent.
   * On an error nothing is saved and the pending changes stay.
   * A new row sends only the columns the user set. The database fills in the rest.
   */
  async saveChanges(): Promise<void> {
    if (!this.page || !this.table || this.pendingCount === 0) {
      return
    }
    const deletes = Object.values(this.deletes)
    if (deletes.length > 0) {
      const rows = this.plural(deletes.length, 'row')
      const question = `Saving will delete ${rows} from "${this.table.name}". This cannot be undone. Continue?`
      if (!(await this.prompt.ask(question, DELETE_ROWS_TITLE))) {
        return
      }
    }
    const inserts: Values[] = []
    for (const row of this.newRows) {
      const values: Values = {}
      for (const [index, column] of this.page.columns.entries()) {
        if (row[index] !== null) {
          values[column.name] = row[index]
        }
      }
      inserts.push(values)
    }
    const updates = Object.values(this.edits)
    const result = await this.api.saveChanges(this.id, this.table, updates, inserts, deletes)
    this.discardChanges()
    const parts: string[] = []
    if (updates.length > 0) {
      parts.push(`${this.plural(result.updated, 'row')} updated`)
    }
    if (inserts.length > 0) {
      parts.push(`${this.plural(result.inserted, 'row')} added`)
    }
    if (deletes.length > 0) {
      parts.push(`${this.plural(result.deleted, 'row')} deleted`)
    }
    this.status = parts.join(', ')
    await this.loadPage()
  }

  /** True when `deleteRows` would restore these rows: all of them are marked as deleted. */
  restores(indexes: number[]): boolean {
    const marked = this.deletedRows
    for (const index of indexes) {
      if (!marked.includes(index)) {
        return false
      }
    }
    return indexes.length > 0
  }

  /**
   * Marks loaded rows as deleted. Nothing is sent before `saveChanges`.
   * A marked row loses its pending edits. A new row is only removed from the list.
   * When all the rows are marked already, the marks are removed again.
   */
  deleteRows(indexes: number[]): void {
    if (!this.page) {
      return
    }
    const restore = this.restores(indexes)
    const loaded = this.page.rows.length
    const kept: Row[] = []
    for (const [index, row] of this.newRows.entries()) {
      if (!indexes.includes(loaded + index)) {
        kept.push(row)
      }
    }
    this.newRows = kept
    for (const index of indexes) {
      const row = this.page.rows[index]
      if (!row) {
        continue
      }
      const id = this.rowId(row)
      delete this.edits[id]
      if (restore) {
        delete this.deletes[id]
      } else {
        this.deletes[id] = this.keyOf(row)
      }
    }
  }

  async addColumn(spec: ColumnSpec): Promise<void> {
    if (!this.table || !(await this.mayDiscard())) {
      return
    }
    await this.api.addColumn(this.id, this.table, spec)
    this.status = `Column "${spec.name}" added`
    await this.loadPage()
  }

  async dropColumn(name: string): Promise<void> {
    if (!this.table || !(await this.mayDiscard())) {
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
    if (!this.table || !(await this.mayDiscard())) {
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

  /**
   * Creates a table, reloads the list and opens the new table on the Structure view.
   * Bad input is refused here. Nothing is sent then.
   */
  async createTable(name: string, columns: ColumnDraft[]): Promise<void> {
    const problem = TableDraft.problem(name, columns)
    if (problem) {
      throw problem
    }
    const newName = name.trim()
    const before = this.tables
    await this.api.createTable(this.id, newName, TableDraft.columns(columns))
    this.status = `Table "${newName}" created`
    await this.loadTables()
    // PostgreSQL can have the same name in two schemas. Take the one that is new.
    for (const table of this.tables) {
      if (table.name === newName && !this.lists(before, table)) {
        // The user may keep unsaved changes. Then the new table is not opened.
        await this.selectTable(table)
        if (this.isOpen(table)) {
          this.view = 'structure'
        }
        return
      }
    }
  }

  /**
   * Gives a table a new name and reloads the list.
   * The open table stays open under its new name, with its filters and sort order.
   */
  async renameTable(table: TableRef, name: string): Promise<void> {
    const newName = name.trim()
    if (newName === '') {
      throw 'The table needs a name'
    }
    const open = this.isOpen(table)
    if (newName === table.name || (open && !(await this.mayDiscard()))) {
      return
    }
    await this.api.renameTable(this.id, table, newName)
    if (open) {
      this.table = { ...table, name: newName }
    }
    this.status = `Table "${table.name}" renamed to "${newName}"`
    await this.loadTables()
  }

  /** Removes a table or a view. The backend picks DROP TABLE or DROP VIEW. */
  async dropTable(table: TableRef): Promise<void> {
    await this.dropTables([table])
  }

  /**
   * Removes tables and views one by one. It stops at the first error and says which one failed.
   * The list is loaded again once at the end, also after an error.
   */
  async dropTables(tables: TableRef[]): Promise<void> {
    if (this.opensOneOf(tables) && !(await this.mayDiscard())) {
      return
    }
    try {
      await this.runEach(tables, 'drop')
    } finally {
      await this.loadTables()
    }
    this.status = this.doneText(tables, 'dropped')
  }

  /** Deletes all rows of a table. */
  async truncateTable(table: TableRef): Promise<void> {
    await this.truncateTables([table])
  }

  /** Deletes all rows of each table, one by one. It stops at the first error and says which one failed. */
  async truncateTables(tables: TableRef[]): Promise<void> {
    const open = this.opensOneOf(tables)
    if (open && !(await this.mayDiscard())) {
      return
    }
    try {
      await this.runEach(tables, 'truncate')
    } finally {
      if (open) {
        this.offset = 0
        await this.loadPage()
      }
    }
    this.status = this.doneText(tables, 'truncated')
  }

  /** Drops or truncates each table in turn. With many tables, an error says how far it got. */
  private async runEach(tables: TableRef[], verb: 'drop' | 'truncate'): Promise<void> {
    let done = 0
    for (const table of tables) {
      try {
        if (verb === 'drop') {
          await this.api.dropTable(this.id, table)
          // The definition of a dropped view goes too.
          if (this.isOpen(table) || this.shows(Session.viewObject(table))) {
            this.clearTable()
          }
        } else {
          await this.api.truncateTable(this.id, table)
        }
      } catch (error) {
        if (tables.length === 1) {
          throw error
        }
        throw `Could not ${verb} "${table.name}" (${done} of ${tables.length} done): ${String(error)}`
      }
      done += 1
    }
  }

  private opensOneOf(tables: TableRef[]): boolean {
    for (const table of tables) {
      if (this.isOpen(table)) {
        return true
      }
    }
    return false
  }

  /** Like 'Table "users" dropped' or '3 tables dropped'. */
  private doneText(tables: TableRef[], verb: string): string {
    if (tables.length === 1) {
      const kind = tables[0].kind === 'view' ? 'View' : 'Table'
      return `${kind} "${tables[0].name}" ${verb}`
    }
    return `${tables.length} ${TableSelection.noun(tables)} ${verb}`
  }

  /**
   * Loads the user list. It never throws.
   * A failure means only that users cannot be managed here.
   */
  async loadUsers(): Promise<void> {
    try {
      const list = await this.api.listUsers(this.id)
      this.setUsers(list.users, list.canManage)
    } catch {
      this.setUsers([], false)
    }
  }

  async createUser(user: NewUser): Promise<void> {
    await this.api.createUser(this.id, user)
    this.status = `User "${user.name}" added`
    await this.loadUsers()
  }

  async dropUser(user: DbUser): Promise<void> {
    await this.api.dropUser(this.id, user)
    this.status = `User "${user.name}" removed`
    await this.loadUsers()
  }

  async setUserAccess(user: DbUser, access: GrantableAccess): Promise<void> {
    await this.api.setUserAccess(this.id, user, access)
    this.status = `Access of "${user.name}" set to ${ACCESS_LABELS[access]}`
    await this.loadUsers()
  }

  /** The password is never put in the status. */
  async setUserPassword(user: DbUser, password: string): Promise<void> {
    await this.api.setUserPassword(this.id, user, password)
    this.status = `Password of "${user.name}" changed`
  }

  /** Loads the tables and foreign keys for the Diagram view. */
  async loadDiagram(): Promise<void> {
    try {
      this.diagram = await this.api.diagram(this.id)
    } catch (error) {
      // Do not keep showing a diagram that may be of another database.
      this.diagram = null
      throw error
    }
  }

  /** The file name we offer for a picture of the diagram, like `shop-diagram.png`. */
  diagramName(): string {
    return `${this.databaseName() || 'database'}-diagram.png`
  }

  /** Writes the PNG picture of the diagram to the file at `path`. The status goes away after a few seconds. */
  async saveImage(path: string, data: number[]): Promise<void> {
    await this.api.saveImage(path, data)
    this.status = `Diagram saved to ${path}`
    // A second export starts the time again.
    clearTimeout(this.statusTimer)
    this.statusTimer = setTimeout(this.clearStatus.bind(this), DIAGRAM_SAVED_STATUS_MS, this.status)
  }

  /** Removes the status when it still shows `text`. A newer status stays. */
  private clearStatus(text: string): void {
    if (this.status === text) {
      this.status = ''
    }
  }

  /** The file name we offer for a backup made at `now`, like `shop-2026-01-31-0905.sql`. */
  backupName(now: Date): string {
    const name = this.databaseName()
    const day = `${now.getFullYear()}-${this.two(now.getMonth() + 1)}-${this.two(now.getDate())}`
    const time = `${this.two(now.getHours())}${this.two(now.getMinutes())}`
    return `${name || 'backup'}-${day}-${time}.sql`
  }

  /** Writes a SQL dump of the current database to the file at `path`. */
  async backup(path: string): Promise<void> {
    await this.api.dumpDatabase(this.id, path)
    this.status = `Backup saved to ${path}`
  }

  /** The columns of any table or view of the current database. */
  listColumns(table: TableRef): Promise<ColumnInfo[]> {
    return this.api.listColumns(this.id, table)
  }

  /** The file name we offer for an export, like `users.csv`, or `shop.sql` for many tables. */
  exportName(tables: TableRef[], format: ExportFormat): string {
    const name = tables.length === 1 ? tables[0].name : this.databaseName() || 'export'
    return `${name}.${format}`
  }

  /** Writes tables to a SQL or CSV file, then says where they went. */
  async exportTables(request: ExportRequest): Promise<void> {
    await this.api.exportTables(this.id, request)
    const what =
      request.tables.length === 1
        ? `"${request.tables[0].name}"`
        : this.plural(request.tables.length, 'table')
    this.status = `Exported ${what} to ${request.path}`
  }

  /**
   * The rows to export: the rows at `indexes`, in grid order, or all rows when
   * `indexes` is empty. Indexes past the end are skipped. In the content view
   * those are draft new rows, which are not saved yet.
   * Pass the stored rows (`page.rows`), so unsaved cell edits are not exported.
   */
  pickRows(rows: Row[], indexes: number[]): Row[] {
    if (indexes.length === 0) {
      return rows
    }
    const wanted = new Set(indexes)
    const picked: Row[] = []
    for (const [index, row] of rows.entries()) {
      if (wanted.has(index)) {
        picked.push(row)
      }
    }
    return picked
  }

  /** The file name we offer for exported rows, like `users.csv`. A query result has no table. */
  rowsName(table: TableRef | null, format: ExportFormat): string {
    return `${table?.name ?? QUERY_RESULT_NAME}.${format}`
  }

  /**
   * Writes rows the app already has to a file, then says where they went.
   * A query result has no table (`table` is null). Its INSERT statements use the file name.
   */
  async exportRows(
    format: ExportFormat,
    table: TableRef | null,
    columns: string[],
    rows: Row[],
    path: string,
  ): Promise<void> {
    const fileName = path
      .split(/[\\/]/)
      .pop()
      ?.replace(/\.[^.]*$/, '')
    const target = table ?? { schema: null, name: fileName || QUERY_RESULT_NAME, kind: 'table' }
    const request: RowsExport = { format, table: target, columns, rows, path }
    await this.api.exportRows(this.id, request)
    this.status = `Exported ${this.plural(rows.length, 'row')} to ${path}`
  }

  /** The first rows of a CSV file and how many rows it has. */
  previewCsv(path: string): Promise<CsvPreview> {
    return this.api.previewCsv(path)
  }

  /**
   * Inserts the rows of a CSV file into a table. When that table is open,
   * its unsaved changes must go first, and its rows are loaded again after.
   */
  async importCsv(request: CsvImport): Promise<void> {
    const open = this.isOpen(request.table)
    if (open && !(await this.mayDiscard())) {
      return
    }
    this.status = `Importing into "${request.table.name}"…`
    const count = await this.api.importCsv(this.id, request)
    this.status = `Imported ${this.plural(count, 'row')} into "${request.table.name}"`
    if (open) {
      await this.loadPage()
    }
  }

  /** Runs all statements of a SQL file. It may have made or dropped tables, so they load again. */
  async importSql(path: string): Promise<void> {
    this.status = `Importing ${path}…`
    const count = await this.api.importSql(this.id, path)
    this.status = `Ran ${this.plural(count, 'statement')} from ${path}`
    await this.loadTables()
  }

  /** The table list is replaced on refresh, so compare by name and not by object. */
  isOpen(table: TableRef): boolean {
    return this.table?.name === table.name && this.table.schema === table.schema
  }

  /** True when `table` is in `tables`. */
  private lists(tables: TableRef[], table: TableRef): boolean {
    for (const known of tables) {
      if (known.name === table.name && known.schema === table.schema) {
        return true
      }
    }
    return false
  }

  /** Shows the last result that has columns. Adds up the affected rows of the rest. */
  async runQuery(sql: string): Promise<void> {
    const started = performance.now()
    this.cancellable = true
    const sets = await this.api.runQuery(this.id, sql)
    this.cancellable = false
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

  /** The database name. For SQLite the file name without its folder and its ending. */
  private databaseName(): string {
    const name = this.config?.database ?? ''
    if (this.config?.driver !== 'sqlite') {
      return name
    }
    return (name.split(/[\\/]/).pop() ?? '').replace(/\.[^.]*$/, '')
  }

  /** The part of `connect` that runs after the backend is connected. */
  private async setUp(config: ConnectionConfig, title: string, environment: string): Promise<void> {
    this.config = { ...config, password: '', ssh: null }
    this.title = title
    this.environment = Environment.clean(environment)
    this.databases = await this.api.listDatabases(this.id)
    await this.loadTables()
    await this.loadUsers()
  }

  /** Names a loaded row by its primary key values. */
  private rowId(row: Row): string {
    return JSON.stringify(this.keyOf(row))
  }

  /** Rows that are not on the page any more lose their pending edits and delete marks. */
  private dropLostEdits(): void {
    if (!this.page || Object.keys(this.edits).length + Object.keys(this.deletes).length === 0) {
      return
    }
    const onPage = new Set<string>()
    for (const row of this.page.rows) {
      onPage.add(this.rowId(row))
    }
    for (const pending of [this.edits, this.deletes]) {
      for (const id of Object.keys(pending)) {
        if (!onPage.has(id)) {
          delete pending[id]
        }
      }
    }
  }

  /** The primary key columns of a loaded row, with the values as they were loaded. */
  private keyOf(row: Row): Values {
    const key: Values = {}
    for (const [index, column] of (this.page?.columns ?? []).entries()) {
      if (column.primaryKey) {
        key[column.name] = row[index]
      }
    }
    return key
  }

  /** Leaves the Users view when it is not allowed any more. */
  private setUsers(users: DbUser[], canManage: boolean): void {
    this.users = users
    this.canManageUsers = canManage
    if (!canManage && this.view === 'users') {
      this.view = 'content'
    }
  }

  private clearTable(): void {
    this.table = null
    this.definition = null
    this.page = null
    this.keys = null
    this.discardChanges()
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
