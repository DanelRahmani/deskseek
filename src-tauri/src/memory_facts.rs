//! Tier 2 memory: facts that are *retrieved* per turn rather than always
//! injected.
//!
//! Tier 1 is `MEMORY.md` — a short, hand-editable file of identity and standing
//! preferences, injected whole on every request. Everything else lives here and
//! is looked up by relevance to what the user just asked, because a store that
//! grows forever cannot be injected whole.
//!
//! Retrieval is keyword based (SQLite FTS5), which is a deliberate choice: it
//! needs no embedding model, no download and no network. The honest cost is that
//! it matches words, not meaning — asking about "the car" will not surface a
//! fact stored as "vehicle".

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::store::db::now_ts;

/// How many facts the ranker considers before the budget trims them.
const CANDIDATES: usize = 40;
/// Longest single fact kept, so one rambling entry cannot eat the budget.
const MAX_FACT_CHARS: usize = 400;
/// Words taken from the user's message. A long paste should not become a query.
const MAX_TERMS: usize = 24;
/// Shortest word worth searching for; `is`, `the` and `a` are noise.
const MIN_TERM_CHARS: usize = 2;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FactRow {
    pub id: String,
    pub scope: String,
    pub kind: String,
    pub text: String,
    pub importance: i64,
    pub source_chat_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// The scope key for a workspace, or `global` when there is none.
pub fn scope_for(workspace: Option<&Path>) -> String {
    match workspace {
        Some(path) => format!("project:{}", path.display()),
        None => "global".to_string(),
    }
}

/// Lowercased, whitespace-collapsed, trailing punctuation removed. This is the
/// dedup key: "User is called Dan." and "user is called dan" are one fact.
pub fn normalise(text: &str) -> String {
    let lowered = text.to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    let mut last_space = true;
    for ch in lowered.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim()
        .trim_end_matches(['.', '!', '?', ',', ';', ':', ' '])
        .to_string()
}

/// A fact, or a replacement for one that already exists.
///
/// Dedup is by normalised text, so the same fact proposed twice updates the
/// original rather than stacking a near-duplicate. Importance only ever goes up,
/// and a fact keeps the scope it was first filed under — a global fact does not
/// become project-local because a later extraction happened in a project.
pub fn upsert(
    conn: &Connection,
    scope: &str,
    kind: &str,
    text: &str,
    source_chat_id: Option<&str>,
    importance: i64,
) -> rusqlite::Result<Option<FactRow>> {
    let text = text.trim();
    let norm = normalise(text);
    if norm.is_empty() {
        return Ok(None);
    }
    let ts = now_ts();
    conn.execute(
        "INSERT INTO memory_facts
             (id, scope, kind, text, norm, tags, importance, source_chat_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, '', ?6, ?7, ?8, ?8)
         ON CONFLICT(norm) DO UPDATE SET
             text       = excluded.text,
             importance = MAX(memory_facts.importance, excluded.importance),
             updated_at = excluded.updated_at",
        params![
            uuid::Uuid::new_v4().to_string(),
            scope,
            kind,
            text,
            norm,
            importance.clamp(1, 3),
            source_chat_id,
            ts
        ],
    )?;
    get_by_norm(conn, &norm)
}

pub fn get_by_norm(conn: &Connection, norm: &str) -> rusqlite::Result<Option<FactRow>> {
    conn.query_row(&format!("{SELECT_FACT} WHERE norm = ?1"), [norm], map_fact)
        .optional()
}

pub fn list(conn: &Connection, scope: Option<&str>) -> rusqlite::Result<Vec<FactRow>> {
    let mut out = Vec::new();
    match scope {
        Some(scope) => {
            let mut stmt = conn.prepare(&format!(
                "{SELECT_FACT} WHERE scope = ?1 ORDER BY updated_at DESC"
            ))?;
            for row in stmt.query_map([scope], map_fact)? {
                out.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(&format!(
                "{SELECT_FACT} ORDER BY updated_at DESC LIMIT 500"
            ))?;
            for row in stmt.query_map([], map_fact)? {
                out.push(row?);
            }
        }
    }
    Ok(out)
}

pub fn delete(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM memory_facts WHERE id = ?1", [id])?;
    Ok(())
}

const SELECT_FACT: &str = "SELECT id, scope, kind, text, importance, source_chat_id, created_at, updated_at \
                           FROM memory_facts";

fn map_fact(r: &rusqlite::Row<'_>) -> rusqlite::Result<FactRow> {
    Ok(FactRow {
        id: r.get(0)?,
        scope: r.get(1)?,
        kind: r.get(2)?,
        text: r.get(3)?,
        importance: r.get(4)?,
        source_chat_id: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
    })
}

/// A safe FTS5 `MATCH` expression for arbitrary user text.
///
/// This matters: FTS5 has its own query syntax, so raw text containing a quote,
/// `*`, `:`, `NEAR`, `AND` or a stray `-` is either a syntax error or silently
/// means something else. Every word is therefore wrapped in double quotes — with
/// embedded quotes doubled — and the words are OR'd together. Words that carry
/// no letters or digits are dropped, and `None` means "nothing worth searching
/// for", which the caller treats as "no tier 2 this turn".
pub fn match_expression(query: &str) -> Option<String> {
    let mut terms: Vec<String> = Vec::new();
    for word in query.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if word.chars().count() < MIN_TERM_CHARS {
            continue;
        }
        let term = format!("\"{}\"", word.to_lowercase().replace('"', "\"\""));
        if !terms.contains(&term) {
            terms.push(term);
        }
        if terms.len() == MAX_TERMS {
            break;
        }
    }
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" OR "))
    }
}

