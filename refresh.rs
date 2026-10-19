//! Refresh baselines and controlled diagnostics.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshStage {
    RepositoryValidation,
    Discovery,
    Fetch,
    ResolveCommit,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    RepositoryUnavailable,
    CommandUnavailable,
    GitFailed,
    InvalidCommit,
    MissingDefault,
    AmbiguousDefault,
    InvalidDefault,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CorrectiveInstruction {
    #[serde(
        rename = "Check the registered repository. Inspect this operation before explicit recovery."
    )]
    CheckRepository,
    #[serde(
        rename = "Check Git installation and execution permissions. Inspect this operation before explicit recovery."
    )]
    CheckGit,
    #[serde(
        rename = "Check origin access, the main branch, and destination reference locks. Inspect this operation before explicit recovery."
    )]
    CheckFetch,
    #[serde(
        rename = "Check the origin/main commit and local object storage. Inspect this operation before explicit recovery."
    )]
    CheckCommit,
    #[serde(
        rename = "Check origin access and its default branch. Inspect this operation before explicit recovery."
    )]
    CheckDiscovery,
    #[serde(
        rename = "Check origin access, the recorded baseline, and destination reference locks. Inspect this operation before explicit recovery."
    )]
    CheckBaselineFetch,
    #[serde(
        rename = "Check the recorded baseline commit and local object storage. Inspect this operation before explicit recovery."
    )]
    CheckBaselineCommit,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshDiagnostic {
    pub category: FailureCategory,
    pub stage: RefreshStage,
    pub source_reference: Option<String>,
    pub destination_reference: Option<String>,
    pub git_exit_status: Option<i32>,
    pub next_action: CorrectiveInstruction,
}
impl RefreshDiagnostic {
    pub(crate) fn git_failure(stage: RefreshStage, output: &std::process::Output) -> Self {
        let category = if stage == RefreshStage::RepositoryValidation {
            FailureCategory::RepositoryUnavailable
        } else {
            FailureCategory::GitFailed
        };
        Self::new(stage, category, output.status.code())
    }

