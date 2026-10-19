use crate::refresh::{FailureCategory, RefreshDiagnostic, RefreshStage};
use googletest::{assert_that, matchers::eq};
use std::{
    os::unix::process::ExitStatusExt,
    process::{ExitStatus, Output},
};

#[googletest::test]
fn git_failure_diagnostics_exclude_untrusted_output_and_retain_stage_and_status() {
    for (stage, category, source, destination) in [
        (
            RefreshStage::RepositoryValidation,
            FailureCategory::RepositoryUnavailable,
            None,
            None,
        ),
        (
            RefreshStage::Fetch,
            FailureCategory::GitFailed,
            Some("refs/heads/main"),
            Some("refs/remotes/origin/main"),
        ),
        (
            RefreshStage::ResolveCommit,
            FailureCategory::GitFailed,
            Some("refs/remotes/origin/main"),
            None,
        ),
    ] {
        let output = Output {
            status: ExitStatus::from_raw(128 << 8),
            stdout: b"https://user:secret@example.invalid/private".to_vec(),
            stderr: b"fatal: secret-token https://user:password@example.invalid/repo".to_vec(),
        };
        let diagnostic = RefreshDiagnostic::git_failure(stage, &output);
        assert_that!(diagnostic.category, eq(category));
        assert_that!(diagnostic.stage, eq(stage));
        assert_that!(diagnostic.git_exit_status, eq(Some(128)));
        let wire = serde_json::to_value(&diagnostic).unwrap();
        assert_that!(wire["source_reference"].as_str(), eq(source));
        assert_that!(wire["destination_reference"].as_str(), eq(destination));
        for excluded in [
            "secret",
            "password",
            "https://",
            "example.invalid",
            "fatal:",
        ] {
            assert_that!(wire.to_string().contains(excluded), eq(false));
        }
        assert_that!(diagnostic.validate().is_ok(), eq(true));
    }
}

#[googletest::test]
fn uncertain_refresh_result_keeps_operation_identity_and_unknown_outcome() {
    let repository_id = crate::management::RepositoryId::new();
    let operation_id = crate::management::OperationId::new();
    let error = crate::error::PoolError::RefreshResult {
        repository_id,
        operation_id,
        source: Box::new(crate::error::PoolError::CommitUnknown),
    };
    let receipt = crate::cli::Envelope::failure("acquire".into(), &error);
    assert_that!(receipt.outcome, eq("unknown"));
    assert_that!(receipt.reason_code, eq("commit_unknown"));
    assert_that!(
        receipt.context["repository_id"].as_str(),
        eq(Some(repository_id.to_string().as_str()))
    );
    assert_that!(
        receipt.context["operation_id"].as_str(),
        eq(Some(operation_id.to_string().as_str()))
    );
}

