//! The query plan as a tree. The repository gives the plan SQL of its database. This reads
//! the output of each database into one `PlanNode` tree, and marks slow steps and common
//! problems the same way for all of them.
//!
//! ponytail: the share of a step is its own time or cost against the whole plan. Its own
//! part is its value minus the values of its children. The databases count loops, parallel
//! workers and subplans each in their own way, so a share is a good hint, not an exact number.

use serde_json::Value;

use super::{ConnectionService, claude};
use crate::constants::{
    ANALYZE_NOT_A_READ, EXPLAIN_ONE_STATEMENT, NO_EXPLAIN, PLAN_ROW_GAP, PLAN_SLOW_SHARE,
    PLAN_WARM_SHARE,
};
use crate::error::AppResult;
use crate::model::{
    PlanDetail, PlanFormat, PlanHeat, PlanMeasure, PlanNode, PlanProblem, QueryPlan, ResultSet,
};

impl ConnectionService {
    /// The plan of one statement as a tree, with the plain `EXPLAIN` output for the Raw tab.
    /// Without `analyze` the statement never runs. With `analyze` it runs once, for real
    /// times: only a read is taken, and it runs inside the read-only guard that is rolled back.
    pub async fn query_plan(&self, id: &str, sql: &str, analyze: bool) -> AppResult<QueryPlan> {
        let connection = self.connection(id)?;
        let repository = connection.repository.as_ref();
        let statement = claude::explainable(sql).ok_or(EXPLAIN_ONE_STATEMENT)?;
        if analyze && !claude::is_read_only(statement) {
            return Err(ANALYZE_NOT_A_READ.into());
        }
        let plan_sql = repository
            .plan_sql(statement, analyze)
            .await?
            .ok_or(NO_EXPLAIN)?;
        if analyze {
            Self::check_no_transaction(&connection).await?;
        }
        let sets = if plan_sql.before.is_none() && plan_sql.after.is_none() {
            if analyze {
                Self::guarded_batch(repository, &plan_sql.sql).await?
            } else {
                repository.query(&plan_sql.sql).await?
            }
        } else {
            // The settings stay on for the connection. So they run on the same connection
            // as the statement, with nothing between, and `after` turns them off again.
            // A failed Analyze ends its guard. A failed Explain keeps the user's transaction.
            let body = if analyze {
                Self::guarded_sql(repository, &plan_sql.sql)
            } else {
                plan_sql.sql.clone()
            };
            let mut steps: Vec<&str> = plan_sql.before.into_iter().collect();
            steps.push(&body);
            let end = repository.read_only_guard().1;
            let on_error: &[&str] = if analyze { &[end] } else { &[] };
            let always: Vec<&str> = plan_sql.after.into_iter().collect();
            repository.query_steps(&steps, on_error, &always).await?
        };
        let output = last_with_columns(sets);
        let raw = match repository.explain_prefix() {
            // `SQLite` explains with the same words. Its plan is the plain output already.
            Some(prefix) if format!("{prefix} {statement}") == plan_sql.sql => output.clone(),
            Some(prefix) => {
                last_with_columns(repository.query(&format!("{prefix} {statement}")).await?)
            }
            None => output.clone(),
        };
        let nodes = output
            .as_ref()
            .and_then(|set| parse(plan_sql.format, set))
            .unwrap_or_default();
        Ok(plan(nodes, analyze, raw))
    }
}

/// The last result set that has columns. The guard and the settings give empty ones.
fn last_with_columns(sets: Vec<ResultSet>) -> Option<ResultSet> {
    sets.into_iter().rfind(|set| !set.columns.is_empty())
}

/// Reads the plan output into trees. `None` when it is not in the expected form.
fn parse(format: PlanFormat, set: &ResultSet) -> Option<Vec<PlanNode>> {
    if format == PlanFormat::SqliteRows {
        return sqlite(&set.rows);
    }
    // The plan is one text cell. Some clients split it into one row per line.
    let lines: Vec<&str> = set
        .rows
        .iter()
        .filter_map(|row| row.first().and_then(Option::as_deref))
        .collect();
    let text = lines.join("\n");
    let nodes = match format {
        PlanFormat::PostgresJson => postgres(&text),
        PlanFormat::MysqlJson => mysql_json(&text),
        PlanFormat::MysqlTree => Some(mysql_tree(&text)),
        PlanFormat::MssqlXml => mssql(&text),
        PlanFormat::SqliteRows => None,
    }?;
    (!nodes.is_empty()).then_some(nodes)
}

/// Fills in the totals, the shares, the heat and the row estimate badges.
fn plan(mut nodes: Vec<PlanNode>, analyzed: bool, raw: Option<ResultSet>) -> QueryPlan {
    for node in &mut nodes {
        finish(node);
    }
    let has_time = analyzed && nodes.iter().any(|node| node.time_ms.is_some());
    let has_cost = nodes.iter().any(|node| node.cost.is_some());
    let measure = if has_time {
        Some(PlanMeasure::Time)
    } else if has_cost {
        Some(PlanMeasure::Cost)
    } else {
        None
    };
    let total = measure.map(|measure| {
        nodes
            .iter()
            .filter_map(|node| value(node, measure))
            .sum::<f64>()
    });
    if let (Some(measure), Some(total)) = (measure, total)
        && total > 0.0
    {
        for node in &mut nodes {
            mark(node, measure, total);
        }
    }
    QueryPlan {
        nodes,
        analyzed,
        measure,
        total,
        raw,
    }
}

/// The value of a step with its children, in `measure`.
fn value(node: &PlanNode, measure: PlanMeasure) -> Option<f64> {
    match measure {
        PlanMeasure::Time => node.time_ms,
        PlanMeasure::Cost => node.cost,
    }
}

/// Works out the values a database does not give: the own part from the value with
/// the children, or the other way round. Children first.
fn finish(node: &mut PlanNode) {
    for child in &mut node.children {
        finish(child);
    }
    let (cost, own_cost) = complete(
        node.cost,
        node.own_cost,
        node.children.iter().map(|child| child.cost),
    );
    node.cost = cost;
    node.own_cost = own_cost;
    let (time, own_time) = complete(
        node.time_ms,
        node.own_time,
        node.children.iter().map(|child| child.time_ms),
    );
    node.time_ms = time;
    node.own_time = own_time;
    if let (Some(estimated), Some(actual)) = (node.estimated_rows, node.actual_rows)
        && node.loops != Some(0.0)
    {
        // +1 so a step with 0 rows does not divide by zero, and 0 against 3 is no gap.
        let gap = (estimated.max(actual) + 1.0) / (estimated.min(actual) + 1.0);
        if gap >= PLAN_ROW_GAP {
            add(&mut node.problems, PlanProblem::RowEstimate);
        }
    }
}

