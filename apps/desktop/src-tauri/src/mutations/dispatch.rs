use std::collections::HashMap;
use std::sync::Arc;

use super::handlers;
use super::{Mutation, MutationKind};

pub fn dispatch_table() -> HashMap<MutationKind, Arc<dyn Mutation>> {
    let mut map = HashMap::new();
    for handler in handlers::all() {
        map.insert(handler.kind(), handler);
    }
    map
}

pub fn handler_for(kind: MutationKind) -> Option<Arc<dyn Mutation>> {
    match kind {
        MutationKind::AddComment => Some(Arc::new(handlers::comments::AddComment)),
        MutationKind::EditComment => Some(Arc::new(handlers::comments::EditComment)),
        MutationKind::DeleteComment => Some(Arc::new(handlers::comments::DeleteComment)),
        MutationKind::AddReaction => Some(Arc::new(handlers::reactions::AddReaction)),
        MutationKind::RemoveReaction => Some(Arc::new(handlers::reactions::RemoveReaction)),
        MutationKind::AddLabel => Some(Arc::new(handlers::labels::AddLabel)),
        MutationKind::RemoveLabel => Some(Arc::new(handlers::labels::RemoveLabel)),
        MutationKind::SetAssignees => Some(Arc::new(handlers::assignees::SetAssignees)),
        MutationKind::RequestReview => Some(Arc::new(handlers::reviewers::RequestReview)),
        MutationKind::RemoveReviewRequest => {
            Some(Arc::new(handlers::reviewers::RemoveReviewRequest))
        }
        MutationKind::SubmitReview => Some(Arc::new(handlers::reviews::SubmitReview)),
        MutationKind::ResolveThread => Some(Arc::new(handlers::threads::ResolveThread)),
        MutationKind::UnresolveThread => Some(Arc::new(handlers::threads::UnresolveThread)),
        MutationKind::MarkFileViewed => Some(Arc::new(handlers::viewed_files::MarkFileViewed)),
        MutationKind::UnmarkFileViewed => Some(Arc::new(handlers::viewed_files::UnmarkFileViewed)),
        MutationKind::UpdatePrTitle => Some(Arc::new(handlers::pr_meta::UpdatePrTitle)),
        MutationKind::UpdatePrDescription => Some(Arc::new(handlers::pr_meta::UpdatePrDescription)),
        MutationKind::SetMilestone => Some(Arc::new(handlers::pr_meta::SetMilestone)),
        MutationKind::SetProject => Some(Arc::new(handlers::pr_meta::SetProject)),
        MutationKind::ConvertToDraft => Some(Arc::new(handlers::pr_meta::ConvertToDraft)),
        MutationKind::MarkReadyForReview => Some(Arc::new(handlers::pr_meta::MarkReadyForReview)),
        MutationKind::EnableAutoMerge => Some(Arc::new(handlers::merge_controls::EnableAutoMerge)),
        MutationKind::DisableAutoMerge => {
            Some(Arc::new(handlers::merge_controls::DisableAutoMerge))
        }
        MutationKind::UpdateBranch => Some(Arc::new(handlers::merge_controls::UpdateBranch)),
        MutationKind::Merge => Some(Arc::new(handlers::merge_controls::Merge)),
        MutationKind::ClosePr => Some(Arc::new(handlers::merge_controls::ClosePr)),
        MutationKind::ReopenPr => Some(Arc::new(handlers::merge_controls::ReopenPr)),
    }
}
