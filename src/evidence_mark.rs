//! Evidence marks on the *Verified:* line of a requirement.
//!
//! The canonical form of a mark is an ASCII keyword in square brackets,
//! for example `[test]`. The keyword survives editors, copy and paste,
//! diff tools, and screen readers, and `grep` finds it. The four emoji
//! that older documents use are accepted as aliases of the same classes.
//! `cargo shallguard fmt` rewrites an alias to its keyword.

/// One evidence class that a *Verified:* segment can claim.
#[shallguard::enforces("REQ-SPEC-007")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceMark {
    /// An anchored automated test backs the requirement.
    Test,
    /// An end-to-end or production validation backs the requirement.
    EndToEnd,
    /// Only a code review backs the requirement.
    ReviewOnly,
    /// Evidence is pending.
    Pending,
}

/// The Unicode variation selector that some editors append to an emoji.
const VARIATION_SELECTOR: char = '\u{FE0F}';

impl EvidenceMark {
    /// Every mark, in the order the check report uses.
    pub const ALL: [EvidenceMark; 4] = [
        EvidenceMark::Test,
        EvidenceMark::EndToEnd,
        EvidenceMark::ReviewOnly,
        EvidenceMark::Pending,
    ];

    /// The canonical ASCII keyword of the mark.
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            EvidenceMark::Test => "[test]",
            EvidenceMark::EndToEnd => "[e2e]",
            EvidenceMark::ReviewOnly => "[review]",
            EvidenceMark::Pending => "[pending]",
        }
    }

    /// The emoji that older documents use for the mark.
    #[must_use]
    pub const fn emoji(self) -> char {
        match self {
            EvidenceMark::Test => '\u{2705}',        // ✅
            EvidenceMark::EndToEnd => '\u{1F52C}',   // 🔬
            EvidenceMark::ReviewOnly => '\u{1F441}', // 👁
            EvidenceMark::Pending => '\u{23F3}',     // ⏳
        }
    }

    /// A short description of the evidence class for people.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            EvidenceMark::Test => "automated test",
            EvidenceMark::EndToEnd => "end-to-end validation",
            EvidenceMark::ReviewOnly => "code review only",
            EvidenceMark::Pending => "pending",
        }
    }

    /// Returns whether the text claims the mark, in the keyword form or in
    /// the emoji form.
    #[must_use]
    pub fn is_claimed_in(self, text: &str) -> bool {
        text.contains(self.keyword()) || text.contains(self.emoji())
    }

    /// Returns whether the text claims at least one mark in any form.
    #[must_use]
    pub fn any_claimed_in(text: &str) -> bool {
        EvidenceMark::ALL
            .iter()
            .any(|mark| mark.is_claimed_in(text))
    }

    /// Rewrites every emoji alias in the text to its canonical keyword.
    ///
    /// A variation selector after an emoji is removed with the emoji.
    #[must_use]
    pub fn canonicalize(text: &str) -> String {
        let mut canonical = text.to_string();
        for mark in EvidenceMark::ALL {
            let with_selector = format!("{}{VARIATION_SELECTOR}", mark.emoji());
            canonical = canonical.replace(&with_selector, mark.keyword());
            canonical = canonical.replace(mark.emoji(), mark.keyword());
        }
        canonical
    }

    /// The keywords of every mark, joined for a diagnostic message.
    #[must_use]
    pub fn keyword_list() -> String {
        let keywords: Vec<&str> = EvidenceMark::ALL
            .iter()
            .map(|mark| mark.keyword())
            .collect();
        keywords.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[shallguard::verifies("REQ-SPEC-007")]
    #[test]
    fn recognizes_keywords_and_emoji_aliases() {
        for mark in EvidenceMark::ALL {
            let keyword_text = format!("{} `src/lib.rs` (`t`)", mark.keyword());
            let emoji_text = format!("{} `src/lib.rs` (`t`)", mark.emoji());
            assert!(mark.is_claimed_in(&keyword_text), "{mark:?} keyword");
            assert!(mark.is_claimed_in(&emoji_text), "{mark:?} emoji");
            assert_eq!(
                EvidenceMark::canonicalize(&emoji_text),
                keyword_text,
                "{mark:?} canonical form"
            );
        }
        assert!(!EvidenceMark::any_claimed_in("code review"));
        assert!(!EvidenceMark::Test.is_claimed_in("[e2e] differential"));
    }

    #[shallguard::verifies("REQ-SPEC-007")]
    #[test]
    fn canonicalize_removes_variation_selectors_and_keeps_keywords() {
        let text = "\u{1F441}\u{FE0F} code review only";
        assert_eq!(
            EvidenceMark::canonicalize(text),
            "[review] code review only"
        );
        let already = "[test] `src/lib.rs` (`t`) [pending]";
        assert_eq!(EvidenceMark::canonicalize(already), already);
        assert_eq!(
            EvidenceMark::keyword_list(),
            "[test], [e2e], [review], [pending]"
        );
    }
}