/// `(with children, own)` from what is known. A negative own part becomes 0.
fn complete(
    total: Option<f64>,
    own: Option<f64>,
    children: impl Iterator<Item = Option<f64>>,
) -> (Option<f64>, Option<f64>) {
    let children: Vec<f64> = children.flatten().collect();
    let sum = (!children.is_empty()).then(|| children.iter().sum::<f64>());
    match (total, own) {
        (Some(total), None) => (Some(total), Some((total - sum.unwrap_or(0.0)).max(0.0))),
        (None, Some(own)) => (Some(own + sum.unwrap_or(0.0)), Some(own)),
        (None, None) => (sum, sum.map(|_| 0.0)),
        known => known,
    }
}

fn mark(node: &mut PlanNode, measure: PlanMeasure, total: f64) {
    let own = match measure {
        PlanMeasure::Time => node.own_time,
        PlanMeasure::Cost => node.own_cost,
    };
    if let Some(own) = own {
        let share = (own / total).clamp(0.0, 1.0);
        node.share = Some(share);
        node.heat = if share >= PLAN_SLOW_SHARE {
            Some(PlanHeat::Slow)
        } else if share >= PLAN_WARM_SHARE {
            Some(PlanHeat::Warm)
        } else {
            None
        };
    }
    for child in &mut node.children {
        mark(child, measure, total);
    }
}

fn add(problems: &mut Vec<PlanProblem>, problem: PlanProblem) {
    if !problems.contains(&problem) {
        problems.push(problem);
    }
}

fn detail(details: &mut Vec<PlanDetail>, name: &str, value: impl Into<String>) {
    let value = value.into();
    if !value.is_empty() {
        details.push(PlanDetail {
            name: name.to_owned(),
            value,
        });
    }
}

/// A number from JSON. `MySQL` writes numbers as text, like `"12.50"`.
fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

/// A JSON value as text: a list is joined with commas.
fn text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| text(Some(item)))
            .collect::<Vec<_>>()
            .join(", "),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(true)) => "yes".to_owned(),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------------------
// PostgreSQL: EXPLAIN (FORMAT JSON)

/// The fields of a `PostgreSQL` step that show as details, with their label.
const POSTGRES_DETAILS: [(&str, &str); 14] = [
    ("Join Type", "Join"),
    ("Strategy", "Strategy"),
    ("Hash Cond", "Hash condition"),
    ("Merge Cond", "Merge condition"),
    ("Index Cond", "Index condition"),
    ("Recheck Cond", "Recheck condition"),
    ("Join Filter", "Join filter"),
    ("Filter", "Filter"),
    ("Rows Removed by Filter", "Rows removed by filter"),
    ("Sort Key", "Sort key"),
    ("Sort Method", "Sort method"),
    ("Group Key", "Group key"),
    ("Subplan Name", "Subplan"),
    ("Workers Planned", "Workers"),
];

fn postgres(text: &str) -> Option<Vec<PlanNode>> {
    let value: Value = serde_json::from_str(text).ok()?;
    let plan = value.get(0)?.get("Plan")?;
    Some(vec![postgres_node(plan)])
}

fn postgres_node(plan: &Value) -> PlanNode {
    let field = |name: &str| plan.get(name);
    let operation = text(field("Node Type"));
    let mut node = PlanNode {
        object: postgres_object(plan),
        estimated_rows: number(field("Plan Rows")),
        actual_rows: number(field("Actual Rows")),
        loops: number(field("Actual Loops")),
        cost: number(field("Total Cost")),
        ..PlanNode::default()
    };
    // The time is for one loop. A step that never ran has no time.
    if let Some(loops) = node.loops {
        node.time_ms = Some(number(field("Actual Total Time")).unwrap_or(0.0) * loops);
    }
    for (name, label) in POSTGRES_DETAILS {
        detail(&mut node.details, label, text(field(name)));
    }
    match operation.as_str() {
        "Seq Scan" => add(&mut node.problems, PlanProblem::FullScan),
        "Sort" | "Incremental Sort" => add(&mut node.problems, PlanProblem::Filesort),
        _ => {}
    }
    let on_disk = text(field("Sort Space Type")) == "Disk"
        || number(field("Disk Usage")).is_some_and(|kb| kb > 0.0)
        || number(field("HashAgg Batches")).is_some_and(|batches| batches > 1.0)
        || number(field("Temp Written Blocks")).is_some_and(|blocks| blocks > 0.0);
    if on_disk {
        add(&mut node.problems, PlanProblem::TempTable);
    }
    if let Some(Value::Array(plans)) = field("Plans") {
        node.children = plans.iter().map(postgres_node).collect();
    }
    node.operation = operation;
    node
}

/// Like `orders o using orders_customer`.
fn postgres_object(plan: &Value) -> Option<String> {
    let relation = ["Relation Name", "CTE Name", "Function Name"]
        .iter()
        .map(|name| text(plan.get(*name)))
        .find(|name| !name.is_empty());
    let alias = text(plan.get("Alias"));
    let index = text(plan.get("Index Name"));
    let mut object = match relation {
        Some(relation) if !alias.is_empty() && alias != relation => format!("{relation} {alias}"),
        Some(relation) => relation,
        None => String::new(),
    };
    if !index.is_empty() {
        object = if object.is_empty() {
            index
        } else {
            format!("{object} using {index}")
        };
    }
    (!object.is_empty()).then_some(object)
}

// ---------------------------------------------------------------------------------------
// MySQL and MariaDB: EXPLAIN FORMAT=JSON, and ANALYZE FORMAT=JSON of MariaDB

/// The JSON keys of a step that is not a table, with the name it shows.
const MYSQL_OPERATIONS: [(&str, &str); 12] = [
    ("ordering_operation", "Order"),
    ("grouping_operation", "Group"),
    ("duplicates_removal", "Distinct"),
    ("windowing", "Window"),
    ("union_result", "Union"),
    ("buffer_result", "Buffer result"),
    ("materialized_from_subquery", "Materialize"),
    ("materialized", "Materialize"),
    ("filesort", "Sort"),
    ("temporary_table", "Temporary table"),
    ("read_sorted_file", "Read sorted file"),
    ("block-nl-join", "Block nested loop"),
];

fn mysql_json(text: &str) -> Option<Vec<PlanNode>> {
    let value: Value = serde_json::from_str(text).ok()?;
    Some(mysql_nodes(&value))
}

