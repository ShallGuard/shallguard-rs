const ADVISORY_WORKFLOW: &str = include_str!("../../.github/workflows/shallguard-review.yml");
const REQUIRED_WORKFLOW: &str = include_str!("../../.github/workflows/rust.yml");

#[shallguard::verifies("REQ-PORT-009")]
#[test]
fn advisory_review_is_isolated_from_the_required_deterministic_gate() {
    let prepare_start = ADVISORY_WORKFLOW
        .find("  prepare:")
        .expect("advisory workflow has a preparation job");
    let review_start = ADVISORY_WORKFLOW
        .find("  review:")
        .expect("advisory workflow has a review job");
    let publish_start = ADVISORY_WORKFLOW
        .find("  publish:")
        .expect("advisory workflow has a publisher job");
    let prepare = &ADVISORY_WORKFLOW[prepare_start..review_start];
    let review = &ADVISORY_WORKFLOW[review_start..publish_start];
    let publish = &ADVISORY_WORKFLOW[publish_start..];

    assert!(ADVISORY_WORKFLOW.contains("pull_request_target:"));
    assert!(prepare.contains("permissions:\n      contents: read"));
    assert!(prepare.contains("ref: ${{ github.event.pull_request.base.sha }}"));
    assert!(prepare.contains("ref: refs/pull/${{ github.event.pull_request.number }}/merge"));
    assert!(prepare.contains("TRUSTED_SHALLGUARD:"));
    assert!(prepare.contains("\"$TRUSTED_SHALLGUARD\" impact"));
    assert!(prepare.contains("\"$TRUSTED_SHALLGUARD\" bundle"));
    assert!(!prepare.contains("secrets."));
    assert!(!prepare.contains("COPILOT_GITHUB_TOKEN"));

    let same_repository_guard =
        "github.event.pull_request.head.repo.full_name == github.repository";
    assert!(review.contains(same_repository_guard));
    assert!(review.contains("ref: ${{ github.event.pull_request.base.sha }}"));
    assert!(review.contains("copilot-requests: write"));
    assert!(
        review
            .contains("COPILOT_GITHUB_TOKEN: ${{ secrets.COPILOT_GITHUB_TOKEN || github.token }}")
    );
    assert!(review.contains("--provider copilot"));
    assert!(review.contains("--format markdown"));
    assert!(review.contains("exit 0"));

    assert!(publish.contains(same_repository_guard));
    assert!(publish.contains("issues: write"));
    assert!(publish.contains("issues.updateComment"));
    assert!(publish.contains("issues.createComment"));

    assert!(REQUIRED_WORKFLOW.contains("cargo shallguard-dev fmt --check"));
    assert!(REQUIRED_WORKFLOW.contains("cargo shallguard-dev check"));
    assert!(!REQUIRED_WORKFLOW.contains("COPILOT_GITHUB_TOKEN"));
}
