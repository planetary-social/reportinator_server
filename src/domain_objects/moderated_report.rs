use crate::domain_objects::{ModerationCategory, ReportRequest};
use std::fmt::{self, Display, Formatter};

pub struct ModeratedReport {
    pub request: ReportRequest,
    pub category: Option<ModerationCategory>,
}

impl ModeratedReport {
    pub(super) fn new(request: ReportRequest, category: Option<ModerationCategory>) -> Self {
        ModeratedReport { request, category }
    }
}

impl Display for ModeratedReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ModeratedReport {{ request: {}, category: {:?} }}",
            self.request, self.category
        )
    }
}