/// The steps in `value`. Keys we do not know are searched for steps too, like
/// `query_specifications` or `attached_subqueries`.
fn mysql_nodes(value: &Value) -> Vec<PlanNode> {
    match value {
        Value::Array(items) => items.iter().flat_map(mysql_nodes).collect(),
        Value::Object(map) => {
            let mut nodes = Vec::new();
            for (key, child) in map {
                if key == "query_block" {
                    nodes.push(mysql_query_block(child));
                } else if key == "table" {
                    nodes.push(mysql_table(child));
                } else if key == "nested_loop" {
                    let mut children = mysql_nodes(child);
                    if children.len() == 1 {
                        nodes.append(&mut children);
                    } else {
                        nodes.push(PlanNode {
                            operation: "Nested loop".to_owned(),
                            children,
                            ..PlanNode::default()
                        });
                    }
                } else if let Some((_, label)) =
                    MYSQL_OPERATIONS.iter().find(|(name, _)| name == key)
                {
                    nodes.push(mysql_operation(key, label, child));
                } else if child.is_object() || child.is_array() {
                    nodes.extend(mysql_nodes(child));
                }
            }
            nodes
        }
        _ => Vec::new(),
    }
}

fn mysql_query_block(block: &Value) -> PlanNode {
    let id = text(block.get("select_id"));
    let mut node = PlanNode {
        operation: if id.is_empty() {
            "Query block".to_owned()
        } else {
            format!("Query block #{id}")
        },
        // `MySQL` gives `cost_info.query_cost`, `MariaDB` gives `cost`. Both with the children.
        cost: number(block.pointer("/cost_info/query_cost")).or_else(|| number(block.get("cost"))),
        time_ms: number(block.get("r_total_time_ms")),
        loops: number(block.get("r_loops")),
        children: mysql_nodes(block),
        ..PlanNode::default()
    };
    detail(&mut node.details, "Message", text(block.get("message")));
    mysql_flags(block, &mut node.problems);
    node
}

fn mysql_table(table: &Value) -> PlanNode {
    let access = text(table.get("access_type"));
    let operation = match access.as_str() {
        "ALL" => "Full table scan",
        "index" => "Full index scan",
        "range" => "Index range scan",
        "ref" => "Index lookup",
        "eq_ref" => "Unique index lookup",
        "ref_or_null" => "Index lookup or NULL",
        "const" | "system" => "Constant row",
        "fulltext" => "Full-text search",
        "index_merge" => "Index merge",
        "unique_subquery" | "index_subquery" => "Index subquery",
        "" => "Table",
        other => other,
    };
    let name = text(table.get("table_name"));
    let key = text(table.get("key"));
    let object = match (name.is_empty(), key.is_empty()) {
        (false, false) => Some(format!("{name} using {key}")),
        (false, true) => Some(name),
        _ => None,
    };
    // `MySQL` gives the read and the evaluation cost, `MariaDB` one cost. Both without children.
    let own_cost = match (
        number(table.pointer("/cost_info/read_cost")),
        number(table.pointer("/cost_info/eval_cost")),
    ) {
        (None, None) => number(table.get("cost")),
        (read, eval) => Some(read.unwrap_or(0.0) + eval.unwrap_or(0.0)),
    };
    let own_time = match (
        number(table.get("r_table_time_ms")),
        number(table.get("r_other_time_ms")),
    ) {
        (None, None) => number(table.get("r_total_time_ms")),
        (table_time, other) => Some(table_time.unwrap_or(0.0) + other.unwrap_or(0.0)),
    };
    let mut node = PlanNode {
        operation: operation.to_owned(),
        object,
        estimated_rows: number(table.get("rows_examined_per_scan"))
            .or_else(|| number(table.get("rows"))),
        actual_rows: number(table.get("r_rows")),
        loops: number(table.get("r_loops")),
        own_cost,
        own_time,
        children: mysql_nodes(table),
        ..PlanNode::default()
    };
    let details = &mut node.details;
    detail(details, "Possible keys", text(table.get("possible_keys")));
    detail(details, "Key parts", text(table.get("used_key_parts")));
    detail(details, "Ref", text(table.get("ref")));
    detail(details, "Filter", text(table.get("attached_condition")));
    detail(details, "Filtered %", text(table.get("filtered")));
    detail(details, "Real filtered %", text(table.get("r_filtered")));
    detail(details, "Covering index", text(table.get("using_index")));
    if access == "ALL" {
        add(&mut node.problems, PlanProblem::FullScan);
    }
    mysql_flags(table, &mut node.problems);
    node
}

fn mysql_operation(key: &str, label: &str, value: &Value) -> PlanNode {
    let mut node = PlanNode {
        operation: label.to_owned(),
        object: Some(text(value.get("table_name"))).filter(|name| !name.is_empty()),
        own_cost: number(value.pointer("/cost_info/sort_cost")),
        children: mysql_nodes(value),
        ..PlanNode::default()
    };
    if key == "filesort" {
        // The time `MariaDB` gives here is the time of the sort itself.
        node.own_time = number(value.get("r_total_time_ms"));
        node.loops = number(value.get("r_loops"));
        node.actual_rows = number(value.get("r_output_rows"));
        add(&mut node.problems, PlanProblem::Filesort);
    }
    if matches!(
        key,
        "temporary_table" | "materialized_from_subquery" | "materialized" | "buffer_result"
    ) {
        add(&mut node.problems, PlanProblem::TempTable);
    }
    detail(&mut node.details, "Sort key", text(value.get("sort_key")));
    mysql_flags(value, &mut node.problems);
    node
}

/// The `using_filesort` and `using_temporary_table` flags of `MySQL`.
fn mysql_flags(value: &Value, problems: &mut Vec<PlanProblem>) {
    if value.get("using_filesort") == Some(&Value::Bool(true)) {
        add(problems, PlanProblem::Filesort);
    }
    if value.get("using_temporary_table") == Some(&Value::Bool(true)) {
        add(problems, PlanProblem::TempTable);
    }
}

// ---------------------------------------------------------------------------------------
// MySQL: the text tree of EXPLAIN ANALYZE

/// Each line is one step: `-> Table scan on c  (cost=0.35 rows=1) (actual time=0.0183..0.0183 rows=0 loops=1)`.
/// Four spaces of indent are one level.
fn mysql_tree(text: &str) -> Vec<PlanNode> {
    let mut roots = Vec::new();
    // The open steps, from the top down, with their depth.
    let mut open: Vec<(usize, PlanNode)> = Vec::new();
    for line in text.lines() {
        let body = line.trim_start();
        let Some(step) = body.strip_prefix("-> ") else {
            continue;
        };
        let depth = line.len() - body.len();
        while open.last().is_some_and(|(top, _)| *top >= depth) {
            close(&mut open, &mut roots);
        }
        open.push((depth, mysql_tree_node(step)));
    }
    while !open.is_empty() {
        close(&mut open, &mut roots);
    }
    roots
}

