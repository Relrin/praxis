use crate::types::{ConversationMemory, ExtractedLine};

/// A stage represents a span of the conversation delimited by stage markers.
///
/// Items are assigned to stages based on which turn range they fall within.
/// The first stage (if no marker appears at turn 0) is a synthetic "Initial" stage.
#[derive(Debug, Clone)]
pub struct Stage {
    pub stage_index: usize,
    pub label: String,
    pub start_turn: usize,
    pub end_turn: usize,
    pub constraints: Vec<ExtractedLine>,
    pub decisions: Vec<ExtractedLine>,
    pub open_questions: Vec<ExtractedLine>,
}

/// Build stages from conversation memory using stage marker boundaries.
///
/// Algorithm:
/// 1. Collect unique stage marker turn indices, sorted ascending.
/// 2. If none exist, create a single "Initial" stage spanning all turns.
/// 3. Otherwise: synthetic "Initial" stage for turns before the first marker
///    (if marker is not at turn 0), then one stage per unique marker boundary.
/// 4. Multiple markers at the same turn are merged — the label lists all files.
/// 5. Last stage extends to `turn_count - 1`.
pub fn build_stages(memory: &ConversationMemory) -> Vec<Stage> {
    let max_turn = if memory.turn_count > 0 {
        memory.turn_count - 1
    } else {
        0
    };

    // Collect unique boundaries: (turn_index, merged label)
    let mut boundaries: Vec<(usize, String)> = Vec::new();
    for marker in &memory.stage_markers {
        if let Some(existing) = boundaries.iter_mut().find(|(t, _)| *t == marker.turn_index) {
            // Merge file into existing label (avoid duplicates)
            if !existing.1.contains(&marker.file) {
                existing.1.push_str(", ");
                existing.1.push_str(&marker.file);
            }
        } else {
            boundaries.push((marker.turn_index, marker.file.clone()));
        }
    }
    boundaries.sort_by_key(|(t, _)| *t);

    let mut stages: Vec<Stage> = Vec::new();

    if boundaries.is_empty() {
        stages.push(build_single_stage(
            0,
            "Initial".to_string(),
            0,
            max_turn,
            memory,
        ));
        return stages;
    }

    let mut stage_idx = 0;

    // Synthetic "Initial" stage if first marker is not at turn 0
    if boundaries[0].0 > 0 {
        stages.push(build_single_stage(
            stage_idx,
            "Initial".to_string(),
            0,
            boundaries[0].0 - 1,
            memory,
        ));
        stage_idx += 1;
    }

    // One stage per boundary
    for (i, (turn, label)) in boundaries.iter().enumerate() {
        let end_turn = if i + 1 < boundaries.len() {
            boundaries[i + 1].0 - 1
        } else {
            max_turn
        };

        stages.push(build_single_stage(
            stage_idx,
            label.clone(),
            *turn,
            end_turn,
            memory,
        ));
        stage_idx += 1;
    }

    stages
}

