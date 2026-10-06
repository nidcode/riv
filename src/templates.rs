//! Spec templates, embedded from `spec-templates/` (single source for `riv init` and the repository).

pub const REQUIREMENTS: &str = include_str!("../spec-templates/reinvent-2026/requirements.md");
pub const DESIGN: &str = include_str!("../spec-templates/reinvent-2026/design.md");
pub const TASKS: &str = include_str!("../spec-templates/reinvent-2026/tasks.md");
pub const REQUIREMENTS_JA: &str = include_str!("../spec-templates/reinvent-2026/requirements.ja.md");
pub const DESIGN_JA: &str = include_str!("../spec-templates/reinvent-2026/design.ja.md");
pub const TASKS_JA: &str = include_str!("../spec-templates/reinvent-2026/tasks.ja.md");

/// (requirements, design, tasks) for a language.
pub fn for_lang(lang: crate::i18n::Lang) -> (&'static str, &'static str, &'static str) {
    match lang {
        crate::i18n::Lang::En => (REQUIREMENTS, DESIGN, TASKS),
        crate::i18n::Lang::Ja => (REQUIREMENTS_JA, DESIGN_JA, TASKS_JA),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both language variants must declare the same (empty) desired state.
    #[test]
    fn language_variants_share_the_same_desired_state() {
        let en = crate::desired::parse_markdown(DESIGN).expect("en");
        let ja = crate::desired::parse_markdown(DESIGN_JA).expect("ja");
        assert_eq!(crate::desired::desired_hash(&en), crate::desired::desired_hash(&ja));
    }
}
