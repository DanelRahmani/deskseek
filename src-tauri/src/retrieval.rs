//! What a citation is, and whether it resolves.
//!
//! One rule in one file, because it is the rule the whole "answer from these
//! sources, with citations" feature rests on: a label the model writes that names
//! nothing it was given is a fabrication, and it has to be catchable without
//! asking the model what it meant. So the label format lives here, the parser
//! that finds citations in an answer lives here, and the check that every one of
//! them names something real lives here.
//!
//! The chunk index and the query it serves arrive with 5.4. The label format and
//! this rule arrived first, in 4.1, because a notebook's sources carry labels
//! before anything is indexed — and a second home for "what a citation is" would
//! be exactly the drift this file exists to prevent.

/// The label one source is cited by: its name, and the heading when the citation
/// is about one part of it.
///
/// `report.pdf`, or `report.pdf#Method`. Deliberately the file's own name rather
/// than a number: a reader can see at a glance whether the source it names is one
/// the notebook holds, and so can the check below.
pub fn label(name: &str, heading: Option<&str>) -> String {
    match heading.map(str::trim).filter(|h| !h.is_empty()) {
        Some(heading) => format!("{}#{heading}", name.trim()),
        None => name.trim().to_string(),
    }
}

/// Looks like a source rather than prose.
///
/// ponytail: a heuristic, and the ceiling is that a source named with no dot and
/// no hash — a bare `README` — is not recognised as a citation. It is here
/// because the alternative is worse: with every bracketed span counted, `[sic]`,
/// an edit marker and a numbered footnote `[1]` all become citations naming
/// nothing, and a rule that fires on prose gets turned off. The upgrade path is
/// to pass the label list in and match against it, which this function already
/// receives at the call site.
fn looks_like_a_source(inner: &str) -> bool {
    !inner.is_empty()
        && !inner.chars().all(|c| c.is_ascii_digit())
        && (inner.contains('.') || inner.contains('#') || inner.contains('/'))
}

/// Every source an answer cites, in order of first mention and without repeats.
///
/// A markdown link — `[text](url)` — is not a citation: a bracket followed by a
/// paren is a link, and every answer that links something would otherwise be
/// reported as citing a source that does not exist.
pub fn cited(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '[' {
            i += 1;
            continue;
        }
        let Some(offset) = chars[i + 1..].iter().position(|c| *c == ']') else {
            break;
        };
        let close = i + 1 + offset;
        let inner: String = chars[i + 1..close].iter().collect();
        let inner = inner.trim().to_string();
        let is_link = chars.get(close + 1) == Some(&'(');
        if !is_link && !inner.contains('\n') && looks_like_a_source(&inner) {
            let known = out.iter().any(|seen| seen.eq_ignore_ascii_case(&inner));
            if !known {
                out.push(inner);
            }
        }
        i = close + 1;
    }
    out
}

/// The citations that name nothing in `available`.
///
/// This is the bug the feature's own spec names: a citation for a source that
/// was not there to be read. Compared without case, because whether the model
/// capitalised a file name is not the point.
///
/// A citation may name a *part* of a source — `report.pdf#Method` — so a label is
/// held against both the full label and the name before its hash.
pub fn unresolved(text: &str, available: &[String]) -> Vec<String> {
    let known = |citation: &str| {
        let name = citation.split('#').next().unwrap_or(citation).trim();
        available.iter().any(|label| {
            label.eq_ignore_ascii_case(citation)
                || label.eq_ignore_ascii_case(name)
                || label.split('#').next().unwrap_or(label).eq_ignore_ascii_case(name)
        })
    };
    cited(text).into_iter().filter(|c| !known(c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn a_citation_names_its_source_and_the_part_it_used() {
        assert_eq!(label("report.pdf", None), "report.pdf");
        assert_eq!(label("report.pdf", Some("  ")), "report.pdf");
        assert_eq!(label("report.pdf", Some(" Method ")), "report.pdf#Method");
    }

    #[test]
    fn a_citation_that_names_nothing_is_caught() {
        let available = names(&["q3.pdf", "notes.md"]);
        // Two real ones and one invented: the invented one is what this rule is
        // for, and it is reported on its own.
        let answer = "Revenue rose [q3.pdf] and costs fell [notes.md#Costs], but \
                      the outlook is [strategy-2030.docx].";
        assert_eq!(unresolved(answer, &available), names(&["strategy-2030.docx"]));
        // Case is not the point.
        assert!(unresolved("See [Q3.PDF].", &available).is_empty());
        // A part of a source is still that source.
        assert!(unresolved("See [q3.pdf#Revenue].", &available).is_empty());
        // And an answer with no citations at all is not an offender.
        assert!(unresolved("Nothing here.", &available).is_empty());
    }

    #[test]
    fn prose_and_links_are_not_mistaken_for_citations() {
        let available = names(&["q3.pdf"]);
        // A link, an edit marker, a footnote number, and a bracketed aside.
        let answer = "See [the release notes](https://example.com/x), [sic], [1], and [Note].";
        assert!(cited(answer).is_empty(), "got {:?}", cited(answer));
        assert!(unresolved(answer, &available).is_empty());
        // The same answer with a real citation still finds it, once.
        let mixed = "[q3.pdf] and [q3.pdf] and [1] and [link](http://x)";
        assert_eq!(cited(mixed), names(&["q3.pdf"]));
    }
}
