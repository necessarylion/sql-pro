import { DEFAULT_SCHEMAS } from '../constants'

/** How the UI writes a name that may have a schema. Display only, never for SQL. */
export class SchemaName {
  /** Shows the schema only when it is not the default one (`public`, `dbo`). */
  static label(schema: string | null, name: string): string {
    return schema && !DEFAULT_SCHEMAS.includes(schema) ? `${schema}.${name}` : name
  }
}
