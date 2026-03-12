use std::path::{Path, PathBuf};
use std::process::Command;

/// Workspace root (parent of the `praxis` crate directory).
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("praxis crate should be inside a workspace")
        .to_path_buf()
}

fn fixtures_dir() -> PathBuf {
    workspace_root().join("tests").join("fixtures")
}

fn praxis_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_praxis"))
}

fn run_praxis(args: &[&str]) -> String {
    let output = Command::new(praxis_bin())
        .args(args)
        .output()
        .expect("failed to execute praxis");

    if !output.status.success() {
        panic!(
            "praxis {} failed (exit {}):\nstdout: {}\nstderr: {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    String::from_utf8(output.stdout).expect("stdout is not valid UTF-8")
}

#[allow(dead_code)]
fn run_praxis_json(args: &[&str]) -> serde_json::Value {
    let output = run_praxis(args);
    serde_json::from_str(&output).unwrap_or_else(|e| {
        panic!(
            "Failed to parse JSON from praxis {} output:\n{}\nError: {}",
            args.join(" "),
            &output[..output.len().min(500)],
            e
        )
    })
}

fn run_praxis_result(args: &[&str]) -> Result<String, String> {
    let output = Command::new(praxis_bin())
        .args(args)
        .output()
        .expect("failed to execute praxis");

    if output.status.success() {
        Ok(String::from_utf8(output.stdout).unwrap())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("praxis_test_{}", name))
}

fn setup_diff_fixture() -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("praxis_test_diff_repo_{}", id));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();

    // git init
    run_git(&dir, &["init"]);

    // Commit 1: baseline
    std::fs::write(
        dir.join("src/auth.rs"),
        "pub fn verify_token(token: &str) -> bool {\n    token.len() > 10\n}\n\npub fn check_session(session_id: &str) -> bool {\n    !session_id.is_empty()\n}\n",
    ).unwrap();
    std::fs::write(
        dir.join("src/middleware.rs"),
        "use crate::auth;\n\npub fn auth_middleware(req: Request) -> Response {\n    if auth::verify_token(&req.token) {\n        handle(req)\n    } else {\n        Response::unauthorized()\n    }\n}\n",
    ).unwrap();
    std::fs::write(
        dir.join("src/handler.rs"),
        "use crate::auth;\n\npub fn login_handler(req: Request) -> Response {\n    let valid = auth::verify_token(&req.token);\n    if valid {\n        Response::ok()\n    } else {\n        Response::forbidden()\n    }\n}\n",
    ).unwrap();

    run_git(&dir, &["add", "-A"]);
    run_git(&dir, &["commit", "-m", "Initial commit"]);

    // Commit 2: feature/auth changes
    std::fs::write(
        dir.join("src/auth.rs"),
        "pub fn verify_token(token: &str, ctx: &AuthCtx) -> Result<Claims> {\n    ctx.validate(token)\n}\n\npub fn refresh_token(token: &str) -> Result<String> {\n    Ok(format!(\"refreshed_{}\", token))\n}\n",
    ).unwrap();
    std::fs::write(
        dir.join("src/middleware.rs"),
        "use crate::auth;\n\npub async fn auth_middleware(req: Request) -> Result<Response> {\n    let claims = auth::verify_token(&req.token, &req.ctx)?;\n    Ok(handle(req, claims))\n}\n",
    ).unwrap();
    std::fs::write(
        dir.join("src/token.rs"),
        "pub struct Claims {\n    pub sub: String,\n    pub exp: u64,\n}\n\npub struct AuthCtx {\n    pub secret: String,\n}\n\nimpl AuthCtx {\n    pub fn validate(&self, token: &str) -> Result<Claims> {\n        Ok(Claims { sub: \"user\".into(), exp: 0 })\n    }\n}\n",
    ).unwrap();

    run_git(&dir, &["add", "-A"]);
    run_git(&dir, &["commit", "-m", "Add OAuth2 auth with JWT tokens"]);

    dir
}

fn run_git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run git");
    assert!(
        output.status.success(),
        "git {} failed:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr),
    );
}

// =========================================================================
// Summarize tests
// =========================================================================

