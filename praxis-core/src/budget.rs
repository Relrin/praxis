/// Configuration for the token budget allocation.
#[derive(Debug, Clone)]
pub struct BudgetConfig {
    pub total_budget: usize,
    pub buffer_pct: f64,
    pub strict: bool,
}

impl BudgetConfig {
    /// Creates a new [`BudgetConfig`] with the given budget and default settings.
    ///
    /// Defaults to 10% buffer in soft mode.
    pub fn new(total_budget: usize) -> Self {
        Self {
            total_budget,
            buffer_pct: 0.10,
            strict: false,
        }
    }

    /// Sets strict mode (hard cap at `total_budget`, no buffer).
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// Sets the buffer percentage for soft mode.
    pub fn with_buffer_pct(mut self, buffer_pct: f64) -> Self {
        self.buffer_pct = buffer_pct;
        self
    }
}

/// Token budget breakdown used across all bundle types.
///
/// For context bundles, all bucket fields are populated.
/// For diff bundles, bucket fields default to 0.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenBudget {
    pub declared: usize,
    pub effective: usize,
    #[serde(default)]
    pub task: usize,
    #[serde(default)]
    pub repo_summary: usize,
    #[serde(default)]
    pub memory: usize,
    #[serde(default)]
    pub safety: usize,
    #[serde(default)]
    pub code: usize,
    pub strict: bool,
    #[serde(default)]
    pub overflow: bool,
}

/// Allocates the token budget across fixed reserves and the code bucket.
///
/// Reserve allocation order:
/// 1. task — actual token count of the task string
/// 2. repo_summary — `max(300, effective * 0.05)`
/// 3. memory — `min(effective * 0.20, remaining)`
/// 4. safety — `effective * 0.05` (skipped in strict mode)
/// 5. code — whatever remains (minimum 0)
///
/// If reserves exceed the effective budget, `code` is set to 0 and
/// `overflow` is flagged. A zero-code bundle is valid output.
pub fn allocate_budget(config: &BudgetConfig, task_text: &str) -> TokenBudget {
    let declared = config.total_budget;

    let effective = if config.strict {
        declared
    } else {
        (declared as f64 * (1.0 + config.buffer_pct)) as usize
    };

    let task = task_text.len() / 4;

    let repo_summary_min = 300;
    let repo_summary_pct = (effective as f64 * 0.05) as usize;
    let repo_summary = if repo_summary_min > repo_summary_pct {
        repo_summary_min
    } else {
        repo_summary_pct
    };

    let used_after_summary = task + repo_summary;

    let remaining_for_memory = effective.saturating_sub(used_after_summary);
    let memory_pct = (effective as f64 * 0.20) as usize;
    let memory = if memory_pct < remaining_for_memory {
        memory_pct
    } else {
        remaining_for_memory
    };

    let used_after_memory = used_after_summary + memory;

    let safety = if config.strict {
        0
    } else {
        (effective as f64 * 0.05) as usize
    };

    let used_after_safety = used_after_memory + safety;

    let code = effective.saturating_sub(used_after_safety);
    let overflow = used_after_safety >= effective;

    TokenBudget {
        declared,
        effective,
        task,
        repo_summary,
        memory,
        safety,
        code,
        strict: config.strict,
        overflow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_mode_applies_buffer() {
        let config = BudgetConfig::new(8000);
        let breakdown = allocate_budget(&config, "fix the parser");

        assert_eq!(breakdown.declared, 8000);
        assert_eq!(breakdown.effective, 8800);
        assert!(!breakdown.strict);
    }

    #[test]
    fn strict_mode_no_buffer() {
        let config = BudgetConfig::new(8000).with_strict(true);
        let breakdown = allocate_budget(&config, "fix the parser");

        assert_eq!(breakdown.declared, 8000);
        assert_eq!(breakdown.effective, 8000);
        assert!(breakdown.strict);
        assert_eq!(breakdown.safety, 0);
    }

    #[test]
    fn task_tokens_estimated_from_length() {
        let task = "a]".repeat(100);
        let config = BudgetConfig::new(8000);
        let breakdown = allocate_budget(&config, &task);

        assert_eq!(breakdown.task, task.len() / 4);
    }

    #[test]
    fn repo_summary_minimum_300() {
        let config = BudgetConfig::new(1000);
        let breakdown = allocate_budget(&config, "task");

        assert!(breakdown.repo_summary >= 300);
    }

    #[test]
    fn repo_summary_scales_with_budget() {
        let config = BudgetConfig::new(100_000);
        let breakdown = allocate_budget(&config, "task");

        let expected_pct = (breakdown.effective as f64 * 0.05) as usize;
        assert_eq!(breakdown.repo_summary, expected_pct);
    }

    #[test]
    fn code_bucket_gets_remainder() {
        let config = BudgetConfig::new(8000);
        let breakdown = allocate_budget(&config, "fix");

        let reserved = breakdown.task + breakdown.repo_summary + breakdown.memory + breakdown.safety;
        assert_eq!(breakdown.code, breakdown.effective - reserved);
    }

    #[test]
    fn overflow_when_reserves_exceed_budget() {
        let huge_task = "x".repeat(40_000);
        let config = BudgetConfig::new(1000);
        let breakdown = allocate_budget(&config, &huge_task);

        assert!(breakdown.overflow);
        assert_eq!(breakdown.code, 0);
    }

    #[test]
    fn no_overflow_with_normal_budget() {
        let config = BudgetConfig::new(8000);
        let breakdown = allocate_budget(&config, "fix the parser bug");

        assert!(!breakdown.overflow);
        assert!(breakdown.code > 0);
    }

    #[test]
    fn all_buckets_sum_to_effective_or_less() {
        let config = BudgetConfig::new(8000);
        let breakdown = allocate_budget(&config, "refactor the scoring engine");

        let total = breakdown.task
            + breakdown.repo_summary
            + breakdown.memory
            + breakdown.safety
            + breakdown.code;
        assert_eq!(total, breakdown.effective);
    }

    #[test]
    fn custom_buffer_pct() {
        let config = BudgetConfig::new(10_000).with_buffer_pct(0.25);
        let breakdown = allocate_budget(&config, "task");

        assert_eq!(breakdown.effective, 12_500);
    }
}