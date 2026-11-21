//! The operator axis for canon as one predicate that every canon reading path shares.

/// ` AND (...)` over the `named_entities` row aliased `alias`.
/// Reads the reader key from placeholder `$param` (TEXT, NULL when the Host names no operator).
pub(crate) fn visible(alias: &str, param: usize) -> String {
    format!(
        " AND ({alias}.subject IS NULL OR {alias}.scope = 'company' OR {alias}.subject = ${param})"
    )
}

/// `superseded_by` of the row aliased `alias`, or NULL when the reader cannot see that successor.
pub(crate) fn visible_successor(alias: &str, param: usize) -> String {
    format!(
        "(SELECT successor.id FROM named_entities successor
          WHERE successor.id = {alias}.superseded_by{})",
        visible("successor", param)
    )
}

/// `supersedes` of the row aliased `alias`, keeping only the IDs that the reader can see.
pub(crate) fn visible_predecessors(alias: &str, param: usize) -> String {
    format!(
        "ARRAY(SELECT predecessor.id
               FROM unnest({alias}.supersedes) WITH ORDINALITY AS link(id,position)
               JOIN named_entities predecessor ON predecessor.id = link.id{}
               ORDER BY link.position)",
        visible("predecessor", param)
    )
}
