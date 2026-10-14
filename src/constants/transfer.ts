/** The format version of an exported connections file. Raise it when the format changes. */
export const CONNECTIONS_FILE_VERSION = 1

/** The file name the save dialog suggests for an export. */
export const CONNECTIONS_FILE_NAME = 'sql-pro-connections.json'

export const CONNECTIONS_FILE_FILTERS = [{ name: 'SQL Pro connections', extensions: ['json'] }]

export const CONNECTIONS_FILE_ERROR = 'This is not a SQL Pro connections file.'

export const CONNECTIONS_FILE_NEWER =
  'This file is from a newer SQL Pro. Update the app, then import it again.'
