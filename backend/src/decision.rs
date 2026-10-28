//! Pure decision logic: Jev probabilities + configuration -> what to do.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Rules that change where a message lives or how it looks never fire below this.
pub const MIN_ACTION_PROBABILITY: f64 = 0.9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    Archive,
    MarkRead,
    MoveToFolder,
    Star,
}

impl RuleAction {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "archive" => Some(Self::Archive),
            "mark_read" => Some(Self::MarkRead),
            "move_to_folder" => Some(Self::MoveToFolder),
            "star" => Some(Self::Star),
            _ => None,
        }
    }

    /// Moving a message can change its id, so these run last and at most once.
    pub fn relocates(self) -> bool {
        matches!(self, Self::Archive | Self::MoveToFolder)
    }
}

#[derive(Debug, Clone)]
pub struct LabelInfo {
    pub id: i64,
    pub name: String,
    pub threshold: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct RuleInfo {
    pub id: i64,
    pub label_id: i64,
    pub action: RuleAction,
    pub folder: Option<String>,
    pub min_probability: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedAction {
    pub rule_id: i64,
    pub action: RuleAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    /// Label to apply: the top label, or Review when below its threshold.
    pub label_id: i64,
    pub label_name: String,
    pub is_review: bool,
    pub top_label: Option<String>,
    pub top_probability: f64,
    pub threshold: f64,
    pub actions: Vec<PlannedAction>,
}

fn probability_of(probs: &BTreeMap<String, f64>, name: &str) -> f64 {
    probs
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, p)| if p.is_finite() { *p } else { 0.0 })
        .unwrap_or(0.0)
}