#[test]
fn test_summarize_flat_json_deterministic() {
    let input = fixtures_dir().join("conversation.md");
    let input_str = input.to_str().unwrap();

    let output_1 = run_praxis(&["summarize", "--input", input_str, "--mode", "flat", "--format", "json"]);
    let output_2 = run_praxis(&["summarize", "--input", input_str, "--mode", "flat", "--format", "json"]);

    assert_eq!(output_1, output_2, "Determinism: two runs must produce identical output");

    let parsed: serde_json::Value = serde_json::from_str(&output_1).unwrap();
    // Flat mode outputs items array
    assert!(
        parsed.get("items").is_some() || parsed.get("constraints").is_some(),
        "Output should contain extracted items"
    );
}

#[test]
fn test_summarize_all_modes() {
    let input = fixtures_dir().join("conversation.md");
    let input_str = input.to_str().unwrap();

    for mode in &["flat", "hierarchical", "decision-focused"] {
        for format in &["json", "markdown"] {
            let output = run_praxis(&[
                "summarize",
                "--input", input_str,
                "--mode", mode,
                "--format", format,
            ]);
            assert!(
                !output.is_empty(),
                "Mode {mode} format {format} produced empty output"
            );
        }
    }
}

#[test]
fn test_summarize_since_filter() {
    let input = fixtures_dir().join("conversation.md");
    let input_str = input.to_str().unwrap();

    let full = run_praxis(&["summarize", "--input", input_str, "--mode", "flat", "--format", "json"]);
    let filtered = run_praxis(&[
        "summarize", "--input", input_str,
        "--mode", "flat", "--format", "json",
        "--since", "5",
    ]);

    let full_parsed: serde_json::Value = serde_json::from_str(&full).unwrap();
    let filtered_parsed: serde_json::Value = serde_json::from_str(&filtered).unwrap();

    // The filtered output should have fewer or equal items
    let full_text = full_parsed.to_string();
    let filtered_text = filtered_parsed.to_string();

    // Filtered should be equal or smaller (fewer items)
    assert!(
        filtered_text.len() <= full_text.len(),
        "--since should reduce or maintain output size"
    );
}

// =========================================================================
// Inspect tests
// =========================================================================

#[test]
fn test_inspect_context_bundle() {
    let fixture = fixtures_dir().join("context_8k.json");
    let output = run_praxis(&["inspect", fixture.to_str().unwrap()]);

    assert!(output.contains("Schema"), "Should show schema info");
    assert!(output.contains("Token"), "Should show token budget info");
}

#[test]
fn test_inspect_diff_bundle() {
    let fixture = fixtures_dir().join("diff_main_auth.json");
    let output = run_praxis(&["inspect", fixture.to_str().unwrap()]);

    assert!(output.contains("Schema") || output.contains("schema"), "Should show schema info");
    assert!(output.contains("Stats") || output.contains("stats") || output.contains("files"), "Should show stats");
}

#[test]
fn test_inspect_json_flag() {
    let fixture = fixtures_dir().join("context_8k.json");
    let output = run_praxis(&["inspect", fixture.to_str().unwrap(), "--json"]);

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(parsed.get("schema_version").is_some(), "JSON audit should include schema_version");
}

#[test]
fn test_inspect_invalid_json() {
    // Create a temp file with invalid content
    let path = temp_path("not_a_bundle.txt");
    std::fs::write(&path, "this is not json").unwrap();

    let result = run_praxis_result(&["inspect", path.to_str().unwrap()]);
    assert!(result.is_err(), "inspect should fail on non-JSON input");

    let _ = std::fs::remove_file(&path);
}

// =========================================================================
// Prune tests
// =========================================================================

#[test]
fn test_prune_reduces_inclusion() {
    let fixture = fixtures_dir().join("context_8k.json");
    let output_path = temp_path("pruned_reduce.json");

    run_praxis(&[
        "prune", fixture.to_str().unwrap(),
        "--token-budget", "200",
        "--strict",
        "--output", output_path.to_str().unwrap(),
    ]);

    let pruned_content = std::fs::read_to_string(&output_path).unwrap();
    let pruned: serde_json::Value = serde_json::from_str(&pruned_content).unwrap();

    let files = pruned["relevant_files"].as_array().unwrap();
    let full_count = files
        .iter()
        .filter(|f| f["inclusion_mode"].as_str() == Some("full"))
        .count();

    // With a very small budget, should have fewer full files than original 6
    assert!(
        full_count < 6,
        "Pruning to 200 tokens should reduce full file count, got {full_count}"
    );

    let _ = std::fs::remove_file(&output_path);
}