/// Ends the last open step: it goes into the step above it, or is a top step.
fn close(open: &mut Vec<(usize, PlanNode)>, roots: &mut Vec<PlanNode>) {
    if let Some((_, node)) = open.pop() {
        match open.last_mut() {
            Some((_, parent)) => parent.children.push(node),
            None => roots.push(node),
        }
    }
}

fn mysql_tree_node(step: &str) -> PlanNode {
    let mut node = PlanNode::default();
    let mut rest = step.trim_end();
    // The numbers are in groups at the end: `(cost=…)`, `(actual time=…)`, `(never executed)`.
    while let Some(start) = rest.rfind(" (") {
        let group = &rest[start + 2..];
        let Some(inner) = group.strip_suffix(')') else {
            break;
        };
        if inner == "never executed" {
            node.loops = Some(0.0);
            node.actual_rows = Some(0.0);
            node.time_ms = Some(0.0);
        } else if let Some(numbers) = inner.strip_prefix("actual ") {
            let (last_row, rows, loops) = (
                pair_end(numbers, "time="),
                field(numbers, "rows="),
                field(numbers, "loops="),
            );
            node.actual_rows = rows;
            node.loops = loops;
            // The time is to the last row of one loop.
            node.time_ms = last_row.map(|time| time * loops.unwrap_or(1.0));
        } else if inner.starts_with("cost=") || inner.starts_with("rows=") {
            node.cost = pair_end(inner, "cost=");
            node.estimated_rows = field(inner, "rows=");
        } else {
            break;
        }
        rest = rest[..start].trim_end();
    }
    // `Filter: (a on b)` and `Index lookup on t (a: b)`: the first marker ends the name.
    let colon = rest.find(": ");
    let on = rest.find(" on ");
    let (operation, object) = match (colon, on) {
        (Some(colon), on) if on.is_none_or(|on| colon < on) => {
            detail(&mut node.details, "Detail", &rest[colon + 2..]);
            (&rest[..colon], None)
        }
        (_, Some(on)) => (&rest[..on], Some(rest[on + 4..].to_owned())),
        _ => (rest, None),
    };
    // Only the name of the step and of an inner table (`<temporary>`) count, not a condition.
    let lower = operation.to_ascii_lowercase();
    let inner = object.as_deref().filter(|name| name.starts_with('<'));
    if operation == "Table scan" && object.is_some() && inner.is_none() {
        add(&mut node.problems, PlanProblem::FullScan);
    }
    if operation.starts_with("Sort") {
        add(&mut node.problems, PlanProblem::Filesort);
    }
    if lower.contains("temporary")
        || lower.contains("materialize")
        || inner.is_some_and(|name| name.contains("temporary"))
    {
        add(&mut node.problems, PlanProblem::TempTable);
    }
    operation.clone_into(&mut node.operation);
    node.object = object;
    node
}

/// The number after `name` in `text`, like `rows=20`. MySQL can write `626e-6`.
fn field(text: &str, name: &str) -> Option<f64> {
    let start = text.find(name)? + name.len();
    let value = text[start..].split_whitespace().next()?;
    value.parse().ok()
}

/// The second number of a pair like `time=0.17..4.5`, or the only one.
fn pair_end(text: &str, name: &str) -> Option<f64> {
    let start = text.find(name)? + name.len();
    let value = text[start..].split_whitespace().next()?;
    value.rsplit("..").next()?.parse().ok()
}

// ---------------------------------------------------------------------------------------
// SQLite: EXPLAIN QUERY PLAN

/// The rows are `id, parent, notused, detail`. Parent 0 is the top.
fn sqlite(rows: &[Vec<Option<String>>]) -> Option<Vec<PlanNode>> {
    let mut steps = Vec::new();
    for row in rows {
        let cell = |index: usize| row.get(index).cloned().flatten();
        let id: i64 = cell(0)?.parse().ok()?;
        let parent: i64 = cell(1)?.parse().ok()?;
        steps.push((id, parent, sqlite_node(&cell(3).unwrap_or_default())));
    }
    Some(sqlite_children(&mut steps, 0))
}

/// Takes the steps under `parent` out of `steps`, each with its own children.
fn sqlite_children(steps: &mut Vec<(i64, i64, PlanNode)>, parent: i64) -> Vec<PlanNode> {
    let mut nodes = Vec::new();
    while let Some(index) = steps.iter().position(|(_, of, _)| *of == parent) {
        let (id, _, mut node) = steps.remove(index);
        // An id that is its own parent would loop forever.
        if id != parent {
            node.children = sqlite_children(steps, id);
        }
        nodes.push(node);
    }
    nodes
}

fn sqlite_node(detail: &str) -> PlanNode {
    let mut node = PlanNode::default();
    let (operation, object) = match detail.split_once(' ') {
        Some((word @ ("SCAN" | "SEARCH"), rest)) => {
            // Before 3.36 it was `SCAN TABLE t`.
            let rest = rest.strip_prefix("TABLE ").unwrap_or(rest);
            let name = if word == "SCAN" { "Scan" } else { "Search" };
            (name.to_owned(), Some(rest.to_owned()))
        }
        _ => (detail.to_owned(), None),
    };
    if let Some(object) = &object
        && operation == "Scan"
        && !object.contains("USING")
        && !object.starts_with('(')
        && !object.starts_with("CONSTANT")
    {
        add(&mut node.problems, PlanProblem::FullScan);
    }
    if detail.starts_with("USE TEMP B-TREE") {
        let problem = if detail.contains("ORDER BY") {
            PlanProblem::Filesort
        } else {
            PlanProblem::TempTable
        };
        add(&mut node.problems, problem);
    }
    if detail.starts_with("MATERIALIZE") {
        add(&mut node.problems, PlanProblem::TempTable);
    }
    node.operation = operation;
    node.object = object;
    node
}

// ---------------------------------------------------------------------------------------
// SQL Server: SHOWPLAN_XML and STATISTICS XML

/// Steps that write rows to a work table in `tempdb`.
const MSSQL_SPOOLS: [&str; 3] = ["Table Spool", "Index Spool", "Row Count Spool"];

fn mssql(text: &str) -> Option<Vec<PlanNode>> {
    let document = roxmltree::Document::parse(text).ok()?;
    let nodes = document
        .descendants()
        .filter(|element| element.has_tag_name("QueryPlan"))
        .filter_map(|plan| plan.children().find(|child| child.has_tag_name("RelOp")))
        .map(mssql_node)
        .collect();
    Some(nodes)
}

