use super::{Segment, SegmentData};
use crate::config::{InputData, SegmentId};
use std::collections::HashMap;

#[derive(Default)]
pub struct GlmCodingPlanSegment;

impl GlmCodingPlanSegment {
    pub fn new() -> Self {
        Self
    }
}

impl Segment for GlmCodingPlanSegment {
    fn collect(&self, _input: &InputData) -> Option<SegmentData> {
        // TODO: 后续开发动态数据获取，当前使用静态内容
        Some(SegmentData {
            primary: "5h:22% · 7d:10%".to_string(),
            secondary: String::new(),
            metadata: HashMap::new(),
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::GlmCodingPlan
    }
}
