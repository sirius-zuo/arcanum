use std::collections::HashMap;
use std::sync::RwLock;
use serde_json::Value;
use tracing::instrument;

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl ToolDefinition {
    pub fn new(name: impl Into<String>, description: impl Into<String>, input_schema: Value) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }
}

#[derive(Debug)]
pub struct CapabilityRegistry {
    tools: RwLock<HashMap<String, ToolDefinition>>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self { tools: RwLock::new(HashMap::new()) }
    }

    #[instrument(skip(self, tool), fields(tool_name = %tool.name))]
    pub fn register(&self, tool: ToolDefinition) {
        self.tools.write().unwrap().insert(tool.name.clone(), tool);
    }

    pub fn list(&self) -> Vec<ToolDefinition> {
        let tools = self.tools.read().unwrap();
        let mut list: Vec<_> = tools.values().cloned().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    #[instrument(skip(self), fields(tool_name = name))]
    pub fn get(&self, name: &str) -> Option<ToolDefinition> {
        self.tools.read().unwrap().get(name).cloned()
    }

    #[instrument(skip(self), fields(tool_name = name))]
    pub fn unregister(&self, name: &str) -> bool {
        self.tools.write().unwrap().remove(name).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_register_and_list_tool() {
        let registry = CapabilityRegistry::new();
        let tool = ToolDefinition::new(
            "search",
            "Search the index",
            json!({ "type": "object", "properties": { "query": { "type": "string" } } }),
        );
        registry.register(tool);
        let list = registry.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "search");
        assert_eq!(list[0].description, "Search the index");
    }

    #[test]
    fn test_get_registered_tool() {
        let registry = CapabilityRegistry::new();
        registry.register(ToolDefinition::new("ingest", "Ingest docs", json!({})));
        assert!(registry.get("ingest").is_some());
        assert!(registry.get("unknown").is_none());
    }

    #[test]
    fn test_unregister_tool() {
        let registry = CapabilityRegistry::new();
        registry.register(ToolDefinition::new("ingest", "Ingest docs", json!({})));
        assert!(registry.unregister("ingest"));
        assert!(registry.list().is_empty());
        assert!(!registry.unregister("ingest"));
    }
}
