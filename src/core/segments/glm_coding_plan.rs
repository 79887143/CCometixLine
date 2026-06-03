use super::{Segment, SegmentData};
use crate::config::{InputData, SegmentId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// API response structures
#[derive(Debug, Deserialize)]
struct QuotaResponse {
    code: i32,
    data: QuotaData,
}

#[derive(Debug, Deserialize)]
struct QuotaData {
    limits: Vec<QuotaLimit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QuotaLimit {
    #[serde(rename = "type")]
    limit_type: String,
    unit: u32,
    number: u32,
    percentage: u32,
}

// Cache structure for file-based persistence
#[derive(Debug, Serialize, Deserialize)]
struct QuotaCache {
    timestamp: u64,
    data: Vec<QuotaLimit>,
}

pub struct GlmCodingPlanSegment {
    api_url: String,
    token: String,
    cache_duration_secs: u64,
}

impl Default for GlmCodingPlanSegment {
    fn default() -> Self {
        Self::new()
    }
}

impl GlmCodingPlanSegment {
    pub fn new() -> Self {
        Self {
            api_url: "https://bigmodel.cn/api/monitor/usage/quota/limit".to_string(),
            token: String::new(),
            cache_duration_secs: 60,
        }
    }

    pub fn with_api_url(mut self, url: String) -> Self {
        if !url.is_empty() {
            self.api_url = url;
        }
        self
    }

    pub fn with_token(mut self, token: String) -> Self {
        self.token = token;
        self
    }

    pub fn with_cache_duration(mut self, secs: u64) -> Self {
        if secs > 0 {
            self.cache_duration_secs = secs;
        }
        self
    }

    fn get_cache_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude")
            .join("ccline")
            .join(".glm_quota.json")
    }

    /// Load cached quota data if still valid
    fn load_cache(&self) -> Option<Vec<QuotaLimit>> {
        let path = Self::get_cache_path();
        let content = fs::read_to_string(&path).ok()?;
        let cache: QuotaCache = serde_json::from_str(&content).ok()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_secs();

        if now.saturating_sub(cache.timestamp) < self.cache_duration_secs {
            Some(cache.data)
        } else {
            None
        }
    }

    /// Load expired cache as fallback when API fails
    fn load_expired_cache(&self) -> Option<Vec<QuotaLimit>> {
        let path = Self::get_cache_path();
        let content = fs::read_to_string(&path).ok()?;
        let cache: QuotaCache = serde_json::from_str(&content).ok()?;
        Some(cache.data)
    }

    fn save_cache(&self, data: &[QuotaLimit]) {
        let path = Self::get_cache_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let cache = QuotaCache {
            timestamp: now,
            data: data.to_vec(),
        };
        if let Ok(content) = serde_json::to_string(&cache) {
            let _ = fs::write(&path, content);
        }
    }

    fn fetch_quota(&self) -> Option<Vec<QuotaLimit>> {
        // Check cache first
        if let Some(cached) = self.load_cache() {
            return Some(cached);
        }

        // Make API request
        match ureq::get(&self.api_url)
            .header("Authorization", &self.token)
            .call()
        {
            Ok(resp) => {
                let quota_resp: QuotaResponse = resp.into_body().read_json().ok()?;

                if quota_resp.code != 200 {
                    return self.load_expired_cache();
                }

                // Filter TOKENS_LIMIT only, ignore TIME_LIMIT
                let limits: Vec<QuotaLimit> = quota_resp
                    .data
                    .limits
                    .into_iter()
                    .filter(|l| l.limit_type == "TOKENS_LIMIT")
                    .collect();

                // Save to cache
                self.save_cache(&limits);

                Some(limits)
            }
            Err(_) => {
                // API failed, try expired cache as fallback
                self.load_expired_cache()
            }
        }
    }

    /// Format quota data into display string
    /// unit 3 = hours → "5h:44%"
    /// unit 6 = weeks → "7d:18%" (convert to days)
    fn format_quota(&self, limits: &[QuotaLimit]) -> String {
        let mut parts = Vec::new();

        for limit in limits {
            let display = match limit.unit {
                3 => format!("{}h:{}%", limit.number, limit.percentage),
                6 => format!("{}d:{}%", limit.number * 7, limit.percentage),
                _ => continue,
            };
            parts.push(display);
        }

        parts.join(" · ")
    }
}

impl Segment for GlmCodingPlanSegment {
    fn collect(&self, _input: &InputData) -> Option<SegmentData> {
        // Need token to make API request
        if self.token.is_empty() {
            return None;
        }

        let limits = self.fetch_quota()?;
        let primary = self.format_quota(&limits);

        if primary.is_empty() {
            return None;
        }

        Some(SegmentData {
            primary,
            secondary: String::new(),
            metadata: HashMap::new(),
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::GlmCodingPlan
    }
}