#[googletest::test]
fn refresh_failure_facts_validate_causes_and_rebuild_without_inventing_history() {
    use crate::{
        domain::{CatalogId, decode_event, management_event, management_event_caused_by},
        management::{
            ManagementEvent, OperationId, RefreshFailed, RefreshStarted, Repository, RepositoryId,
        },
        paths::EncodedPath,
        store::Store,
    };
    for historical in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("catalog.redb");
        let catalog = CatalogId::new();
        let repository = RepositoryId::new();
        let operation_id = OperationId::new();
        Store::create_with_id(&path, catalog).unwrap();
        let store = Store::open(&path, catalog).unwrap();
        store
            .append(
                1,
                management_event(
                    catalog,
                    &ManagementEvent::RepositoryRegistered(Repository {
                        repository_id: repository,
                        common_directory: EncodedPath::from_path(directory.path()),
                        context_path: EncodedPath::from_path(directory.path()),
                        capacity: 4,
                        revision: 0,
                    }),
                )
                .unwrap(),
            )
            .unwrap();
        let started = management_event(
            catalog,
            &ManagementEvent::RefreshStarted(RefreshStarted {
                repository_id: repository,
                operation_id,
            }),
        )
        .unwrap();
        let cause = started["id"].as_str().unwrap().to_owned();
        store.append(0, started).unwrap();
        let diagnostic: RefreshDiagnostic = serde_json::from_value(serde_json::json!({
            "category": "git_failed", "stage": "fetch",
            "source_reference": "refs/heads/main", "destination_reference": "refs/remotes/origin/main",
            "git_exit_status": 128,
            "next_action": "Check origin access, the main branch, and destination reference locks. Inspect this operation before explicit recovery."
        })).unwrap();
        let fact = ManagementEvent::RefreshFailed(RefreshFailed {
            repository_id: repository,
            operation_id,
            diagnostic: diagnostic.clone(),
        });
        let failed = if historical {
            let mut wire = management_event_caused_by(catalog, &fact, &cause).unwrap();
            wire["type"] =
                serde_json::json!("io.lowkeylab.worktreepool.repository.refresh.finished.v1");
            wire["data"] = serde_json::json!({"kind": "refresh_finished", "record": {"repository_id": repository, "operation_id": operation_id, "resolved_commit": null}});
            wire
        } else {
            management_event_caused_by(catalog, &fact, &cause).unwrap()
        };
        let before = store.projection().unwrap();
        for mutation in [
            "cause",
            "subject",
            "operation",
            "missing-cause",
            "diagnostic",
            "stage",
            "instruction",
            "status",
            "revision",
        ] {
            let mut invalid = failed.clone();
            match mutation {
                "cause" => invalid["causationid"] = serde_json::json!(OperationId::new()),
                "subject" => invalid["subject"] = serde_json::json!(format!("catalogs/{catalog}")),
                "operation" => {
                    let other = OperationId::new();
                    invalid["operationid"] = serde_json::json!(other);
                    invalid["data"]["record"]["operation_id"] = serde_json::json!(other);
                }
                "missing-cause" => {
                    invalid.as_object_mut().unwrap().remove("causationid");
                }
                "revision" => {
                    assert_that!(store.append(0, failed.clone()).is_err(), eq(true));
                    continue;
                }
                "stage" | "instruction" | "status" => {
                    if historical {
                        continue;
                    }
                    let (key, value) = match mutation {
                        "stage" => ("stage", serde_json::json!("resolve_commit")),
                        "instruction" => (
                            "next_action",
                            serde_json::json!(
                                "Check the registered repository. Inspect this operation before explicit recovery."
                            ),
                        ),
                        _ => ("git_exit_status", serde_json::json!(0)),
                    };
                    invalid["data"]["record"]["diagnostic"][key] = value;
                }
                _ => {
                    if historical {
                        continue;
                    }
                    invalid["data"]["record"]["diagnostic"]["source_reference"] =
                        serde_json::json!("https://user:password@example.invalid/private");
                }
            }
            assert_that!(store.append(1, invalid).is_err(), eq(true));
            assert_that!(store.projection().unwrap(), eq(&before));
            assert_that!(store.events().unwrap().len(), eq(3));
        }
        assert_that!(decode_event(&failed, catalog).is_ok(), eq(true));
        let state = store.append(1, failed.clone()).unwrap();
        assert_that!(state.revision, eq(4));
        assert_that!(state.repositories[0].revision, eq(2));
        assert_that!(
            state.operations[0].diagnostic.as_ref(),
            eq(if historical { None } else { Some(&diagnostic) })
        );
        assert_that!(store.append(2, failed).is_err(), eq(true));
        assert_that!(store.projection().unwrap(), eq(&state));
        let history = store.events().unwrap();
        drop(store);
        let rebuilt = Store::rebuild(&path, catalog, |_| {}).unwrap();
        assert_that!(rebuilt, eq(&state));
        let reopened = Store::open(&path, catalog).unwrap();
        assert_that!(reopened.events().unwrap(), eq(&history));
        assert_that!(reopened.projection().unwrap(), eq(&state));
    }
}

#[googletest::test]
fn rejected_refresh_intent_does_not_report_an_unrecorded_operation() {
    let repository_id = crate::management::RepositoryId::new();
    let operation_id = crate::management::OperationId::new();
    for source in [
        crate::error::PoolError::Storage,
        crate::error::PoolError::Conflict,
    ] {
        let reason = source.reason_code();
        let error = source.refresh_intent_context(repository_id, operation_id);
        let receipt = crate::cli::Envelope::failure("acquire".into(), &error);
        assert_that!(receipt.outcome, eq("rejected"));
        assert_that!(receipt.reason_code, eq(reason));
        assert_that!(receipt.context, eq(&serde_json::json!({})));
        assert_that!(
            receipt.data["next_action"]
                .as_str()
                .unwrap()
                .contains("recorded operation"),
            eq(false)
        );
    }
    let error =
        crate::error::PoolError::CommitUnknown.refresh_intent_context(repository_id, operation_id);
    let receipt = crate::cli::Envelope::failure("acquire".into(), &error);
    assert_that!(receipt.outcome, eq("unknown"));
    assert_that!(
        receipt.context["operation_id"].as_str(),
        eq(Some(operation_id.to_string().as_str()))
    );
}

