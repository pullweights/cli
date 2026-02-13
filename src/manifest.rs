use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullWeightsManifest {
    pub schema_version: u32,
    pub name: String,
    pub org: String,
    pub tag: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub framework: Option<String>,
    #[serde(default)]
    pub architecture: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub files: Vec<ManifestFile>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    pub filename: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub content_type: Option<String>,
}

impl PullWeightsManifest {
    #[allow(dead_code)]
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size_bytes).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_serialization() {
        let manifest = PullWeightsManifest {
            schema_version: 1,
            name: "llama-7b".to_string(),
            org: "meta".to_string(),
            tag: "v1.0".to_string(),
            description: Some("LLaMA 7B model".to_string()),
            framework: Some("pytorch".to_string()),
            architecture: Some("transformer".to_string()),
            license: Some("MIT".to_string()),
            files: vec![ManifestFile {
                filename: "model.bin".to_string(),
                size_bytes: 1024,
                sha256: "abc123".to_string(),
                content_type: Some("application/octet-stream".to_string()),
            }],
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: PullWeightsManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "llama-7b");
        assert_eq!(deserialized.total_size(), 1024);
    }
}