#[test]
fn test_prune_preserves_scores() {
    let fixture = fixtures_dir().join("context_8k.json");
    let original_content = std::fs::read_to_string(&fixture).unwrap();
    let original: serde_json::Value = serde_json::from_str(&original_content).unwrap();

    let output_path = temp_path("pruned_scores.json");
    run_praxis(&[
        "prune", fixture.to_str().unwrap(),
        "--token-budget", "4000",
        "--output", output_path.to_str().unwrap(),
    ]);

    let pruned_content = std::fs::read_to_string(&output_path).unwrap();
    let pruned: serde_json::Value = serde_json::from_str(&pruned_content).unwrap();

    // Check that scores are preserved for files present in both
    let orig_files = original["relevant_files"].as_array().unwrap();
    let pruned_files = pruned["relevant_files"].as_array().unwrap();

    for orig in orig_files {
        let path = orig["path"].as_str().unwrap();
        if let Some(pruned_file) = pruned_files.iter().find(|f| f["path"].as_str() == Some(path)) {
            assert_eq!(
                orig["relevance_score"], pruned_file["relevance_score"],
                "Relevance score for {path} must be preserved"
            );
        }
    }

    let _ = std::fs::remove_file(&output_path);
}

#[test]
fn test_prune_preserve_files() {
    let fixture = fixtures_dir().join("context_8k.json");
    let output_path = temp_path("pruned_preserve.json");

    run_praxis(&[
        "prune", fixture.to_str().unwrap(),
        "--token-budget", "200",
        "--strict",
        "--preserve-files", "src/auth.rs",
        "--output", output_path.to_str().unwrap(),
    ]);

    let pruned_content = std::fs::read_to_string(&output_path).unwrap();
    let pruned: serde_json::Value = serde_json::from_str(&pruned_content).unwrap();

    let auth_file = pruned["relevant_files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"].as_str() == Some("src/auth.rs"))
        .expect("src/auth.rs should be in pruned output");

    assert_eq!(
        auth_file["inclusion_mode"].as_str(),
        Some("full"),
        "Preserved file must stay at full inclusion"
    );

    let _ = std::fs::remove_file(&output_path);
}

#[test]
fn test_prune_deterministic() {
    let fixture = fixtures_dir().join("context_8k.json");
    let out1 = temp_path("prune_det1.json");
    let out2 = temp_path("prune_det2.json");

    run_praxis(&[
        "prune", fixture.to_str().unwrap(),
        "--token-budget", "4000",
        "--output", out1.to_str().unwrap(),
    ]);
    run_praxis(&[
        "prune", fixture.to_str().unwrap(),
        "--token-budget", "4000",
        "--output", out2.to_str().unwrap(),
    ]);

    let content1 = std::fs::read_to_string(&out1).unwrap();
    let content2 = std::fs::read_to_string(&out2).unwrap();
    assert_eq!(content1, content2, "Two prune runs must produce identical output");

    let _ = std::fs::remove_file(&out1);
    let _ = std::fs::remove_file(&out2);
}

// =========================================================================
// Diff tests (require bash and git)
// =========================================================================

#[test]
fn test_diff_deterministic() {
    let repo_path = setup_diff_fixture();
    let repo_str = repo_path.to_str().unwrap();

    let out1 = temp_path("diff_det1.json");
    let out2 = temp_path("diff_det2.json");

    run_praxis(&["diff", "--repo", repo_str, "--from", "HEAD~1", "--to", "HEAD", "--output", out1.to_str().unwrap()]);
    run_praxis(&["diff", "--repo", repo_str, "--from", "HEAD~1", "--to", "HEAD", "--output", out2.to_str().unwrap()]);

    let content1 = std::fs::read_to_string(&out1).unwrap();
    let content2 = std::fs::read_to_string(&out2).unwrap();
    assert_eq!(content1, content2, "Two diff runs must produce identical output");

    let _ = std::fs::remove_file(&out1);
    let _ = std::fs::remove_file(&out2);
}

