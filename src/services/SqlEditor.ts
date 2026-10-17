import {
  autocompletion,
  completionKeymap,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from '@codemirror/autocomplete'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { sql } from '@codemirror/lang-sql'
import { HighlightStyle, bracketMatching, syntaxHighlighting } from '@codemirror/language'
import { Compartment, EditorState, Prec, type Extension } from '@codemirror/state'
import {
  EditorView,
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
  placeholder,
  type ViewUpdate,
} from '@codemirror/view'
import { tags } from '@lezer/highlight'
import { SQL_DIALECTS } from '../constants/sql'
import type { ColumnInfo, Driver, TableRef } from '../types'
import type { SqlFormat } from './SqlFormat'
import { SQL_NAME_PART, SqlTableNames } from './SqlTableNames'

// All colours are CSS variables, so the editor follows the theme of the app.
const HIGHLIGHT = HighlightStyle.define([
  { tag: tags.keyword, color: 'var(--accent-text)' },
  // Quoted identifiers are a special kind of string. This rule covers them too.
  { tag: tags.string, color: 'var(--warn)' },
  { tag: [tags.number, tags.bool, tags.null, tags.typeName], color: 'var(--syntax-type)' },
  { tag: tags.comment, color: 'var(--text-3)', fontStyle: 'italic' },
])

// A table or alias, a dot and the start of a column name: `a.em`, `"admins".em`.
const QUALIFIED = new RegExp(`(${SQL_NAME_PART})\\.(\\w*)$`)

const SELECTION = 'color-mix(in srgb, var(--accent) 35%, transparent)'

const LOOK = EditorView.theme({
  '&': { height: '100%', background: 'var(--bg)', color: 'var(--text)' },
  '&.cm-focused': { outline: 'none' },
  '.cm-scroller': { font: '12px/19px var(--font-mono)' },
  // The body turns text selection off. The editor needs it, or typing fails in WebKit.
  '.cm-content': {
    padding: '8px 0',
    outline: 'none',
    caretColor: 'var(--text)',
    userSelect: 'text',
    WebkitUserSelect: 'text',
  },
  '.cm-cursor': { borderLeftColor: 'var(--text)' },
  '.cm-gutters': { background: 'var(--bg)', border: '0', color: 'var(--text-3)' },
  '.cm-lineNumbers .cm-gutterElement': { minWidth: '32px', padding: '0 8px 0 10px' },
  '.cm-activeLine': { background: 'var(--stmt-highlight)' },
  // CodeMirror draws the selection behind the lines. The solid line colour would hide it
  // on the line with the cursor, so the line colour is off while text is selected.
  '&.cm-has-selection .cm-activeLine': { background: 'transparent' },
  '.cm-activeLineGutter': { background: 'var(--stmt-highlight)', color: 'var(--accent-text)' },
  '.cm-selectionBackground, &.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground':
    { background: SELECTION },
  '.cm-matchingBracket, &.cm-focused .cm-matchingBracket': {
    background: 'none',
    outline: '1px solid var(--text-3)',
  },
  '.cm-placeholder': { color: 'var(--text-3)' },
  '.cm-tooltip': {
    background: 'var(--toolbar)',
    border: '1px solid var(--border)',
    borderRadius: '5px',
    color: 'var(--text)',
  },
  '.cm-tooltip-autocomplete ul li[aria-selected]': {
    background: 'var(--accent)',
    color: 'var(--on-accent)',
  },
})

/** Adds `cm-has-selection` to the editor while some text is selected. */
function selectionClass(state: EditorState): Record<string, string> {
  for (const range of state.selection.ranges) {
    if (!range.empty) {
      return { class: 'cm-has-selection' }
    }
  }
  return {}
}

export interface SqlEditorOptions {
  parent: HTMLElement
  text: string
  /** Called with the full text after each edit. */
  onChange?(text: string): void
  /** Called on Cmd+Enter or Ctrl+Enter. */
  onRun?(): void
  /** Called on Shift+Cmd+F or Ctrl+Shift+F. */
  onFormat?(): void
  /** True to only show the text. It can still be selected and copied. */
  readOnly?: boolean
  /** Loads the columns of one table. Without it, column names are not completed. */
  columns?(table: TableRef): Promise<ColumnInfo[]>
}

/** The SQL editor: CodeMirror with our look, our keys and the table names. */
export class SqlEditor {
  private view: EditorView
  private options: SqlEditorOptions
  // The dialect and the table names change while the editor lives.
  private language = new Compartment()
  private tables: TableRef[] = []
  // Column names per table, loaded the first time a query names the table.
  private columnCache = new Map<string, Promise<string[]>>()

  constructor(options: SqlEditorOptions) {
    this.options = options
    this.view = new EditorView({
      parent: options.parent,
      state: EditorState.create({
        doc: options.text,
        extensions: [
          // Before the default keys, which use Mod-Enter for a new line.
          Prec.highest(
            keymap.of([
              { key: 'Mod-Enter', run: this.handleRun.bind(this) },
              { key: 'Mod-Shift-f', run: this.handleFormat.bind(this) },
            ]),
          ),
          lineNumbers(),
          highlightActiveLine(),
          highlightActiveLineGutter(),
          drawSelection(),
          EditorView.editorAttributes.compute(['selection'], selectionClass),
          history(),
          bracketMatching(),
          autocompletion(),
          placeholder('SELECT * FROM …'),
          keymap.of([...completionKeymap, ...defaultKeymap, ...historyKeymap, indentWithTab]),
          this.language.of(sql()),
          syntaxHighlighting(HIGHLIGHT),
          LOOK,
          EditorView.updateListener.of(this.handleUpdate.bind(this)),
          EditorState.readOnly.of(options.readOnly ?? false),
        ],
      }),
    })
  }

  /** Sets the dialect of the driver and the table names for completion. */
  setSchema(driver: Driver | undefined, tables: TableRef[]): void {
    this.tables = tables
    // A new table list can mean another database. Its columns are not the same.
    this.columnCache.clear()
    const schema: Record<string, string[]> = {}
    for (const table of tables) {
      schema[this.key(table)] = []
    }
    const dialect = driver ? SQL_DIALECTS[driver] : undefined
    const support = sql({ dialect, schema, defaultSchema: 'public' })
    // lang-sql completes columns only after `table.`. This source adds the columns of the
    // tables in the query without a table name in front.
    const columns = support.language.data.of({ autocomplete: this.completeColumns.bind(this) })
    const language: Extension = [support, columns]
    this.view.dispatch({ effects: this.language.reconfigure(language) })
  }

  /** Replaces the text when it was changed from outside the editor. */
  setText(text: string): void {
    if (text !== this.text()) {
      this.view.dispatch({ changes: { from: 0, to: this.view.state.doc.length, insert: text } })
    }
  }

  text(): string {
    return this.view.state.doc.toString()
  }

  /** The selected text. Empty when nothing is selected. */
  selectedText(): string {
    const range = this.view.state.selection.main
    return this.view.state.sliceDoc(range.from, range.to)
  }

  /**
   * Formats the selected text, or all text when nothing is selected.
   * It is one change, so one undo brings the old text back.
   * Throws when the SQL cannot be parsed. The text then stays as it is.
   */
  format(formatter: SqlFormat): void {
    const range = this.view.state.selection.main
    const from = range.empty ? 0 : range.from
    const to = range.empty ? this.view.state.doc.length : range.to
    const insert = formatter.format(this.view.state.sliceDoc(from, to))
    this.view.dispatch({
      changes: { from, to, insert },
      selection: { anchor: from, head: from + insert.length },
    })
  }

  destroy(): void {
    this.view.destroy()
  }

  /**
   * Offers column names. After `x.` they are the columns of table or alias x. Without a
   * qualifier they are the columns of all tables in the query, ranked above keywords and tables.
   */
  private async completeColumns(context: CompletionContext): Promise<CompletionResult | null> {
    if (!this.options.columns) {
      return null
    }
    // ponytail: scans the whole text, so tables of other statements count too.
    const text = context.state.doc.toString()
    const qualified = context.matchBefore(QUALIFIED)
    if (qualified) {
      const parts = QUALIFIED.exec(qualified.text)
      // A schema (`public.`) is no table here. lang-sql then completes its tables.
      const table = parts ? this.findTable(SqlTableNames.resolve(text, parts[1])) : undefined
      if (!parts || !table) {
        return null
      }
      // lang-sql adds nothing here: its schema has no columns and it skips keywords after a dot.
      const options = await this.columnOptions([table])
      return { from: qualified.to - parts[2].length, options, validFor: /^\w*$/ }
    }
    const word = context.matchBefore(/\w*/)
    if (!word || (word.from === word.to && !context.explicit)) {
      return null
    }
    const tables = []
    for (const name of SqlTableNames.find(text)) {
      const table = this.findTable(name)
      if (table) {
        tables.push(table)
      }
    }
    const options = await this.columnOptions(tables)
    return options.length > 0 ? { from: word.from, options, validFor: /^\w*$/ } : null
  }

  private async columnOptions(tables: TableRef[]): Promise<Completion[]> {
    const options: Completion[] = []
    for (const table of tables) {
      for (const column of await this.loadColumns(table)) {
        options.push({ label: column, type: 'property', detail: table.name, boost: 1 })
      }
    }
    return options
  }

  /** The known table for `name` or `schema.name`. Case does not matter. */
  private findTable(name: string): TableRef | undefined {
    const wanted = name.toLowerCase()
    for (const table of this.tables) {
      if (table.name.toLowerCase() === wanted || this.key(table).toLowerCase() === wanted) {
        return table
      }
    }
    return undefined
  }

  private loadColumns(table: TableRef): Promise<string[]> {
    const key = this.key(table)
    let names = this.columnCache.get(key)
    if (!names) {
      names = this.fetchColumns(table)
      this.columnCache.set(key, names)
    }
    return names
  }

  private async fetchColumns(table: TableRef): Promise<string[]> {
    try {
      const columns = (await this.options.columns?.(table)) ?? []
      const names = []
      for (const column of columns) {
        names.push(column.name)
      }
      return names
    } catch {
      // Try again on the next completion. A failed load must not stay in the cache.
      this.columnCache.delete(this.key(table))
      return []
    }
  }

  private key(table: TableRef): string {
    return table.schema ? `${table.schema}.${table.name}` : table.name
  }

  private handleRun(): boolean {
    this.options.onRun?.()
    return true
  }

  private handleFormat(): boolean {
    this.options.onFormat?.()
    return true
  }

  private handleUpdate(update: ViewUpdate): void {
    if (update.docChanged) {
      this.options.onChange?.(update.state.doc.toString())
    }
  }
}