/// `labels` are the enabled, non-system labels in display order; ties go to the earlier one.
pub fn decide(
    probs: &BTreeMap<String, f64>,
    labels: &[LabelInfo],
    review: &LabelInfo,
    global_threshold: f64,
    rules: &[RuleInfo],
) -> Outcome {
    let mut best: Option<(&LabelInfo, f64)> = None;
    for label in labels {
        let p = probability_of(probs, &label.name);
        if best.is_none_or(|(_, bp)| p > bp) {
            best = Some((label, p));
        }
    }

    let review_outcome = |top: Option<&LabelInfo>, p: f64, threshold: f64| Outcome {
        label_id: review.id,
        label_name: review.name.clone(),
        is_review: true,
        top_label: top.map(|l| l.name.clone()),
        top_probability: p,
        threshold,
        actions: Vec::new(),
    };

    let Some((top, p)) = best else {
        return review_outcome(None, 0.0, global_threshold);
    };
    let threshold = top.threshold.unwrap_or(global_threshold);
    if p < threshold {
        return review_outcome(Some(top), p, threshold);
    }

    let mut matching: Vec<&RuleInfo> = rules
        .iter()
        .filter(|r| r.label_id == top.id && p >= r.min_probability.max(MIN_ACTION_PROBABILITY))
        .filter(|r| r.action != RuleAction::MoveToFolder || r.folder.as_deref().is_some_and(|f| !f.trim().is_empty()))
        .collect();
    matching.sort_by_key(|r| (r.action.relocates(), r.id));

    let mut actions = Vec::new();
    let mut relocated = false;
    for rule in matching {
        if rule.action.relocates() {
            if relocated {
                continue;
            }
            relocated = true;
        }
        if actions.iter().any(|a: &PlannedAction| a.action == rule.action && !rule.action.relocates()) {
            continue;
        }
        actions.push(PlannedAction { rule_id: rule.id, action: rule.action, folder: rule.folder.clone() });
    }

    Outcome {
        label_id: top.id,
        label_name: top.name.clone(),
        is_review: false,
        top_label: Some(top.name.clone()),
        top_probability: p,
        threshold,
        actions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probs(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn labels() -> Vec<LabelInfo> {
        vec![
            LabelInfo { id: 1, name: "Needs reply".into(), threshold: None },
            LabelInfo { id: 2, name: "Promos".into(), threshold: Some(0.5) },
            LabelInfo { id: 3, name: "Spam".into(), threshold: Some(0.95) },
        ]
    }

    fn review() -> LabelInfo {
        LabelInfo { id: 99, name: "Review".into(), threshold: None }
    }

    fn rule(id: i64, label_id: i64, action: RuleAction, min: f64) -> RuleInfo {
        RuleInfo { id, label_id, action, folder: None, min_probability: min }
    }

    #[test]
    fn applies_top_label_at_or_above_global_threshold() {
        let o = decide(&probs(&[("Needs reply", 0.7), ("Promos", 0.3)]), &labels(), &review(), 0.7, &[]);
        assert_eq!(o.label_id, 1);
        assert!(!o.is_review);
    }

    #[test]
    fn falls_back_to_review_below_threshold() {
        let o = decide(&probs(&[("Needs reply", 0.69), ("Promos", 0.31)]), &labels(), &review(), 0.7, &[]);
        assert!(o.is_review);
        assert_eq!(o.label_id, 99);
        assert_eq!(o.top_label.as_deref(), Some("Needs reply"));
        assert!(o.actions.is_empty());
    }

    #[test]
    fn per_label_threshold_overrides_global() {
        let low = decide(&probs(&[("Promos", 0.55), ("Spam", 0.45)]), &labels(), &review(), 0.7, &[]);
        assert_eq!(low.label_id, 2);
        let high = decide(&probs(&[("Spam", 0.9), ("Promos", 0.1)]), &labels(), &review(), 0.7, &[]);
        assert!(high.is_review, "Spam needs 0.95");
    }

    #[test]
    fn label_matching_is_case_insensitive_and_unknown_labels_are_ignored() {
        let o = decide(&probs(&[("needs REPLY", 0.8), ("Deleted label", 0.2)]), &labels(), &review(), 0.7, &[]);
        assert_eq!(o.label_id, 1);
    }

    #[test]
    fn no_labels_means_review() {
        let o = decide(&probs(&[("x", 1.0)]), &[], &review(), 0.7, &[]);
        assert!(o.is_review);
        assert_eq!(o.top_label, None);
    }

    #[test]
    fn rules_need_at_least_point_nine_even_if_configured_lower() {
        let rules = [rule(1, 2, RuleAction::Archive, 0.5)];
        let o = decide(&probs(&[("Promos", 0.85)]), &labels(), &review(), 0.7, &rules);
        assert!(!o.is_review);
        assert!(o.actions.is_empty());
        let o = decide(&probs(&[("Promos", 0.9)]), &labels(), &review(), 0.7, &rules);
        assert_eq!(o.actions.len(), 1);
    }

    #[test]
    fn rules_respect_their_own_higher_minimum() {
        let rules = [rule(1, 2, RuleAction::MarkRead, 0.97)];
        assert!(decide(&probs(&[("Promos", 0.95)]), &labels(), &review(), 0.7, &rules).actions.is_empty());
        assert_eq!(decide(&probs(&[("Promos", 0.98)]), &labels(), &review(), 0.7, &rules).actions.len(), 1);
    }

    #[test]
    fn only_rules_for_the_chosen_label_fire() {
        let rules = [rule(1, 1, RuleAction::Star, 0.9), rule(2, 2, RuleAction::Archive, 0.9)];
        let o = decide(&probs(&[("Needs reply", 0.99)]), &labels(), &review(), 0.7, &rules);
        assert_eq!(o.actions, vec![PlannedAction { rule_id: 1, action: RuleAction::Star, folder: None }]);
    }

    #[test]
    fn relocating_actions_run_last_and_only_once() {
        let mut mv = rule(3, 2, RuleAction::MoveToFolder, 0.9);
        mv.folder = Some("Deals".into());
        let rules = [rule(1, 2, RuleAction::Archive, 0.9), rule(2, 2, RuleAction::MarkRead, 0.9), mv];
        let o = decide(&probs(&[("Promos", 0.99)]), &labels(), &review(), 0.7, &rules);
        let kinds: Vec<_> = o.actions.iter().map(|a| a.action).collect();
        assert_eq!(kinds, vec![RuleAction::MarkRead, RuleAction::Archive]);
    }

    #[test]
    fn move_without_folder_is_skipped() {
        let rules = [rule(1, 2, RuleAction::MoveToFolder, 0.9)];
        assert!(decide(&probs(&[("Promos", 0.99)]), &labels(), &review(), 0.7, &rules).actions.is_empty());
    }
}
