import type { DbObjectKind, Driver } from '../types'

/**
 * The starter SQL of a new view, trigger, procedure or function, per database.
 * `my_table` is a placeholder. SQLite has no procedures or functions.
 */
export const OBJECT_TEMPLATES: Record<Driver, Partial<Record<DbObjectKind, string>>> = {
  mysql: {
    view: 'CREATE VIEW new_view AS\nSELECT *\nFROM my_table',
    trigger:
      'CREATE TRIGGER new_trigger\nBEFORE INSERT ON my_table\nFOR EACH ROW\nBEGIN\n  -- SET NEW.column = value;\nEND',
    procedure: 'CREATE PROCEDURE new_procedure()\nBEGIN\n  SELECT 1;\nEND',
    function:
      'CREATE FUNCTION new_function(a INT)\nRETURNS INT\nDETERMINISTIC\nBEGIN\n  RETURN a + 1;\nEND',
  },
  postgres: {
    view: 'CREATE VIEW new_view AS\nSELECT *\nFROM my_table',
    trigger:
      'CREATE TRIGGER new_trigger\nBEFORE INSERT ON my_table\nFOR EACH ROW\nEXECUTE FUNCTION my_trigger_function()',
    procedure:
      'CREATE PROCEDURE new_procedure()\nLANGUAGE plpgsql\nAS $$\nBEGIN\n  -- Your code\nEND\n$$',
    function:
      'CREATE FUNCTION new_function(a integer)\nRETURNS integer\nLANGUAGE sql\nAS $$\n  SELECT a + 1\n$$',
  },
  mssql: {
    view: 'CREATE VIEW new_view AS\nSELECT *\nFROM my_table',
    trigger:
      'CREATE TRIGGER new_trigger\nON my_table\nAFTER INSERT\nAS\nBEGIN\n  SET NOCOUNT ON;\nEND',
    procedure: 'CREATE PROCEDURE new_procedure\nAS\nBEGIN\n  SELECT 1;\nEND',
    function: 'CREATE FUNCTION new_function(@a INT)\nRETURNS INT\nAS\nBEGIN\n  RETURN @a + 1;\nEND',
  },
  sqlite: {
    view: 'CREATE VIEW new_view AS\nSELECT *\nFROM my_table',
    trigger: 'CREATE TRIGGER new_trigger\nAFTER INSERT ON my_table\nBEGIN\n  SELECT 1;\nEND',
  },
}