fn build_single_stage(
    stage_index: usize,
    label: String,
    start_turn: usize,
    end_turn: usize,
    memory: &ConversationMemory,
) -> Stage {
    let in_range = |turn: usize| turn >= start_turn && turn <= end_turn;

    Stage {
        stage_index,
        label,
        start_turn,
        end_turn,
        constraints: memory
            .constraints
            .iter()
            .filter(|l| in_range(l.turn_index))
            .cloned()
            .collect(),
        decisions: memory
            .decisions
            .iter()
            .filter(|l| in_range(l.turn_index))
            .cloned()
            .collect(),
        open_questions: memory
            .open_questions
            .iter()
            .filter(|l| in_range(l.turn_index))
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Classification, ExtractedLine, StageMarker};

    fn make_line(text: &str, turn: usize, class: Classification) -> ExtractedLine {
        ExtractedLine::new(text.to_string(), turn, class, 0.8, turn as u64)
    }

    fn make_marker(file: &str, turn: usize) -> StageMarker {
        StageMarker {
            file: file.to_string(),
            turn_index: turn,
            fingerprint: turn as u64,
        }
    }

    #[test]
    fn no_markers_single_initial_stage() {
        let mut mem = ConversationMemory::new(5);
        mem.constraints
            .push(make_line("c1", 0, Classification::Constraint));
        mem.decisions
            .push(make_line("d1", 3, Classification::Decision));

        let stages = build_stages(&mem);
        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].label, "Initial");
        assert_eq!(stages[0].start_turn, 0);
        assert_eq!(stages[0].end_turn, 4);
        assert_eq!(stages[0].constraints.len(), 1);
        assert_eq!(stages[0].decisions.len(), 1);
    }

    #[test]
    fn single_marker_at_turn_zero() {
        let mut mem = ConversationMemory::new(5);
        mem.stage_markers.push(make_marker("src/auth.rs", 0));
        mem.constraints
            .push(make_line("c1", 2, Classification::Constraint));

        let stages = build_stages(&mem);
        // No initial stage since marker is at 0
        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].label, "src/auth.rs");
        assert_eq!(stages[0].start_turn, 0);
        assert_eq!(stages[0].end_turn, 4);
        assert_eq!(stages[0].constraints.len(), 1);
    }

    #[test]
    fn single_marker_not_at_zero() {
        let mut mem = ConversationMemory::new(10);
        mem.stage_markers.push(make_marker("src/auth.rs", 3));
        mem.constraints
            .push(make_line("c1", 1, Classification::Constraint));
        mem.decisions
            .push(make_line("d1", 5, Classification::Decision));

        let stages = build_stages(&mem);
        assert_eq!(stages.len(), 2);
        // Initial stage: turns 0-2
        assert_eq!(stages[0].label, "Initial");
        assert_eq!(stages[0].start_turn, 0);
        assert_eq!(stages[0].end_turn, 2);
        assert_eq!(stages[0].constraints.len(), 1);
        // File stage: turns 3-9
        assert_eq!(stages[1].label, "src/auth.rs");
        assert_eq!(stages[1].start_turn, 3);
        assert_eq!(stages[1].end_turn, 9);
        assert_eq!(stages[1].decisions.len(), 1);
    }

    #[test]
    fn multiple_markers_different_turns() {
        let mut mem = ConversationMemory::new(12);
        mem.stage_markers.push(make_marker("src/auth.rs", 2));
        mem.stage_markers.push(make_marker("src/cache.rs", 7));
        mem.constraints
            .push(make_line("c1", 0, Classification::Constraint));
        mem.decisions
            .push(make_line("d1", 4, Classification::Decision));
        mem.decisions
            .push(make_line("d2", 9, Classification::Decision));

        let stages = build_stages(&mem);
        assert_eq!(stages.len(), 3);
        // Initial: 0-1
        assert_eq!(stages[0].label, "Initial");
        assert_eq!(stages[0].constraints.len(), 1);
        // auth: 2-6
        assert_eq!(stages[1].label, "src/auth.rs");
        assert_eq!(stages[1].decisions.len(), 1);
        // cache: 7-11
        assert_eq!(stages[2].label, "src/cache.rs");
        assert_eq!(stages[2].decisions.len(), 1);
    }

    #[test]
    fn markers_at_same_turn_merged() {
        let mut mem = ConversationMemory::new(5);
        mem.stage_markers.push(make_marker("src/auth.rs", 2));
        mem.stage_markers.push(make_marker("src/token.rs", 2));

        let stages = build_stages(&mem);
        // Initial + one merged stage
        assert_eq!(stages.len(), 2);
        assert!(stages[1].label.contains("src/auth.rs"));
        assert!(stages[1].label.contains("src/token.rs"));
    }

    #[test]
    fn empty_memory() {
        let mem = ConversationMemory::new(0);
        let stages = build_stages(&mem);
        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].label, "Initial");
        assert_eq!(stages[0].start_turn, 0);
        assert_eq!(stages[0].end_turn, 0);
    }

    #[test]
    fn empty_stages_still_appear() {
        let mut mem = ConversationMemory::new(10);
        mem.stage_markers.push(make_marker("src/auth.rs", 0));
        mem.stage_markers.push(make_marker("src/cache.rs", 5));
        // All items in the second stage's range
        mem.constraints
            .push(make_line("c1", 7, Classification::Constraint));

        let stages = build_stages(&mem);
        assert_eq!(stages.len(), 2);
        // First stage: empty
        assert_eq!(stages[0].constraints.len(), 0);
        assert_eq!(stages[0].decisions.len(), 0);
        // Second stage: has the constraint
        assert_eq!(stages[1].constraints.len(), 1);
    }
}
