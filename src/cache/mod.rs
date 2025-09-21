use crate::error::{Result, SprocketError};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedWorkflowResult {
    pub workflow_id: Uuid,
    pub workflow_name: String,
    pub cache_key: String,
    pub outputs: HashMap<String, serde_json::Value>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

pub struct WorkflowCache {
    cache: Arc<RwLock<HashMap<String, CachedWorkflowResult>>>,
    ttl_hours: i64,
}

impl WorkflowCache {
    pub fn new(ttl_hours: i64) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl_hours,
        }
    }

    pub fn generate_cache_key(
        workflow_name: &str,
        inputs: &HashMap<String, serde_json::Value>,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(workflow_name.as_bytes());
        
        let mut sorted_inputs: Vec<_> = inputs.iter().collect();
        sorted_inputs.sort_by_key(|&(k, _)| k);
        
        for (key, value) in sorted_inputs {
            hasher.update(key.as_bytes());
            hasher.update(value.to_string().as_bytes());
        }
        
        format!("{:x}", hasher.finalize())
    }

    pub async fn get(
        &self,
        workflow_name: &str,
        inputs: &HashMap<String, serde_json::Value>,
    ) -> Option<CachedWorkflowResult> {
        let cache_key = Self::generate_cache_key(workflow_name, inputs);
        let cache = self.cache.read().await;
        
        if let Some(cached_result) = cache.get(&cache_key) {
            if cached_result.expires_at > Utc::now() {
                return Some(cached_result.clone());
            }
        }
        
        None
    }

    pub async fn put(
        &self,
        workflow_id: Uuid,
        workflow_name: String,
        inputs: &HashMap<String, serde_json::Value>,
        outputs: HashMap<String, serde_json::Value>,
        status: String,
    ) -> Result<()> {
        let cache_key = Self::generate_cache_key(&workflow_name, inputs);
        let now = Utc::now();
        let expires_at = now + Duration::hours(self.ttl_hours);
        
        let cached_result = CachedWorkflowResult {
            workflow_id,
            workflow_name,
            cache_key: cache_key.clone(),
            outputs,
            status,
            created_at: now,
            expires_at,
        };
        
        let mut cache = self.cache.write().await;
        cache.insert(cache_key, cached_result);
        
        Ok(())
    }

    pub async fn invalidate(&self, workflow_name: &str, inputs: &HashMap<String, serde_json::Value>) {
        let cache_key = Self::generate_cache_key(workflow_name, inputs);
        let mut cache = self.cache.write().await;
        cache.remove(&cache_key);
    }

    pub async fn clear_expired(&self) {
        let now = Utc::now();
        let mut cache = self.cache.write().await;
        cache.retain(|_, v| v.expires_at > now);
    }

    pub async fn clear_all(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    pub async fn size(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_generate_cache_key_deterministic() {
        let workflow_name = "TestWorkflow";
        let mut inputs = HashMap::new();
        inputs.insert("input1".to_string(), json!("value1"));
        inputs.insert("input2".to_string(), json!(42));
        
        let key1 = WorkflowCache::generate_cache_key(workflow_name, &inputs);
        let key2 = WorkflowCache::generate_cache_key(workflow_name, &inputs);
        
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_generate_cache_key_different_inputs() {
        let workflow_name = "TestWorkflow";
        
        let mut inputs1 = HashMap::new();
        inputs1.insert("input1".to_string(), json!("value1"));
        
        let mut inputs2 = HashMap::new();
        inputs2.insert("input1".to_string(), json!("value2"));
        
        let key1 = WorkflowCache::generate_cache_key(workflow_name, &inputs1);
        let key2 = WorkflowCache::generate_cache_key(workflow_name, &inputs2);
        
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_generate_cache_key_different_workflows() {
        let mut inputs = HashMap::new();
        inputs.insert("input1".to_string(), json!("value1"));
        
        let key1 = WorkflowCache::generate_cache_key("Workflow1", &inputs);
        let key2 = WorkflowCache::generate_cache_key("Workflow2", &inputs);
        
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_generate_cache_key_order_independent() {
        let workflow_name = "TestWorkflow";
        
        let mut inputs1 = HashMap::new();
        inputs1.insert("a".to_string(), json!("1"));
        inputs1.insert("b".to_string(), json!("2"));
        inputs1.insert("c".to_string(), json!("3"));
        
        let mut inputs2 = HashMap::new();
        inputs2.insert("c".to_string(), json!("3"));
        inputs2.insert("a".to_string(), json!("1"));
        inputs2.insert("b".to_string(), json!("2"));
        
        let key1 = WorkflowCache::generate_cache_key(workflow_name, &inputs1);
        let key2 = WorkflowCache::generate_cache_key(workflow_name, &inputs2);
        
        assert_eq!(key1, key2);
    }

    #[tokio::test]
    async fn test_cache_put_and_get() {
        let cache = WorkflowCache::new(1);
        let workflow_id = Uuid::new_v4();
        let workflow_name = "TestWorkflow".to_string();
        
        let mut inputs = HashMap::new();
        inputs.insert("input1".to_string(), json!("value1"));
        
        let mut outputs = HashMap::new();
        outputs.insert("output1".to_string(), json!("result1"));
        
        cache.put(
            workflow_id,
            workflow_name.clone(),
            &inputs,
            outputs.clone(),
            "Completed".to_string(),
        ).await.unwrap();
        
        let cached = cache.get(&workflow_name, &inputs).await;
        assert!(cached.is_some());
        
        let cached_result = cached.unwrap();
        assert_eq!(cached_result.workflow_id, workflow_id);
        assert_eq!(cached_result.outputs, outputs);
        assert_eq!(cached_result.status, "Completed");
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        let cache = WorkflowCache::new(0);
        let workflow_id = Uuid::new_v4();
        let workflow_name = "TestWorkflow".to_string();
        
        let mut inputs = HashMap::new();
        inputs.insert("input1".to_string(), json!("value1"));
        
        let outputs = HashMap::new();
        
        cache.put(
            workflow_id,
            workflow_name.clone(),
            &inputs,
            outputs,
            "Completed".to_string(),
        ).await.unwrap();
        
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        let cached = cache.get(&workflow_name, &inputs).await;
        assert!(cached.is_none());
    }

    #[tokio::test]
    async fn test_cache_invalidate() {
        let cache = WorkflowCache::new(1);
        let workflow_id = Uuid::new_v4();
        let workflow_name = "TestWorkflow".to_string();
        
        let mut inputs = HashMap::new();
        inputs.insert("input1".to_string(), json!("value1"));
        
        let outputs = HashMap::new();
        
        cache.put(
            workflow_id,
            workflow_name.clone(),
            &inputs,
            outputs,
            "Completed".to_string(),
        ).await.unwrap();
        
        let cached = cache.get(&workflow_name, &inputs).await;
        assert!(cached.is_some());
        
        cache.invalidate(&workflow_name, &inputs).await;
        
        let cached = cache.get(&workflow_name, &inputs).await;
        assert!(cached.is_none());
    }

    #[tokio::test]
    async fn test_cache_clear_expired() {
        let cache = WorkflowCache::new(0);
        
        let mut inputs1 = HashMap::new();
        inputs1.insert("input1".to_string(), json!("value1"));
        
        let mut inputs2 = HashMap::new();
        inputs2.insert("input2".to_string(), json!("value2"));
        
        cache.put(
            Uuid::new_v4(),
            "Workflow1".to_string(),
            &inputs1,
            HashMap::new(),
            "Completed".to_string(),
        ).await.unwrap();
        
        cache.put(
            Uuid::new_v4(),
            "Workflow2".to_string(),
            &inputs2,
            HashMap::new(),
            "Completed".to_string(),
        ).await.unwrap();
        
        assert_eq!(cache.size().await, 2);
        
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        cache.clear_expired().await;
        
        assert_eq!(cache.size().await, 0);
    }

    #[tokio::test]
    async fn test_cache_clear_all() {
        let cache = WorkflowCache::new(1);
        
        for i in 0..5 {
            let mut inputs = HashMap::new();
            inputs.insert("input".to_string(), json!(i));
            
            cache.put(
                Uuid::new_v4(),
                format!("Workflow{}", i),
                &inputs,
                HashMap::new(),
                "Completed".to_string(),
            ).await.unwrap();
        }
        
        assert_eq!(cache.size().await, 5);
        
        cache.clear_all().await;
        
        assert_eq!(cache.size().await, 0);
    }
}