#[test]
fn test_diff_change_kinds() {
    let repo_path = setup_diff_fixture();
    let out = temp_path("diff_kinds.json");

    run_praxis(&[
        "diff", "--repo", repo_path.to_str().unwrap(),
        "--from", "HEAD~1", "--to", "HEAD",
        "--output", out.to_str().unwrap(),
    ]);

    let content = std::fs::read_to_string(&out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

    let files = parsed["changed_files"].as_array().unwrap();
    let kinds: Vec<String> = files
        .iter()
        .map(|f| {
            // ChangeKind is internally tagged: {"type": "modified"} or {"type": "added"}
            f["kind"]["type"]
                .as_str()
                .unwrap_or("unknown")
                .to_string()
        })
        .collect();

    assert!(kinds.iter().any(|k| k == "added"), "Should detect added file (src/token.rs), got: {:?}", kinds);
    assert!(kinds.iter().any(|k| k == "modified"), "Should detect modified file (src/auth.rs), got: {:?}", kinds);

    let _ = std::fs::remove_file(&out);
}

#[test]
fn test_diff_symbol_changes() {
    let repo_path = setup_diff_fixture();
    let out = temp_path("diff_symbols.json");

    run_praxis(&[
        "diff", "--repo", repo_path.to_str().unwrap(),
        "--from", "HEAD~1", "--to", "HEAD",
        "--output", out.to_str().unwrap(),
    ]);

    let content = std::fs::read_to_string(&out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

    let symbols = parsed["symbol_changes"].as_array().unwrap();
    assert!(
        !symbols.is_empty(),
        "Should detect symbol changes in the diff"
    );

    let _ = std::fs::remove_file(&out);
}

// =========================================================================
// Phase 1 regression
// =========================================================================

#[test]
fn test_phase1_build_without_conversation() {
    // Build without --conversation should still work and produce no conversation_memory
    let repo_path = setup_diff_fixture();
    let out = temp_path("phase1_build.json");

    run_praxis(&[
        "build",
        "--task", "test task",
        "--repo", repo_path.to_str().unwrap(),
        "--output", out.to_str().unwrap(),
        "--format", "json",
    ]);

    let content = std::fs::read_to_string(&out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert!(parsed.get("schema_version").is_some(), "Should have schema_version");
    assert!(parsed.get("task").is_some(), "Should have task");
    assert!(parsed.get("relevant_files").is_some(), "Should have relevant_files");
    // conversation_memory should be absent (not null)
    assert!(
        parsed.get("conversation_memory").is_none(),
        "Without --conversation, conversation_memory should be absent from JSON"
    );

    let _ = std::fs::remove_file(&out);
}

// =========================================================================
// End-to-end pipeline
// =========================================================================

#[test]
fn test_end_to_end_pipeline() {
    let conv_fixture = fixtures_dir().join("conversation.md");
    let context_fixture = fixtures_dir().join("context_8k.json");

    // 1. Summarize the conversation
    let summary = run_praxis(&[
        "summarize",
        "--input", conv_fixture.to_str().unwrap(),
        "--mode", "decision-focused",
        "--format", "json",
    ]);
    assert!(!summary.is_empty(), "Summarize should produce output");

    // 2. Inspect a context bundle
    let inspect_output = run_praxis(&["inspect", context_fixture.to_str().unwrap()]);
    assert!(!inspect_output.is_empty(), "Inspect should produce output");

    // 3. Prune the bundle
    let pruned = temp_path("e2e_pruned.json");
    run_praxis(&[
        "prune", context_fixture.to_str().unwrap(),
        "--token-budget", "4000",
        "--output", pruned.to_str().unwrap(),
    ]);

    // 4. Inspect the pruned bundle
    let pruned_inspect = run_praxis(&["inspect", pruned.to_str().unwrap()]);
    assert!(!pruned_inspect.is_empty(), "Inspect of pruned bundle should produce output");

    let _ = std::fs::remove_file(&pruned);
}