/// One candidate, already ranked by bm25 in SQL (best first).
#[derive(Debug, Clone)]
struct Candidate {
    text: String,
    norm: String,
    importance: i64,
}

/// Relevance first, importance as a nudge.
///
/// bm25 ordering is all we know about "how well does this match", so its best
/// hit scores 1.0 and its tenth 0.09. Importance adds at most 0.15 — enough to
/// break a near-tie, not enough to let a weak match outrank a strong one.
fn blended(index: usize, importance: i64) -> f64 {
    1.0 / (1.0 + index as f64) + 0.05 * importance.clamp(1, 3) as f64
}

/// Re-rank, drop anything already injected as Tier 1, and stop at the budget.
///
/// Split out from the SQL so the ordering, dedup and cap can be tested without a
/// database.
fn select(candidates: Vec<Candidate>, tier1: &str, budget_chars: usize) -> Vec<String> {
    let tier1 = normalise(tier1);
    let mut ranked: Vec<(usize, Candidate)> = candidates
        .into_iter()
        .enumerate()
        .filter(|(_, c)| tier1.is_empty() || !tier1.contains(&c.norm))
        .collect();

    ranked.sort_by(|a, b| {
        blended(b.0, b.1.importance)
            .partial_cmp(&blended(a.0, a.1.importance))
            .unwrap_or(std::cmp::Ordering::Equal)
            // A stable tiebreak, so the same query always gives the same order.
            .then_with(|| a.0.cmp(&b.0))
    });

    let mut out = Vec::new();
    let mut used = 0usize;
    for (_, candidate) in ranked {
        let cost = candidate.text.chars().count().min(MAX_FACT_CHARS) + 3;
        if used + cost > budget_chars {
            // Skip rather than stop: one long fact should not hide the short
            // ones behind it.
            continue;
        }
        used += cost;
        out.push(candidate.text);
    }
    out
}