    pub(crate) fn new(stage: RefreshStage, category: FailureCategory, status: Option<i32>) -> Self {
        let (source_reference, destination_reference) = match stage {
            RefreshStage::RepositoryValidation | RefreshStage::Discovery => (None, None),
            RefreshStage::Fetch => (
                Some("refs/heads/main".into()),
                Some("refs/remotes/origin/main".into()),
            ),
            RefreshStage::ResolveCommit => (Some("refs/remotes/origin/main".into()), None),
        };
        let next_action = match category {
            FailureCategory::RepositoryUnavailable => CorrectiveInstruction::CheckRepository,
            FailureCategory::CommandUnavailable => CorrectiveInstruction::CheckGit,
            FailureCategory::MissingDefault
            | FailureCategory::AmbiguousDefault
            | FailureCategory::InvalidDefault => CorrectiveInstruction::CheckDiscovery,
            FailureCategory::GitFailed | FailureCategory::InvalidCommit => match stage {
                RefreshStage::Discovery => CorrectiveInstruction::CheckDiscovery,
                RefreshStage::Fetch => CorrectiveInstruction::CheckFetch,
                _ => CorrectiveInstruction::CheckCommit,
            },
        };
        Self {
            category,
            stage,
            source_reference,
            destination_reference,
            git_exit_status: status,
            next_action,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), crate::error::PoolError> {
        let valid = match self.stage {
            RefreshStage::Discovery => false,
            RefreshStage::RepositoryValidation => matches!(
                self.category,
                FailureCategory::RepositoryUnavailable | FailureCategory::CommandUnavailable
            ),
            RefreshStage::Fetch => matches!(
                self.category,
                FailureCategory::CommandUnavailable | FailureCategory::GitFailed
            ),
            RefreshStage::ResolveCommit => matches!(
                self.category,
                FailureCategory::CommandUnavailable
                    | FailureCategory::GitFailed
                    | FailureCategory::InvalidCommit
            ),
        } && self
            .git_exit_status
            .is_none_or(|status| (0..=255).contains(&status))
            && (self.category != FailureCategory::CommandUnavailable
                || self.git_exit_status.is_none())
            && (self.category != FailureCategory::GitFailed || self.git_exit_status != Some(0))
            && (self.category != FailureCategory::InvalidCommit || self.git_exit_status == Some(0))
            && *self == Self::new(self.stage, self.category, self.git_exit_status);
        if valid {
            Ok(())
        } else {
            Err(crate::error::PoolError::Corrupt)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshBaseline {
    pub source_reference: String,
    pub destination_reference: String,
}
impl RefreshBaseline {
    pub(crate) fn from_source(source: &str) -> Result<Self, crate::error::PoolError> {
        let branch = source
            .strip_prefix("refs/heads/")
            .ok_or(crate::error::PoolError::Corrupt)?;
        if branch.is_empty()
            || branch.contains("..")
            || branch.contains("@{")
            || branch.ends_with('.')
            || branch
                .bytes()
                .any(|b| b <= b' ' || b == 127 || b"~^:?*[\\".contains(&b))
            || branch
                .split('/')
                .any(|part| part.is_empty() || part.starts_with('.') || part.ends_with(".lock"))
        {
            return Err(crate::error::PoolError::Corrupt);
        }
        Ok(Self {
            source_reference: source.into(),
            destination_reference: format!("refs/remotes/origin/{branch}"),
        })
    }
    pub(crate) fn validate(&self) -> Result<(), crate::error::PoolError> {
        if Self::from_source(&self.source_reference)? == *self {
            Ok(())
        } else {
            Err(crate::error::PoolError::Corrupt)
        }
    }
    pub(crate) fn historical() -> Self {
        Self {
            source_reference: "refs/heads/main".into(),
            destination_reference: "refs/remotes/origin/main".into(),
        }
    }
}

/// Interpret only the requested remote HEAD advertisement. No untrusted text enters diagnostics.
pub(crate) fn interpret_default(stdout: &[u8]) -> Result<RefreshBaseline, RefreshDiagnostic> {
    let failure = |category| RefreshDiagnostic::new(RefreshStage::Discovery, category, Some(0));
    let text = std::str::from_utf8(stdout).map_err(|_| failure(FailureCategory::InvalidDefault))?;
    let mut selected = None;
    let mut commit = None;
    if !text.is_empty() && !text.ends_with('\n') {
        return Err(failure(FailureCategory::InvalidDefault));
    }
    for line in text.split_terminator('\n') {
        if line.contains('\r') {
            return Err(failure(FailureCategory::InvalidDefault));
        }
        let (value, reference) = line
            .split_once('\t')
            .ok_or_else(|| failure(FailureCategory::InvalidDefault))?;
        // Git patterns also match reference tails, including branches that end in /HEAD.
        if reference != "HEAD" {
            continue;
        }
        if let Some(source) = value.strip_prefix("ref: ") {
            if selected.is_some() {
                return Err(failure(FailureCategory::AmbiguousDefault));
            }
            selected = Some(
                RefreshBaseline::from_source(source)
                    .map_err(|_| failure(FailureCategory::InvalidDefault))?,
            );
        } else {
            if commit.is_some() || !valid_commit(value) {
                return Err(failure(FailureCategory::InvalidDefault));
            }
            commit = Some(value);
        }
    }
    let baseline = selected.ok_or_else(|| failure(FailureCategory::MissingDefault))?;
    if commit.is_none() {
        return Err(failure(FailureCategory::InvalidDefault));
    }
    Ok(baseline)
}

pub(crate) fn valid_commit(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64)
        && oid.bytes().any(|b| b != b'0')
        && oid
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl RefreshDiagnostic {
    pub(crate) fn for_baseline(mut self, baseline: Option<&RefreshBaseline>) -> Self {
        self.source_reference = None;
        self.destination_reference = None;
        match self.stage {
            RefreshStage::Fetch => {
                self.source_reference = baseline.map(|b| b.source_reference.clone());
                self.destination_reference = baseline.map(|b| b.destination_reference.clone());
                if self.category == FailureCategory::GitFailed {
                    self.next_action = CorrectiveInstruction::CheckBaselineFetch;
                }
            }
            RefreshStage::ResolveCommit => {
                self.source_reference = baseline.map(|b| b.destination_reference.clone());
                if matches!(
                    self.category,
                    FailureCategory::GitFailed | FailureCategory::InvalidCommit
                ) {
                    self.next_action = CorrectiveInstruction::CheckBaselineCommit;
                }
            }
            _ => {}
        }
        self
    }
    pub(crate) fn validate_for(
        &self,
        baseline: Option<&RefreshBaseline>,
    ) -> Result<(), crate::error::PoolError> {
        let category_valid = match self.stage {
            RefreshStage::RepositoryValidation => {
                baseline.is_none()
                    && matches!(
                        self.category,
                        FailureCategory::RepositoryUnavailable
                            | FailureCategory::CommandUnavailable
                    )
            }
            RefreshStage::Discovery => {
                baseline.is_none()
                    && matches!(
                        self.category,
                        FailureCategory::CommandUnavailable
                            | FailureCategory::GitFailed
                            | FailureCategory::MissingDefault
                            | FailureCategory::AmbiguousDefault
                            | FailureCategory::InvalidDefault
                    )
            }
            RefreshStage::Fetch => {
                baseline.is_some()
                    && matches!(
                        self.category,
                        FailureCategory::CommandUnavailable | FailureCategory::GitFailed
                    )
            }
            RefreshStage::ResolveCommit => {
                baseline.is_some()
                    && matches!(
                        self.category,
                        FailureCategory::CommandUnavailable
                            | FailureCategory::GitFailed
                            | FailureCategory::InvalidCommit
                    )
            }
        };
        let status_valid = self.git_exit_status.is_none_or(|s| (0..=255).contains(&s))
            && (self.category != FailureCategory::CommandUnavailable
                || self.git_exit_status.is_none())
            && (self.category != FailureCategory::GitFailed || self.git_exit_status != Some(0))
            && (!matches!(
                self.category,
                FailureCategory::InvalidCommit
                    | FailureCategory::MissingDefault
                    | FailureCategory::AmbiguousDefault
                    | FailureCategory::InvalidDefault
            ) || self.git_exit_status == Some(0));
        if category_valid
            && status_valid
            && *self
                == Self::new(self.stage, self.category, self.git_exit_status).for_baseline(baseline)
        {
            Ok(())
        } else {
            Err(crate::error::PoolError::Corrupt)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineSelected {
    pub operation_id: crate::management::OperationId,
    pub repository_id: crate::management::RepositoryId,
    pub baseline: RefreshBaseline,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshResult {
    pub operation_id: crate::management::OperationId,
    pub repository_id: crate::management::RepositoryId,
    pub resolved_commit: Option<String>,
    pub diagnostic: Option<RefreshDiagnostic>,
}
