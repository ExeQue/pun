//! The tests that set the process-wide project codebase (`set_project_codebase`). They are their own binary so that
//! the codebase one of them sets is never seen by another lint test running beside it, and `PROJECT` serialises them.

use std::sync::Mutex;

mod moved_closure;
mod project_codebase;
mod project_rounds;

static PROJECT: Mutex<()> = Mutex::new(());