fn mssql_node(relop: roxmltree::Node) -> PlanNode {
    let attribute = |name: &str| relop.attribute(name).and_then(|value| value.parse().ok());
    let physical = relop.attribute("PhysicalOp").unwrap_or_default();
    let logical = relop.attribute("LogicalOp").unwrap_or_default();
    let operation = if logical.is_empty() || logical == physical {
        physical.to_owned()
    } else {
        format!("{physical} ({logical})")
    };
    let mut node = PlanNode {
        operation,
        estimated_rows: attribute("EstimateRows"),
        cost: attribute("EstimatedTotalSubtreeCost"),
        ..PlanNode::default()
    };
    let mut own = Vec::new();
    own_elements(relop, &mut own);
    if let Some(object) = own.iter().find(|element| element.has_tag_name("Object")) {
        node.object = Some(mssql_object(*object));
    }
    if let Some(predicate) = own.iter().find(|element| element.has_tag_name("Predicate")) {
        let scalar = predicate
            .descendants()
            .find_map(|element| element.attribute("ScalarString"));
        detail(&mut node.details, "Filter", scalar.unwrap_or_default());
    }
    let warnings: Vec<&str> = own
        .iter()
        .filter(|element| element.has_tag_name("Warnings"))
        .flat_map(|warnings| warnings.children().filter(roxmltree::Node::is_element))
        .map(|warning| warning.tag_name().name())
        .collect();
    detail(&mut node.details, "Warnings", warnings.join(", "));
    mssql_runtime(relop, &mut node);
    match physical {
        "Table Scan" | "Clustered Index Scan" => add(&mut node.problems, PlanProblem::FullScan),
        "Sort" => add(&mut node.problems, PlanProblem::Filesort),
        _ => {}
    }
    let spills = warnings.iter().any(|warning| warning.contains("Spill"));
    if MSSQL_SPOOLS.contains(&physical) || spills {
        add(&mut node.problems, PlanProblem::TempTable);
    }
    node.children = own
        .iter()
        .filter(|element| element.has_tag_name("RelOp"))
        .map(|child| mssql_node(*child))
        .collect();
    node
}

/// The elements inside `relop` that belong to it: all but those of the steps below it.
/// The steps below it are in the list, but not what is inside them.
fn own_elements<'a, 'input>(
    relop: roxmltree::Node<'a, 'input>,
    found: &mut Vec<roxmltree::Node<'a, 'input>>,
) {
    for child in relop.children().filter(roxmltree::Node::is_element) {
        found.push(child);
        if !child.has_tag_name("RelOp") {
            own_elements(child, found);
        }
    }
}

/// Like `orders o using PK_orders`. The brackets are left out.
fn mssql_object(object: roxmltree::Node) -> String {
    let part = |name: &str| {
        object
            .attribute(name)
            .unwrap_or_default()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_owned()
    };
    let mut text = part("Table");
    let alias = part("Alias");
    if !alias.is_empty() && alias != text {
        text = format!("{text} {alias}");
    }
    let index = part("Index");
    if !index.is_empty() {
        text = format!("{text} using {index}");
    }
    text
}