#[googletest::test]
fn remote_default_interpretation_selects_one_usable_branch_without_exposing_input() {
    use crate::refresh::interpret_default;
    let oid = "0123456789abcdef0123456789abcdef01234567";
    for (branch, destination) in [
        ("refs/heads/master", "refs/remotes/origin/master"),
        ("refs/heads/HEAD-topic", "refs/remotes/origin/HEAD-topic"),
        (
            "refs/heads/release/stable",
            "refs/remotes/origin/release/stable",
        ),
    ] {
        let baseline =
            interpret_default(format!("ref: {branch}\tHEAD\n{oid}\tHEAD\n").as_bytes()).unwrap();
        assert_that!(baseline.source_reference.as_str(), eq(branch));
        assert_that!(baseline.destination_reference.as_str(), eq(destination));
    }
    let with_tail_match = format!(
        "ref: refs/heads/release/HEAD\tHEAD\n{oid}\tHEAD\n{oid}\trefs/heads/release/HEAD\n"
    );
    assert_that!(
        interpret_default(with_tail_match.as_bytes())
            .unwrap()
            .source_reference
            .as_str(),
        eq("refs/heads/release/HEAD")
    );
    assert_that!(
        interpret_default(format!("ref: refs/heads/@\tHEAD\n{oid}\tHEAD\n").as_bytes())
            .unwrap()
            .source_reference
            .as_str(),
        eq("refs/heads/@")
    );
    for (input, category) in [
        (String::new(), FailureCategory::MissingDefault),
        (format!("{oid}\tHEAD\n"), FailureCategory::MissingDefault),
        (
            format!("ref: refs/heads/master\tHEAD\nref: refs/heads/main\tHEAD\n{oid}\tHEAD\n"),
            FailureCategory::AmbiguousDefault,
        ),
        (
            format!("ref: refs/heads/master\tHEAD\r\n{oid}\tHEAD\r\n"),
            FailureCategory::InvalidDefault,
        ),
        (
            format!("ref: refs/heads/main\tHEAD\n{}\tHEAD\n", "0".repeat(40)),
            FailureCategory::InvalidDefault,
        ),
        (
            "ref: refs/heads/main\tHEAD\n".into(),
            FailureCategory::InvalidDefault,
        ),
        (
            format!("ref: https://user:secret@example.invalid/repo\tHEAD\n{oid}\tHEAD\n"),
            FailureCategory::InvalidDefault,
        ),
    ] {
        let diagnostic = interpret_default(input.as_bytes()).unwrap_err();
        assert_that!(diagnostic.category, eq(category));
        assert_that!(diagnostic.stage, eq(RefreshStage::Discovery));
        assert_that!(diagnostic.source_reference.as_deref(), eq(None));
        assert_that!(diagnostic.destination_reference.as_deref(), eq(None));
        assert_that!(
            serde_json::to_string(&diagnostic)
                .unwrap()
                .contains("secret"),
            eq(false)
        );
        assert_that!(diagnostic.validate_for(None).is_ok(), eq(true));
    }
    for branch in [
        "HEAD",
        "HEAD/topic",
        "HEAD/release/stable",
        "",
        "/main",
        ".hidden",
        "a/.hidden",
        "a.lock",
        "a//b",
        "a..b",
        "a@{b",
        "a\\b",
        "a:b",
        "a b",
        "a\nb",
        "a.",
    ] {
        let diagnostic =
            interpret_default(format!("ref: refs/heads/{branch}\tHEAD\n{oid}\tHEAD\n").as_bytes())
                .unwrap_err();
        assert_that!(diagnostic.category, eq(FailureCategory::InvalidDefault));
    }
}

