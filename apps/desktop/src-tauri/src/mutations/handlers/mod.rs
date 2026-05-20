use std::sync::Arc;

use super::{Mutation, MutationKind};

pub mod assignees;
pub mod checks;
pub mod comments;
mod common;
pub mod labels;
pub mod merge_controls;
pub mod merge_queue;
pub mod pr_meta;
pub mod reactions;
pub mod review_comments;
pub mod reviewers;
pub mod reviews;
pub mod suggestions;
pub mod threads;
pub mod viewed_files;

pub fn all() -> Vec<Arc<dyn Mutation>> {
    vec![
        Arc::new(comments::AddComment),
        Arc::new(review_comments::AddReviewComment),
        Arc::new(comments::EditComment),
        Arc::new(comments::DeleteComment),
        Arc::new(reactions::AddReaction),
        Arc::new(reactions::RemoveReaction),
        Arc::new(labels::AddLabel),
        Arc::new(labels::RemoveLabel),
        Arc::new(assignees::SetAssignees),
        Arc::new(reviewers::RequestReview),
        Arc::new(reviewers::RemoveReviewRequest),
        Arc::new(reviews::SubmitReview),
        Arc::new(threads::ResolveThread),
        Arc::new(threads::UnresolveThread),
        Arc::new(viewed_files::MarkFileViewed),
        Arc::new(viewed_files::UnmarkFileViewed),
        Arc::new(pr_meta::UpdatePrTitle),
        Arc::new(pr_meta::UpdatePrDescription),
        Arc::new(pr_meta::SetMilestone),
        Arc::new(pr_meta::SetProject),
        Arc::new(pr_meta::ConvertToDraft),
        Arc::new(pr_meta::MarkReadyForReview),
        Arc::new(merge_controls::EnableAutoMerge),
        Arc::new(merge_controls::DisableAutoMerge),
        Arc::new(merge_controls::UpdateBranch),
        Arc::new(merge_controls::Merge),
        Arc::new(merge_controls::DeleteHeadRef),
        Arc::new(merge_queue::EnqueueMergeQueue),
        Arc::new(merge_queue::DequeueMergeQueue),
        Arc::new(merge_queue::ReorderMergeQueue),
        Arc::new(merge_controls::ClosePr),
        Arc::new(merge_controls::ReopenPr),
        Arc::new(suggestions::ApplySuggestion),
        Arc::new(suggestions::ApplySuggestionBatch),
        Arc::new(checks::RerunCheckRun),
        Arc::new(checks::RerunCheckSuite),
    ]
}

pub fn has_handler(kind: MutationKind) -> bool {
    matches!(
        kind,
        MutationKind::AddComment
            | MutationKind::AddReviewComment
            | MutationKind::EditComment
            | MutationKind::DeleteComment
            | MutationKind::AddReaction
            | MutationKind::RemoveReaction
            | MutationKind::AddLabel
            | MutationKind::RemoveLabel
            | MutationKind::SetAssignees
            | MutationKind::RequestReview
            | MutationKind::RemoveReviewRequest
            | MutationKind::SubmitReview
            | MutationKind::ResolveThread
            | MutationKind::UnresolveThread
            | MutationKind::MarkFileViewed
            | MutationKind::UnmarkFileViewed
            | MutationKind::UpdatePrTitle
            | MutationKind::UpdatePrDescription
            | MutationKind::SetMilestone
            | MutationKind::SetProject
            | MutationKind::ConvertToDraft
            | MutationKind::MarkReadyForReview
            | MutationKind::EnableAutoMerge
            | MutationKind::DisableAutoMerge
            | MutationKind::UpdateBranch
            | MutationKind::Merge
            | MutationKind::DeleteHeadRef
            | MutationKind::EnqueueMergeQueue
            | MutationKind::DequeueMergeQueue
            | MutationKind::ReorderMergeQueue
            | MutationKind::ClosePr
            | MutationKind::ReopenPr
            | MutationKind::ApplySuggestion
            | MutationKind::ApplySuggestionBatch
            | MutationKind::RerunCheckRun
            | MutationKind::RerunCheckSuite
    )
}
