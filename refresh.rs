//! Controlled diagnostics for the fixed origin/main refresh.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshStage {
    RepositoryValidation,
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
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RefreshReference {
    #[serde(rename = "refs/heads/main")]
    Main,
    #[serde(rename = "refs/remotes/origin/main")]
    OriginMain,
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
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshDiagnostic {
    pub category: FailureCategory,
    pub stage: RefreshStage,
    pub source_reference: Option<RefreshReference>,
    pub destination_reference: Option<RefreshReference>,
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
            RefreshStage::RepositoryValidation => (None, None),
            RefreshStage::Fetch => (
                Some(RefreshReference::Main),
                Some(RefreshReference::OriginMain),
            ),
            RefreshStage::ResolveCommit => (Some(RefreshReference::OriginMain), None),
        };
        let next_action = match category {
            FailureCategory::RepositoryUnavailable => CorrectiveInstruction::CheckRepository,
            FailureCategory::CommandUnavailable => CorrectiveInstruction::CheckGit,
            FailureCategory::GitFailed | FailureCategory::InvalidCommit => match stage {
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
