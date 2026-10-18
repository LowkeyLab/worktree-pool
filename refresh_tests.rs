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