/// The Tier 2 block for this turn, or `None` when nothing relevant was found.
///
/// `tier1` is the injected MEMORY.md body: anything already in it is not
/// repeated, so the model never sees the same fact twice.
pub fn relevant(
    conn: &Connection,
    query: &str,
    workspace: Option<&Path>,
    tier1: &str,
    budget_chars: usize,
) -> rusqlite::Result<Option<String>> {
    let Some(expression) = match_expression(query) else {
        return Ok(None);
    };

    let candidates: Vec<Candidate> = {
        let mut stmt = conn.prepare(
            "SELECT f.text, f.norm, f.importance
               FROM memory_facts_fts
               JOIN memory_facts f ON f.rowid = memory_facts_fts.rowid
              WHERE memory_facts_fts MATCH ?1
                AND f.scope IN (?2, ?3)
              ORDER BY bm25(memory_facts_fts)
              LIMIT ?4",
        )?;
        let rows = stmt.query_map(
            params![expression, "global", scope_for(workspace), CANDIDATES as i64],
            |r| {
                Ok(Candidate {
                    text: r.get(0)?,
                    norm: r.get(1)?,
                    importance: r.get(2)?,
                })
            },
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let picked = select(candidates, tier1, budget_chars);
    if picked.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        picked
            .into_iter()
            .map(|text| format!("- {text}"))
            .collect::<Vec<_>>()
            .join("\n"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalising_collapses_case_space_and_trailing_punctuation() {
        assert_eq!(normalise("  User  is called DAN.  "), "user is called dan");
        assert_eq!(normalise("Likes tea!"), "likes tea");
        assert_eq!(normalise("a\t b\n c"), "a b c");
    }

    #[test]
    fn fts_syntax_in_the_users_text_cannot_break_the_query() {
        // Every one of these is an FTS5 operator or an unbalanced quote.
        assert_eq!(
            match_expression("what about NEAR(\"cats\" AND dogs*)?").unwrap(),
            "\"what\" OR \"about\" OR \"near\" OR \"cats\" OR \"and\" OR \"dogs\""
        );
        assert_eq!(match_expression("read \"the docs\"").unwrap(), "\"read\" OR \"the\" OR \"docs\"");
        assert_eq!(match_expression("cats - dogs^2").unwrap(), "\"cats\" OR \"dogs\"");
        // Underscores are word characters and must not split a term; `AND` and
        // `OR` are ordinary words once they are inside quotes.
        assert_eq!(match_expression("snake_case OR other").unwrap(), "\"snake_case\" OR \"or\" OR \"other\"");
    }

    #[test]
    fn a_query_with_nothing_searchable_is_no_query() {
        assert!(match_expression("").is_none());
        assert!(match_expression("?!., *-").is_none());
        // Single letters are dropped as noise, so a query of only one-letter
        // words leaves nothing to search for.
        assert!(match_expression("a I").is_none());
        assert!(match_expression("a - b^c").is_none());
    }

    #[test]
    fn repeated_words_are_searched_once_and_the_term_count_is_bounded() {
        assert_eq!(match_expression("tea tea TEA").unwrap(), "\"tea\"");
        let many: String = (0..80).map(|n| format!("word{n} ")).collect();
        let expression = match_expression(&many).unwrap();
        assert_eq!(expression.matches(" OR ").count(), MAX_TERMS - 1);
    }

    #[test]
    fn non_ascii_words_are_kept() {
        // The user writes Dutch and is learning Japanese; both must survive.
        assert_eq!(
            match_expression("hoe zeg ik こんにちは?").unwrap(),
            "\"hoe\" OR \"zeg\" OR \"ik\" OR \"こんにちは\""
        );
    }

    fn candidate(text: &str, importance: i64) -> Candidate {
        Candidate {
            text: text.to_string(),
            norm: normalise(text),
            importance,
        }
    }

    #[test]
    fn relevance_leads_and_importance_only_breaks_near_ties() {
        // The best match scores 1.0 + 0.05 = 1.05, and the runner-up cannot
        // reach that even at importance 3 (0.5 + 0.15 = 0.65), so the top hit is
        // never displaced by a merely important fact.
        let picked = select(vec![candidate("best", 1), candidate("second", 3)], "", 10_000);
        assert_eq!(picked, vec!["best", "second"]);

        // Further down the ranking the scores are close enough for importance to
        // decide: index 2 at importance 1 scores 0.38, index 3 at importance 3
        // scores 0.40.
        let picked = select(
            vec![
                candidate("first", 1),
                candidate("second", 1),
                candidate("third", 1),
                candidate("fourth", 3),
            ],
            "",
            10_000,
        );
        assert_eq!(picked, vec!["first", "second", "fourth", "third"]);
    }

    #[test]
    fn a_fact_already_in_the_injected_file_is_not_repeated() {
        let picked = select(
            vec![candidate("User is called Dan", 1), candidate("User lives in Groningen", 1)],
            "# Memory\n\n- (2026-01-01) user is called dan.",
            10_000,
        );
        assert_eq!(picked, vec!["User lives in Groningen"]);
    }

    #[test]
    fn the_budget_is_respected_without_hiding_short_facts_behind_a_long_one() {
        // A full budget is 24 chars.
        let long = "x".repeat(MAX_FACT_CHARS);
        let picked = select(
            vec![candidate(&long, 1), candidate("short and useful", 1)],
            "",
            24,
        );
        // The long one does not fit, and must not stop the short one being offered.
        assert_eq!(picked, vec!["short and useful"]);
    }

    /// These need a real database, because they are testing SQLite rather than
    /// this module: that the bundled build has FTS5 at all, that the sync
    /// triggers fire, and that scoping filters at the SQL level.
    fn temp_db() -> (std::path::PathBuf, Connection) {
        let dir = std::env::temp_dir().join(format!("deskseek-facts-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = crate::store::db::open(&dir.join("facts.db")).unwrap();
        (dir, conn)
    }

    fn texts(conn: &Connection, query: &str, workspace: Option<&Path>) -> Vec<String> {
        relevant(conn, query, workspace, "", 4000)
            .unwrap()
            .map(|block| block.lines().map(|line| line.trim_start_matches("- ").to_string()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn facts_are_indexed_and_found_by_keyword() {
        let (dir, conn) = temp_db();
        upsert(&conn, "global", "fact", "The user prefers dark roast coffee.", None, 1).unwrap();
        upsert(&conn, "global", "fact", "The user's dog is called Waffle.", None, 1).unwrap();

        let found = texts(&conn, "what kind of coffee do I like?", None);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("coffee"));

        // A word that appears in no fact matches nothing, not everything.
        assert!(texts(&conn, "zeppelin", None).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_same_fact_twice_is_one_row_and_importance_only_rises() {
        let (dir, conn) = temp_db();
        upsert(&conn, "global", "fact", "The user's reply language is Dutch.", None, 1).unwrap();
        upsert(
            &conn,
            "global",
            "fact",
            "  the user's REPLY LANGUAGE is dutch.  ",
            None,
            3,
        )
        .unwrap();

        let rows = list(&conn, None).unwrap();
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].importance, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_project_fact_is_invisible_from_another_workspace() {
        let (dir, conn) = temp_db();
        let here = Path::new("C:/work/alpha");
        let elsewhere = Path::new("C:/work/beta");
        upsert(&conn, &scope_for(Some(here)), "fact", "Alpha uses a Postgres database.", None, 1)
            .unwrap();
        upsert(&conn, "global", "fact", "The user is called Dan.", None, 1).unwrap();

        let from_here = texts(&conn, "postgres", Some(here));
        assert_eq!(from_here.len(), 1);
        // Same question, different project: the alpha fact must not leak.
        assert!(texts(&conn, "postgres", Some(elsewhere)).is_empty());
        // A global fact reaches both.
        assert_eq!(texts(&conn, "what is my name", Some(elsewhere)).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deleting_a_fact_takes_it_out_of_the_index_too() {
        let (dir, conn) = temp_db();
        let fact = upsert(&conn, "global", "fact", "The user hates liquorice.", None, 1)
            .unwrap()
            .unwrap();
        assert_eq!(texts(&conn, "liquorice", None).len(), 1);

        delete(&conn, &fact.id).unwrap();
        assert!(texts(&conn, "liquorice", None).is_empty(), "the delete trigger did not fire");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