#[googletest::test]
fn versioned_refresh_transitions_fix_the_baseline_and_preserve_historical_meaning() {
    use crate::{
        domain::{self, CatalogId, CatalogProjection},
        management::{
            ManagementEvent as Fact, OperationId, RefreshFinished, RefreshStarted, Repository,
            RepositoryId,
        },
        paths::EncodedPath,
        refresh::{BaselineSelected, RefreshBaseline, RefreshResult},
    };
    fn apply(
        state: &CatalogProjection,
        wire: &serde_json::Value,
    ) -> Result<CatalogProjection, crate::error::PoolError> {
        domain::reduce(
            Some(state),
            state.revision,
            &domain::decode_event(wire, state.catalog_id)?,
        )
    }
    let catalog = CatalogId::new();
    let initialized =
        domain::decode_event(&domain::initialized_event(catalog).unwrap(), catalog).unwrap();
    let mut state = domain::reduce(None, 0, &initialized).unwrap();
    let repository_id = RepositoryId::new();
    let registered = domain::management_event(
        catalog,
        &Fact::RepositoryRegistered(Repository {
            repository_id,
            common_directory: EncodedPath::from_path(std::path::Path::new("/private/git")),
            context_path: EncodedPath::from_path(std::path::Path::new("/private")),
            capacity: 4,
            revision: 0,
        }),
    )
    .unwrap();
    state = apply(&state, &registered).unwrap();
    let commit = "0123456789abcdef0123456789abcdef01234567".to_owned();
    for historical in [true, false] {
        let operation_id = OperationId::new();
        let start = RefreshStarted {
            operation_id,
            repository_id,
        };
        let started = domain::management_event(
            catalog,
            &if historical {
                Fact::RefreshStarted(start)
            } else {
                Fact::RefreshDiscoveryStarted(start)
            },
        )
        .unwrap();
        let cause = started["id"].as_str().unwrap();
        state = apply(&state, &started).unwrap();
        let finish = |fact| domain::management_event_caused_by(catalog, &fact, cause).unwrap();
        if historical {
            let finished = finish(Fact::RefreshFinished(RefreshFinished {
                operation_id,
                repository_id,
                resolved_commit: Some(commit.clone()),
            }));
            state = apply(&state, &finished).unwrap();
            let operation = state.operations.last().unwrap();
            assert_that!(
                operation
                    .recorded_baseline()
                    .unwrap()
                    .source_reference
                    .as_str(),
                eq("refs/heads/main")
            );
            let wire = serde_json::to_value(operation).unwrap();
            assert_that!(wire.get("baseline"), eq(None));
            assert_that!(wire.get("discovers_default"), eq(None));
            assert_that!(wire.get("diagnostic"), eq(None));
            continue;
        }
        let success = finish(Fact::RefreshResult(RefreshResult {
            operation_id,
            repository_id,
            resolved_commit: Some(commit.clone()),
            diagnostic: None,
        }));
        assert_that!(apply(&state, &success).is_err(), eq(true));
        let selected = finish(Fact::RefreshBaselineSelected(BaselineSelected {
            operation_id,
            repository_id,
            baseline: RefreshBaseline::from_source("refs/heads/release/stable").unwrap(),
        }));
        let mut forged = selected.clone();
        forged["causationid"] = serde_json::json!(OperationId::new());
        assert_that!(apply(&state, &forged).is_err(), eq(true));
        state = apply(&state, &selected).unwrap();
        assert_that!(
            state.operations.last().unwrap().last_checkpoint.as_str(),
            eq("baseline_selected")
        );
        assert_that!(apply(&state, &selected).is_err(), eq(true));
        let changed = finish(Fact::RefreshBaselineSelected(BaselineSelected {
            operation_id,
            repository_id,
            baseline: RefreshBaseline::historical(),
        }));
        assert_that!(apply(&state, &changed).is_err(), eq(true));
        let legacy = finish(Fact::RefreshFinished(RefreshFinished {
            operation_id,
            repository_id,
            resolved_commit: Some(commit.clone()),
        }));
        assert_that!(apply(&state, &legacy).is_err(), eq(true));
        let diagnostic =
            RefreshDiagnostic::new(RefreshStage::Fetch, FailureCategory::GitFailed, Some(128))
                .for_baseline(Some(&RefreshBaseline::historical()));
        let wrong_baseline = finish(Fact::RefreshResult(RefreshResult {
            operation_id,
            repository_id,
            resolved_commit: None,
            diagnostic: Some(diagnostic),
        }));
        assert_that!(apply(&state, &wrong_baseline).is_err(), eq(true));
        state = apply(&state, &success).unwrap();
        assert_that!(
            state.operations.last().unwrap().resolved_commit.as_deref(),
            eq(Some(commit.as_str()))
        );
        assert_that!(
            state
                .operations
                .last()
                .unwrap()
                .recorded_baseline()
                .unwrap()
                .destination_reference
                .as_str(),
            eq("refs/remotes/origin/release/stable")
        );
        assert_that!(apply(&state, &success).is_err(), eq(true));
    }
}
