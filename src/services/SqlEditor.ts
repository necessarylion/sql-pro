import { autocompletion, completionKeymap } from '@codemirror/autocomplete'
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
import type { Driver, TableRef } from '../types'
import type { SqlFormat } from './SqlFormat'

// All colours are CSS variables, so the editor follows the theme of the app.
const HIGHLIGHT = HighlightStyle.define([
  { tag: tags.keyword, color: 'var(--accent-text)' },
  // Quoted identifiers are a special kind of string. This rule covers them too.
  { tag: tags.string, color: 'var(--warn)' },
  { tag: [tags.number, tags.bool, tags.null, tags.typeName], color: 'var(--syntax-type)' },
  { tag: tags.comment, color: 'var(--text-3)', fontStyle: 'italic' },
])

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
}

/** The SQL editor: CodeMirror with our look, our keys and the table names. */
export class SqlEditor {
  private view: EditorView
  private options: SqlEditorOptions
  // The dialect and the table names change while the editor lives.
  private language = new Compartment()

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
    const schema: Record<string, string[]> = {}
    for (const table of tables) {
      schema[table.schema ? `${table.schema}.${table.name}` : table.name] = []
    }
    const dialect = driver ? SQL_DIALECTS[driver] : undefined
    const language: Extension = sql({ dialect, schema, defaultSchema: 'public' })
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
