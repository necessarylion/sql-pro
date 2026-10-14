/** One shown row of the connection list. */
export interface DropRow {
  /** The folder of the row. '' = the top level. */
  folderId: string
  /** The connection of the row. '' = the row of the folder itself. */
  favoriteId: string
}

/** What a drop at the mouse would do. */
export interface Drop {
  /** The folder a connection goes into. '' = the top level. Not used for a dragged folder. */
  folderId: string
  /** The connection or folder it lands next to. '' = at the end. */
  anchorId: string
  /** True = after the anchor. False = before it. */
  after: boolean
  /** The folder to light up. '' = the "no folder" box. null = none. */
  into: string | null
  /** The index of the row that shows the line. -1 = no line. */
  line: number
  /** True = the line is at the bottom of that row. False = at its top. */
  lineAfter: boolean
}

/**
 * Says what a drop means while a connection or a folder is dragged over the list.
 * It only knows the shown rows. It does not read the page and it saves nothing.
 */
export class DropPlan {
  /**
   * `index` is the row under the mouse. -1 = the empty part of the list.
   * `upper` is true when the mouse is in the upper half of that row.
   */
  static decide(rows: DropRow[], folderDrag: boolean, index: number, upper: boolean): Drop {
    const row = rows[index] as DropRow | undefined
    if (folderDrag) {
      return DropPlan.forFolder(rows, row, index, upper)
    }
    if (!row || !row.favoriteId) {
      // A folder row, or no row: the connection goes to the end of that folder.
      const folderId = row?.folderId ?? ''
      return { folderId, anchorId: '', after: false, into: folderId, line: -1, lineAfter: false }
    }
    return {
      folderId: row.folderId,
      anchorId: row.favoriteId,
      after: !upper,
      into: null,
      line: index,
      lineAfter: !upper,
    }
  }

  /** The id that comes after `id` in the list. '' when it is the last one or not there. */
  static next(list: { id: string }[], id: string): string {
    for (const [index, item] of list.entries()) {
      if (item.id === id) {
        return list[index + 1]?.id ?? ''
      }
    }
    return ''
  }

  /**
   * A folder never goes into a folder. A row of a connection counts as its folder.
   * A top level row or no row counts as the last folder.
   */
  private static forFolder(
    rows: DropRow[],
    row: DropRow | undefined,
    index: number,
    upper: boolean,
  ): Drop {
    const none = { folderId: '', into: null }
    if (row && row.folderId && !row.favoriteId && upper) {
      return { ...none, anchorId: row.folderId, after: false, line: index, lineAfter: false }
    }
    // The line goes under the last shown row of the folder.
    const wanted = row?.folderId ?? ''
    let last = -1
    for (const [at, item] of rows.entries()) {
      if (item.folderId && (!wanted || item.folderId === wanted)) {
        last = at
      }
    }
    const anchorId = last < 0 ? '' : rows[last].folderId
    return { ...none, anchorId, after: true, line: last, lineAfter: true }
  }
}