/// The real rows, loops and time of `STATISTICS XML`, added up over all threads.
fn mssql_runtime(relop: roxmltree::Node, node: &mut PlanNode) {
    let Some(runtime) = relop
        .children()
        .find(|child| child.has_tag_name("RunTimeInformation"))
    else {
        return;
    };
    let (mut rows, mut loops, mut batch) = (0.0, 0.0, false);
    // SQL Server 2014 and older give no time. Then the step has none, not 0.
    let mut elapsed: Option<f64> = None;
    for thread in runtime
        .children()
        .filter(|child| child.has_tag_name("RunTimeCountersPerThread"))
    {
        let get = |name: &str| {
            thread
                .attribute(name)
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(0.0)
        };
        rows += get("ActualRows");
        loops += get("ActualExecutions");
        // Threads run at the same time, so the slowest one is the time of the step.
        if let Some(time) = thread
            .attribute("ActualElapsedms")
            .and_then(|value| value.parse::<f64>().ok())
        {
            elapsed = Some(elapsed.map_or(time, |slowest| slowest.max(time)));
        }
        batch |= thread.attribute("ActualExecutionMode") == Some("Batch");
    }
    node.loops = Some(loops);
    node.actual_rows = Some(if loops > 0.0 { rows / loops } else { 0.0 });
    // In row mode the time holds the steps below. In batch mode it is the step alone.
    if batch {
        node.own_time = elapsed;
    } else {
        node.time_ms = elapsed;
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, plan};
    use crate::model::{
        PlanFormat, PlanHeat, PlanMeasure, PlanNode, PlanProblem, QueryPlan, ResultSet,
    };

    fn cell(text: &str) -> ResultSet {
        ResultSet {
            columns: vec!["QUERY PLAN".to_owned()],
            rows: vec![vec![Some(text.to_owned())]],
            ..ResultSet::default()
        }
    }

    fn read(format: PlanFormat, text: &str, analyzed: bool) -> QueryPlan {
        let nodes = parse(format, &cell(text)).expect("the plan can be read");
        plan(nodes, analyzed, None)
    }

    /// All steps from the top down, like the tree shows them.
    fn flat(nodes: &[PlanNode]) -> Vec<&PlanNode> {
        let mut all = Vec::new();
        for node in nodes {
            all.push(node);
            all.extend(flat(&node.children));
        }
        all
    }

    fn find<'a>(plan: &'a QueryPlan, operation: &str) -> &'a PlanNode {
        flat(&plan.nodes)
            .into_iter()
            .find(|node| node.operation == operation)
            .unwrap_or_else(|| panic!("no step {operation}"))
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn postgres_plan_has_costs_and_full_scans() {
        let plan = read(
            PlanFormat::PostgresJson,
            include_str!("../../tests/plans/postgres_plan.json"),
            false,
        );
        assert_eq!(plan.measure, Some(PlanMeasure::Cost));
        assert!(close(plan.total.unwrap_or_default(), 502.26));
        let operations: Vec<&str> = flat(&plan.nodes)
            .iter()
            .map(|node| node.operation.as_str())
            .collect();
        assert_eq!(
            operations,
            [
                "Sort",
                "Aggregate",
                "Hash Join",
                "Seq Scan",
                "Hash",
                "Seq Scan"
            ]
        );
        let scan = find(&plan, "Seq Scan");
        assert_eq!(scan.object.as_deref(), Some("orders o"));
        assert_eq!(scan.problems, [PlanProblem::FullScan]);
        // The scan of orders costs 378 of 502: it is the slow step.
        assert_eq!(scan.heat, Some(PlanHeat::Slow));
        assert!(close(scan.share.unwrap_or_default(), 378.0 / 502.26));
        assert_eq!(find(&plan, "Sort").problems, [PlanProblem::Filesort]);
        assert!(find(&plan, "Sort").actual_rows.is_none());
        // Hash Join: 451.53 - 378 - 81 (the Hash) = its own 0.49 is under 10%.
        assert_eq!(find(&plan, "Hash Join").heat, None);
    }

    #[test]
    fn postgres_analyze_measures_time() {
        let plan = read(
            PlanFormat::PostgresJson,
            include_str!("../../tests/plans/postgres_analyze.json"),
            true,
        );
        assert_eq!(plan.measure, Some(PlanMeasure::Time));
        assert!(close(plan.total.unwrap_or_default(), 3.466));
        let scan = find(&plan, "Seq Scan");
        assert_eq!(scan.actual_rows, Some(6666.0));
        assert_eq!(scan.loops, Some(1.0));
        // The good estimates get no badge.
        assert!(
            flat(&plan.nodes)
                .iter()
                .all(|node| !node.problems.contains(&PlanProblem::RowEstimate))
        );
        let heat: Vec<_> = flat(&plan.nodes).iter().map(|node| node.heat).collect();
        assert!(heat.contains(&Some(PlanHeat::Slow)));
    }

    #[test]
    fn postgres_disk_sort_and_bad_estimate() {
        let plan = read(
            PlanFormat::PostgresJson,
            include_str!("../../tests/plans/postgres_analyze_disk_sort.json"),
            true,
        );
        let sort = find(&plan, "Sort");
        assert_eq!(
            sort.problems,
            [
                PlanProblem::Filesort,
                PlanProblem::TempTable,
                PlanProblem::RowEstimate
            ]
        );
        assert!(
            sort.details
                .iter()
                .any(|detail| detail.value == "external merge")
        );
        // 33 rows were expected, 6666 came.
        let scan = flat(&plan.nodes)
            .into_iter()
            .find(|node| node.object.as_deref() == Some("orders o"))
            .expect("scan of orders");
        assert!(scan.problems.contains(&PlanProblem::RowEstimate));
        let customers = flat(&plan.nodes)
            .into_iter()
            .find(|node| node.object.as_deref() == Some("customers c"))
            .expect("scan of customers");
        assert!(!customers.problems.contains(&PlanProblem::RowEstimate));
    }

    #[test]
    fn postgres_time_counts_all_loops() {
        let plan = read(
            PlanFormat::PostgresJson,
            include_str!("../../tests/plans/postgres_analyze_loops.json"),
            true,
        );
        let inner = flat(&plan.nodes)
            .into_iter()
            .find(|node| node.object.as_deref() == Some("orders o using orders_customer"))
            .expect("index scan");
        assert_eq!(inner.loops, Some(49.0));
        // 0.005 ms for each of the 49 loops.
        assert!(close(inner.time_ms.unwrap_or_default(), 0.245));
        assert_eq!(inner.problems, []);
        let join = find(&plan, "Nested Loop");
        assert!(join.share.unwrap_or_default() < 0.3);
    }

    #[test]
    fn mysql_plan_turns_the_join_list_into_a_tree() {
        let plan = read(
            PlanFormat::MysqlJson,
            include_str!("../../tests/plans/mysql_plan.json"),
            false,
        );
        assert_eq!(plan.measure, Some(PlanMeasure::Cost));
        assert!(close(plan.total.unwrap_or_default(), 2656.75));
        let operations: Vec<&str> = flat(&plan.nodes)
            .iter()
            .map(|node| node.operation.as_str())
            .collect();
        assert_eq!(
            operations,
            [
                "Query block #1",
                "Order",
                "Group",
                "Nested loop",
                "Full table scan",
                "Unique index lookup"
            ]
        );
        assert_eq!(find(&plan, "Order").problems, [PlanProblem::Filesort]);
        assert_eq!(find(&plan, "Group").problems, [PlanProblem::TempTable]);
        let scan = find(&plan, "Full table scan");
        assert_eq!(scan.object.as_deref(), Some("o"));
        assert_eq!(scan.problems, [PlanProblem::FullScan]);
        assert_eq!(scan.estimated_rows, Some(19500.0));
        // read 1779.25 + eval 195 of 2656.75.
        assert!(close(scan.cost.unwrap_or_default(), 1974.25));
        assert_eq!(scan.heat, Some(PlanHeat::Slow));
        let lookup = find(&plan, "Unique index lookup");
        assert_eq!(lookup.object.as_deref(), Some("c using PRIMARY"));
        assert_eq!(lookup.heat, Some(PlanHeat::Warm));
    }

    #[test]
    fn mysql_plan_of_a_union() {
        let plan = read(
            PlanFormat::MysqlJson,
            include_str!("../../tests/plans/mysql_plan_union.json"),
            false,
        );
        let union = find(&plan, "Union");
        assert_eq!(union.object.as_deref(), Some("<union1,3>"));
        assert_eq!(union.problems, [PlanProblem::TempTable]);
        assert_eq!(union.children.len(), 2);
        assert!(close(plan.total.unwrap_or_default(), 959.76 + 1.06));
        assert_eq!(
            find(&plan, "Index range scan").object.as_deref(),
            Some("customers using PRIMARY")
        );
    }

    #[test]
    fn mariadb_plan_has_its_own_fields() {
        let plan = read(
            PlanFormat::MysqlJson,
            include_str!("../../tests/plans/mariadb_plan.json"),
            false,
        );
        let operations: Vec<&str> = flat(&plan.nodes)
            .iter()
            .map(|node| node.operation.as_str())
            .collect();
        assert_eq!(
            operations,
            [
                "Query block #1",
                "Sort",
                "Temporary table",
                "Nested loop",
                "Full table scan",
                "Index lookup"
            ]
        );
        assert_eq!(find(&plan, "Sort").problems, [PlanProblem::Filesort]);
        assert_eq!(
            find(&plan, "Temporary table").problems,
            [PlanProblem::TempTable]
        );
        assert!(close(plan.total.unwrap_or_default(), 19.968));
        let lookup = find(&plan, "Index lookup");
        assert_eq!(lookup.estimated_rows, Some(9.0));
        assert_eq!(lookup.heat, Some(PlanHeat::Slow));
        assert_eq!(find(&plan, "Full table scan").heat, None);
    }

    #[test]
    fn mariadb_analyze_measures_time() {
        let plan = read(
            PlanFormat::MysqlJson,
            include_str!("../../tests/plans/mariadb_analyze.json"),
            true,
        );
        assert_eq!(plan.measure, Some(PlanMeasure::Time));
        assert!(close(plan.total.unwrap_or_default(), 18.5586));
        let lookup = find(&plan, "Index lookup");
        assert_eq!(lookup.actual_rows, Some(10.0));
        assert_eq!(lookup.loops, Some(2000.0));
        assert!(close(lookup.time_ms.unwrap_or_default(), 14.6857 + 3.3284));
        assert_eq!(lookup.heat, Some(PlanHeat::Slow));
        assert_eq!(find(&plan, "Sort").heat, None);
    }

    #[test]
    fn mariadb_analyze_of_a_union_finds_the_subquery() {
        let plan = read(
            PlanFormat::MysqlJson,
            include_str!("../../tests/plans/mariadb_analyze_union.json"),
            true,
        );
        let materialize = find(&plan, "Materialize");
        assert_eq!(materialize.problems, [PlanProblem::TempTable]);
        let scans: Vec<_> = flat(&plan.nodes)
            .into_iter()
            .filter(|node| node.operation == "Full table scan")
            .collect();
        assert!(
            scans
                .iter()
                .any(|scan| scan.object.as_deref() == Some("orders"))
        );
        // 0.178 rows came for each loop where 1 was expected: a small gap, no badge.
        let subquery = flat(&plan.nodes)
            .into_iter()
            .find(|node| node.object.as_deref() == Some("<subquery2> using distinct_key"))
            .expect("subquery");
        assert!(!subquery.problems.contains(&PlanProblem::RowEstimate));
    }

    #[test]
    fn mysql_analyze_tree() {
        let plan = read(
            PlanFormat::MysqlTree,
            include_str!("../../tests/plans/mysql_analyze.txt"),
            true,
        );
        assert_eq!(plan.measure, Some(PlanMeasure::Time));
        assert!(close(plan.total.unwrap_or_default(), 12.0));
        let operations: Vec<&str> = flat(&plan.nodes)
            .iter()
            .map(|node| node.operation.as_str())
            .collect();
        assert_eq!(
            operations,
            [
                "Sort",
                "Table scan",
                "Aggregate using temporary table",
                "Nested loop inner join",
                "Filter",
                "Table scan",
                "Single-row index lookup"
            ]
        );
        let sort = find(&plan, "Sort");
        assert_eq!(sort.problems, [PlanProblem::Filesort]);
        assert_eq!(sort.details[0].value, "`sum(o.total)` DESC");
        let temporary = &plan.nodes[0].children[0];
        assert_eq!(temporary.object.as_deref(), Some("<temporary>"));
        assert_eq!(temporary.problems, [PlanProblem::TempTable]);
        let scan = flat(&plan.nodes)
            .into_iter()
            .find(|node| node.object.as_deref() == Some("o"))
            .expect("scan of o");
        assert_eq!(scan.problems, [PlanProblem::FullScan]);
        assert_eq!(scan.estimated_rows, Some(19500.0));
        assert_eq!(scan.actual_rows, Some(20000.0));
        assert!(close(scan.cost.unwrap_or_default(), 1974.0));
        // The lookup took 652e-6 ms for each of 6666 loops.
        let lookup = find(&plan, "Single-row index lookup");
        assert_eq!(lookup.loops, Some(6666.0));
        assert!(close(lookup.time_ms.unwrap_or_default(), 652e-6 * 6666.0));
        assert_eq!(
            lookup.object.as_deref(),
            Some("c using PRIMARY (id=o.customer_id)")
        );
        // 1950 rows were expected from the filter, 6666 came: under the gap of 10.
        assert!(
            !find(&plan, "Filter")
                .problems
                .contains(&PlanProblem::RowEstimate)
        );
    }

    #[test]
    fn mysql_analyze_tree_of_a_union() {
        let plan = read(
            PlanFormat::MysqlTree,
            include_str!("../../tests/plans/mysql_analyze_union.txt"),
            true,
        );
        assert_eq!(plan.nodes.len(), 1);
        let root = &plan.nodes[0];
        assert_eq!(root.object.as_deref(), Some("<union temporary>"));
        assert_eq!(
            root.children[0].operation,
            "Union materialize with deduplication"
        );
        assert_eq!(root.children[0].children.len(), 2);
        let lookup = find(&plan, "Index lookup");
        assert_eq!(lookup.actual_rows, Some(8.4));
        assert_eq!(lookup.loops, Some(1111.0));
    }

    #[test]
    fn mysql_tree_step_that_never_ran() {
        let text = "-> Nested loop inner join  (cost=0.7 rows=1) (actual time=0.0206..0.0206 rows=0 loops=1)\n    \
            -> Table scan on c  (cost=0.35 rows=1) (actual time=0.0183..0.0183 rows=0 loops=1)\n    \
            -> Filter: (o.`status` = 'paid')  (cost=0.35 rows=1) (never executed)\n        \
            -> Index lookup on o using orders_customer (customer_id=c.id)  (cost=0.35 rows=1) (never executed)";
        let plan = read(PlanFormat::MysqlTree, text, true);
        let filter = find(&plan, "Filter");
        assert_eq!(filter.loops, Some(0.0));
        assert_eq!(filter.time_ms, Some(0.0));
        assert_eq!(filter.details[0].value, "(o.`status` = 'paid')");
        // A step that never ran has no estimate gap.
        assert_eq!(filter.problems, []);
        assert_eq!(filter.children.len(), 1);
    }

    #[test]
    fn sqlite_rows_become_a_tree() {
        let rows: Vec<Vec<Option<String>>> = [
            ("2", "0", "MERGE (UNION)"),
            ("4", "2", "LEFT"),
            (
                "8",
                "4",
                "SEARCH c USING INDEX sqlite_autoindex_customers_1 (id=?)",
            ),
            ("12", "4", "LIST SUBQUERY 1"),
            ("14", "12", "SCAN orders"),
            ("39", "4", "USE TEMP B-TREE FOR ORDER BY"),
            ("50", "2", "RIGHT"),
            (
                "54",
                "50",
                "SCAN customers USING COVERING INDEX sqlite_autoindex_customers_1",
            ),
            ("66", "50", "USE TEMP B-TREE FOR DISTINCT"),
        ]
        .iter()
        .map(|(id, parent, detail)| {
            vec![
                Some((*id).to_owned()),
                Some((*parent).to_owned()),
                Some("0".to_owned()),
                Some((*detail).to_owned()),
            ]
        })
        .collect();
        let set = ResultSet {
            columns: ["id", "parent", "notused", "detail"]
                .map(str::to_owned)
                .to_vec(),
            rows,
            ..ResultSet::default()
        };
        let plan = plan(
            parse(PlanFormat::SqliteRows, &set).expect("plan"),
            false,
            None,
        );
        assert_eq!(plan.measure, None);
        assert_eq!(plan.nodes.len(), 1);
        let union = &plan.nodes[0];
        assert_eq!(union.children.len(), 2);
        assert_eq!(union.children[0].children.len(), 3);
        let search = &union.children[0].children[0];
        assert_eq!(search.operation, "Search");
        assert_eq!(search.problems, []);
        let scan = &union.children[0].children[1].children[0];
        assert_eq!(scan.object.as_deref(), Some("orders"));
        assert_eq!(scan.problems, [PlanProblem::FullScan]);
        assert_eq!(
            union.children[0].children[2].problems,
            [PlanProblem::Filesort]
        );
        assert_eq!(union.children[1].children[0].problems, []);
        assert_eq!(
            union.children[1].children[1].problems,
            [PlanProblem::TempTable]
        );
        assert!(flat(&plan.nodes).iter().all(|node| node.heat.is_none()));
    }

    #[test]
    fn mssql_showplan() {
        let plan = read(
            PlanFormat::MssqlXml,
            include_str!("../../tests/plans/mssql_plan.xml"),
            false,
        );
        assert_eq!(plan.measure, Some(PlanMeasure::Cost));
        assert!(close(plan.total.unwrap_or_default(), 0.299_323));
        assert_eq!(plan.nodes.len(), 1);
        assert_eq!(plan.nodes[0].operation, "Sort");
        assert_eq!(plan.nodes[0].problems, [PlanProblem::Filesort]);
        let scans: Vec<_> = flat(&plan.nodes)
            .into_iter()
            .filter(|node| node.problems.contains(&PlanProblem::FullScan))
            .collect();
        assert_ne!(scans.len(), 0);
        assert!(scans.iter().all(|scan| {
            scan.object
                .as_deref()
                .is_some_and(|name| name.contains(" using PK__"))
        }));
        assert!(
            flat(&plan.nodes)
                .iter()
                .any(|node| node.heat == Some(PlanHeat::Slow))
        );
        assert!(flat(&plan.nodes).iter().all(|node| node.time_ms.is_none()));
    }

    #[test]
    fn mssql_statistics_xml_has_real_rows() {
        let plan = read(
            PlanFormat::MssqlXml,
            include_str!("../../tests/plans/mssql_analyze.xml"),
            true,
        );
        assert_eq!(plan.measure, Some(PlanMeasure::Time));
        let root = &plan.nodes[0];
        assert_eq!(root.actual_rows, Some(20.0));
        assert_eq!(root.loops, Some(1.0));
        assert_eq!(root.time_ms, Some(7.0));
        assert!(close(plan.total.unwrap_or_default(), 7.0));
    }

    #[test]
    fn mssql_statistics_xml_of_a_union() {
        let plan = read(
            PlanFormat::MssqlXml,
            include_str!("../../tests/plans/mssql_analyze_union.xml"),
            true,
        );
        let operations: Vec<&str> = flat(&plan.nodes)
            .iter()
            .map(|node| node.operation.as_str())
            .collect();
        assert_eq!(operations[0], "Merge Join (Union)");
        assert!(operations.contains(&"Sort (Distinct Sort)"));
        assert!(operations.contains(&"Clustered Index Seek"));
        let sort = find(&plan, "Sort (Distinct Sort)");
        assert_eq!(sort.problems, [PlanProblem::Filesort]);
        // 3960 rows read where 3960 were expected.
        let scan = flat(&plan.nodes)
            .into_iter()
            .find(|node| node.actual_rows == Some(3960.0))
            .expect("scan");
        assert_eq!(scan.problems, [PlanProblem::FullScan]);
    }

    #[test]
    fn mysql_steps_keep_the_order_of_the_server() {
        let plan = read(
            PlanFormat::MysqlJson,
            include_str!("../../tests/plans/mysql_plan_subquery.json"),
            false,
        );
        let block = &plan.nodes[0];
        // The table comes before the subquery of the select list, as MySQL wrote it.
        let operations: Vec<&str> = block
            .children
            .iter()
            .map(|node| node.operation.as_str())
            .collect();
        assert_eq!(operations, ["Index range scan", "Query block #2"]);
    }

    #[test]
    fn mysql_tree_badges_come_from_the_step_name_only() {
        let text = "-> Filter: (t.temporary_until > now())  (cost=1 rows=1) (actual time=0.01..0.02 rows=1 loops=1)\n    \
            -> Index lookup on t using k (note='Sort: a')  (cost=1 rows=1) (actual time=0.01..0.01 rows=1 loops=1)";
        let plan = read(PlanFormat::MysqlTree, text, true);
        let filter = &plan.nodes[0];
        assert_eq!(filter.operation, "Filter");
        assert_eq!(filter.problems, []);
        let lookup = &filter.children[0];
        assert_eq!(lookup.operation, "Index lookup");
        assert_eq!(lookup.object.as_deref(), Some("t using k (note='Sort: a')"));
        assert_eq!(lookup.problems, []);
    }

    #[test]
    fn mssql_without_times_has_no_time() {
        // SQL Server 2014 and older write no ActualElapsedms.
        let text = include_str!("../../tests/plans/mssql_analyze.xml");
        let mut old = String::new();
        let mut rest = text;
        while let Some(start) = rest.find(" ActualElapsedms=\"") {
            old.push_str(&rest[..start]);
            let value = &rest[start + 18..];
            rest = &value[value.find('"').map_or(0, |end| end + 1)..];
        }
        old.push_str(rest);
        assert!(!old.contains("ActualElapsedms"));
        let plan = read(PlanFormat::MssqlXml, &old, true);
        assert_eq!(plan.nodes[0].actual_rows, Some(20.0));
        assert_eq!(plan.nodes[0].time_ms, None);
        assert_eq!(plan.measure, Some(PlanMeasure::Cost));
    }

    #[test]
    fn bad_output_gives_no_tree() {
        assert!(parse(PlanFormat::PostgresJson, &cell("Seq Scan on t")).is_none());
        assert!(parse(PlanFormat::MysqlJson, &cell("{}")).is_none());
        assert!(parse(PlanFormat::MssqlXml, &cell("<x")).is_none());
        assert!(parse(PlanFormat::MysqlTree, &cell("")).is_none());
    }
}
