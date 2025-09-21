use crate::error::{Result, SprocketError};
use crate::parser::{parse_wdl, Import, Task, WdlDocument, Workflow};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

pub struct ImportResolver {
    cache: HashMap<String, WdlDocument>,
    visited: HashSet<String>,
}

impl ImportResolver {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            visited: HashSet::new(),
        }
    }

    pub fn resolve_imports<'a>(
        &'a mut self,
        document: &'a mut WdlDocument,
        base_path: Option<&'a Path>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let imports = document.imports.clone();
            
            for import in imports {
                self.resolve_single_import(&import, document, base_path).await?;
            }
            
            Ok(())
        })
    }

    async fn resolve_single_import(
        &mut self,
        import: &Import,
        parent_doc: &mut WdlDocument,
        base_path: Option<&Path>,
    ) -> Result<()> {
        let resolved_path = self.resolve_uri(&import.uri, base_path)?;
        
        if self.visited.contains(&resolved_path) {
            return Err(SprocketError::ParseError(format!(
                "Circular import detected: {}",
                resolved_path
            )));
        }
        
        self.visited.insert(resolved_path.clone());
        
        let imported_doc = if let Some(cached) = self.cache.get(&resolved_path) {
            cached.clone()
        } else {
            let content = self.fetch_content(&resolved_path).await?;
            let mut doc = parse_wdl(&content)?;
            
            let import_base = if resolved_path.starts_with("http") {
                None
            } else {
                Path::new(&resolved_path).parent()
            };
            
            Box::pin(self.resolve_imports(&mut doc, import_base)).await?;
            self.cache.insert(resolved_path.clone(), doc.clone());
            doc
        };
        
        self.merge_document(parent_doc, &imported_doc, import.alias.as_deref())?;
        
        self.visited.remove(&resolved_path);
        
        Ok(())
    }

    fn resolve_uri(&self, uri: &str, base_path: Option<&Path>) -> Result<String> {
        if uri.starts_with("http://") || uri.starts_with("https://") {
            Ok(uri.to_string())
        } else if let Some(base) = base_path {
            let path = base.join(uri);
            path.canonicalize()
                .map(|p| p.to_string_lossy().to_string())
                .map_err(|e| SprocketError::ParseError(format!(
                    "Failed to resolve import path '{}': {}",
                    uri, e
                )))
        } else {
            let path = PathBuf::from(uri);
            path.canonicalize()
                .map(|p| p.to_string_lossy().to_string())
                .map_err(|e| SprocketError::ParseError(format!(
                    "Failed to resolve import path '{}': {}",
                    uri, e
                )))
        }
    }

    async fn fetch_content(&self, path: &str) -> Result<String> {
        if path.starts_with("http://") || path.starts_with("https://") {
            self.fetch_remote_content(path).await
        } else {
            std::fs::read_to_string(path)
                .map_err(|e| SprocketError::ParseError(format!(
                    "Failed to read import file '{}': {}",
                    path, e
                )))
        }
    }

    async fn fetch_remote_content(&self, url: &str) -> Result<String> {
        use tokio::process::Command;
        
        let output = Command::new("curl")
            .arg("-s")
            .arg(url)
            .output()
            .await
            .map_err(|e| SprocketError::ParseError(format!(
                "Failed to fetch remote import '{}': {}",
                url, e
            )))?;
        
        if !output.status.success() {
            return Err(SprocketError::ParseError(format!(
                "Failed to fetch remote import '{}': curl returned non-zero status",
                url
            )));
        }
        
        String::from_utf8(output.stdout)
            .map_err(|e| SprocketError::ParseError(format!(
                "Invalid UTF-8 in remote import '{}': {}",
                url, e
            )))
    }

    fn merge_document(
        &self,
        parent: &mut WdlDocument,
        imported: &WdlDocument,
        alias: Option<&str>,
    ) -> Result<()> {
        for task in &imported.tasks {
            let mut merged_task = task.clone();
            
            if let Some(prefix) = alias {
                merged_task.name = format!("{}.{}", prefix, task.name);
            }
            
            if parent.tasks.iter().any(|t| t.name == merged_task.name) {
                return Err(SprocketError::ParseError(format!(
                    "Duplicate task name after import: {}",
                    merged_task.name
                )));
            }
            
            parent.tasks.push(merged_task);
        }
        
        for workflow in &imported.workflows {
            let mut merged_workflow = workflow.clone();
            
            if let Some(prefix) = alias {
                merged_workflow.name = format!("{}.{}", prefix, workflow.name);
                
                for call in &mut merged_workflow.calls {
                    if !call.task_name.contains('.') {
                        call.task_name = format!("{}.{}", prefix, call.task_name);
                    }
                }
            }
            
            if parent.workflows.iter().any(|w| w.name == merged_workflow.name) {
                return Err(SprocketError::ParseError(format!(
                    "Duplicate workflow name after import: {}",
                    merged_workflow.name
                )));
            }
            
            parent.workflows.push(merged_workflow);
        }
        
        Ok(())
    }
}

pub async fn resolve_document_imports(document: &mut WdlDocument, base_path: Option<&Path>) -> Result<()> {
    let mut resolver = ImportResolver::new();
    resolver.resolve_imports(document, base_path).await
}