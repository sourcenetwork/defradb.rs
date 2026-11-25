//! Vector embedding types for AI/ML integration.
//!
//! Matches Go's VectorEmbeddingDescription in client/collection_description.go

use serde::{Deserialize, Serialize};

/// Describes configuration for generating embedding vectors.
/// Matches Go's VectorEmbeddingDescription.
///
/// Embeddings are AI/ML specific vector representations of document content.
/// When configured, embeddings may call 3rd party APIs inline with document mutations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorEmbeddingDescription {
    /// Name of the field on the collection that this embedding applies to.
    #[serde(rename = "FieldName", default)]
    pub field_name: String,

    /// Fields in the parent schema used as basis for vector generation.
    #[serde(rename = "Fields", default)]
    pub fields: Vec<String>,

    /// The LLM model to use for generating embeddings.
    /// Example: "text-embedding-3-small"
    #[serde(rename = "Model", default)]
    pub model: String,

    /// The API provider for generating embeddings.
    /// Example: "openai"
    #[serde(rename = "Provider", default)]
    pub provider: String,

    /// Optional template path for formatting field values.
    /// Uses Go template syntax with field names as variables.
    #[serde(rename = "Template", default)]
    pub template: String,

    /// URL endpoint of the provider's API.
    /// Example: `https://api.openai.com/v1`
    /// If not provided, uses the default URL for the given provider.
    #[serde(rename = "URL", default)]
    pub url: String,
}

/// Whether a Go `template` reads `field`, as `.field` inside `{{ }}`.
pub fn template_reads(template: &str, field: &str) -> bool {
    let is_name = |c: char| c.is_alphanumeric() || c == '_';
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let (code, after) = action_code(&rest[start + 2..]);
        let read = code.match_indices('.').any(|(at, _)| {
            let before = code[..at].chars().next_back();
            !before.is_some_and(is_name)
                && code[at + 1..]
                    .strip_prefix(field)
                    .is_some_and(|after| !after.starts_with(is_name))
        });
        if read {
            return true;
        }
        rest = after;
    }
    false
}

/// An action's code with its comments and quoted text blanked, and the
/// template after its `}}`. A `}}` inside a string doesn't end the action.
fn action_code(action: &str) -> (String, &str) {
    if action
        .trim_start_matches('-')
        .trim_start()
        .starts_with("/*")
    {
        let end = action.find("*/").map_or(action.len(), |at| at + 2);
        let close = action[end..]
            .find("}}")
            .map_or(action.len(), |at| end + at + 2);
        return (String::new(), &action[close..]);
    }
    let mut code = String::new();
    let (mut quote, mut escaped) = (None, false);
    for (at, c) in action.char_indices() {
        match quote {
            Some(open) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' && open != '`' {
                    escaped = true;
                } else if c == open {
                    quote = None;
                }
                code.push(' ');
            }
            None if action[at..].starts_with("}}") => return (code, &action[at + 2..]),
            None => {
                if matches!(c, '"' | '`' | '\'') {
                    quote = Some(c);
                }
                code.push(c);
            }
        }
    }
    (code, "")
}

impl VectorEmbeddingDescription {
    /// Create a new vector embedding description.
    pub fn new(
        field_name: impl Into<String>,
        model: impl Into<String>,
        provider: impl Into<String>,
    ) -> Self {
        Self {
            field_name: field_name.into(),
            fields: Vec::new(),
            model: model.into(),
            provider: provider.into(),
            template: String::new(),
            url: String::new(),
        }
    }

    /// Add source fields for the embedding.
    pub fn with_fields(mut self, fields: Vec<String>) -> Self {
        self.fields = fields;
        self
    }

    /// Set the template.
    pub fn with_template(mut self, template: impl Into<String>) -> Self {
        self.template = template.into();
        self
    }

    /// Whether it embeds `field`, through `fields` or its template.
    pub fn reads(&self, field: &str) -> bool {
        self.fields.iter().any(|source| source == field) || template_reads(&self.template, field)
    }

    /// Set the API URL.
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_builder() {
        let embedding =
            VectorEmbeddingDescription::new("name_embedding", "text-embedding-3-small", "openai")
                .with_fields(vec!["name".to_string(), "description".to_string()])
                .with_url("https://api.openai.com/v1");

        assert_eq!(embedding.field_name, "name_embedding");
        assert_eq!(embedding.model, "text-embedding-3-small");
        assert_eq!(embedding.provider, "openai");
        assert_eq!(embedding.fields.len(), 2);
    }

    #[test]
    fn test_embedding_serialization() {
        let embedding =
            VectorEmbeddingDescription::new("vec_field", "text-embedding-3-small", "openai")
                .with_fields(vec!["name".to_string()]);

        let json = serde_json::to_string(&embedding).unwrap();
        assert!(json.contains("\"FieldName\""));
        assert!(json.contains("\"Model\""));
        assert!(json.contains("\"Provider\""));

        let parsed: VectorEmbeddingDescription = serde_json::from_str(&json).unwrap();
        assert_eq!(embedding, parsed);
    }

    #[test]
    fn a_template_reads_the_fields_it_names() {
        assert!(template_reads("{{ .body }}", "body"));
        assert!(template_reads(
            "Title: {{.title}}\n{{ .body | trim }}",
            "body"
        ));
        assert!(!template_reads("{{ .bodyText }}", "body"));
        assert!(!template_reads("{{ .meta.body }}", "body"));
        assert!(!template_reads("body .body", "body"));
        assert!(!template_reads("", "body"));
    }

    #[test]
    fn a_template_reads_no_field_in_its_comments_or_strings() {
        assert!(!template_reads("{{ .title }} {{/* .body */}}", "body"));
        assert!(!template_reads("{{- /* .body */ -}}", "body"));
        assert!(!template_reads(r#"{{ printf ".body" .title }}"#, "body"));
        assert!(template_reads(r#"{{ printf "%s}}" .body }}"#, "body"));
        assert!(template_reads(r#"{{ printf "a\"}}" .body }}"#, "body"));
        assert!(template_reads("{{/* note */}}{{ .body }}", "body"));
    }
}
