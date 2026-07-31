use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionConfig {
    pub name: String,
    pub pattern: ProjectionPattern,
    pub source_stores: Vec<String>,
    pub target_key: String,
    pub ttl_seconds: u64,
    pub copy_fields: Option<Vec<CopyField>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyField {
    pub source_store: String,
    pub source_entity: String,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProjectionPattern {
    R,
    M,
}

impl ProjectionPattern {
    pub fn as_str(&self) -> &str {
        match self {
            ProjectionPattern::R => "R",
            ProjectionPattern::M => "M",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProjectionRegistry {
    projections: Vec<ProjectionConfig>,
}

impl ProjectionRegistry {
    pub fn new(projections: Vec<ProjectionConfig>) -> Self {
        Self { projections }
    }

    pub fn len(&self) -> usize {
        self.projections.len()
    }

    pub fn is_empty(&self) -> bool {
        self.projections.is_empty()
    }

    pub fn projections(&self) -> &[ProjectionConfig] {
        &self.projections
    }

    pub fn get(&self, name: &str) -> Option<&ProjectionConfig> {
        self.projections.iter().find(|p| p.name == name)
    }

    pub fn for_store(&self, store: &str) -> Vec<&ProjectionConfig> {
        self.projections
            .iter()
            .filter(|p| p.source_stores.contains(&store.to_string()))
            .collect()
    }

    pub fn pattern_r_projections(&self) -> Vec<&ProjectionConfig> {
        self.projections
            .iter()
            .filter(|p| p.pattern == ProjectionPattern::R)
            .collect()
    }

    pub fn pattern_m_projections(&self) -> Vec<&ProjectionConfig> {
        self.projections
            .iter()
            .filter(|p| p.pattern == ProjectionPattern::M)
            .collect()
    }

    pub fn validate(&self) -> std::result::Result<(), Vec<String>> {
        let mut errors = Vec::new();

        for proj in &self.projections {
            if proj.name.is_empty() {
                errors.push("projection name must not be empty".into());
            }
            if proj.source_stores.is_empty() {
                errors.push(format!(
                    "projection '{}': at least one source_store required",
                    proj.name
                ));
            }
            if proj.source_stores.len() > 2 {
                errors.push(format!(
                    "projection '{}': max 2 source_stores allowed",
                    proj.name
                ));
            }
            if proj.target_key.is_empty() {
                errors.push(format!(
                    "projection '{}': target_key must not be empty",
                    proj.name
                ));
            }
            if proj.ttl_seconds == 0 {
                errors.push(format!(
                    "projection '{}': ttl_seconds must be positive",
                    proj.name
                ));
            }
            if proj.pattern == ProjectionPattern::M && proj.copy_fields.is_none() {
                errors.push(format!(
                    "projection '{}': pattern M requires copy_fields",
                    proj.name
                ));
            }
            if let Some(fields) = &proj.copy_fields {
                for field in fields {
                    if field.source_store.is_empty() {
                        errors.push(format!(
                            "projection '{}': copy_field source_store must not be empty",
                            proj.name
                        ));
                    }
                    if field.fields.is_empty() {
                        errors.push(format!(
                            "projection '{}': copy_field must specify at least one field",
                            proj.name
                        ));
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_r_projection() -> ProjectionConfig {
        ProjectionConfig {
            name: "pedidos_con_usuario".into(),
            pattern: ProjectionPattern::R,
            source_stores: vec!["pedidos".into(), "usuarios".into()],
            target_key: "pedido_id".into(),
            ttl_seconds: 300,
            copy_fields: None,
        }
    }

    fn make_m_projection() -> ProjectionConfig {
        ProjectionConfig {
            name: "usuarios_materializados".into(),
            pattern: ProjectionPattern::M,
            source_stores: vec!["usuarios".into()],
            target_key: "usuario_id".into(),
            ttl_seconds: 600,
            copy_fields: Some(vec![CopyField {
                source_store: "usuarios".into(),
                source_entity: "usuario".into(),
                fields: vec!["nombre".into(), "email".into()],
            }]),
        }
    }

    #[test]
    fn registry_finds_by_name() {
        let reg = ProjectionRegistry::new(vec![make_r_projection(), make_m_projection()]);

        assert_eq!(reg.len(), 2);
        assert!(reg.get("pedidos_con_usuario").is_some());
        assert!(reg.get("usuarios_materializados").is_some());
        assert!(reg.get("inexistente").is_none());
    }

    #[test]
    fn for_store_filters_correctly() {
        let reg = ProjectionRegistry::new(vec![make_r_projection(), make_m_projection()]);

        let pedidos_projs = reg.for_store("pedidos");
        assert_eq!(pedidos_projs.len(), 1);
        assert_eq!(pedidos_projs[0].name, "pedidos_con_usuario");

        let usuarios_projs = reg.for_store("usuarios");
        assert_eq!(usuarios_projs.len(), 2);
    }

    #[test]
    fn pattern_r_projections() {
        let reg = ProjectionRegistry::new(vec![make_r_projection(), make_m_projection()]);

        let r_projs = reg.pattern_r_projections();
        assert_eq!(r_projs.len(), 1);
        assert_eq!(r_projs[0].name, "pedidos_con_usuario");
    }

    #[test]
    fn pattern_m_projections() {
        let reg = ProjectionRegistry::new(vec![make_r_projection(), make_m_projection()]);

        let m_projs = reg.pattern_m_projections();
        assert_eq!(m_projs.len(), 1);
        assert_eq!(m_projs[0].name, "usuarios_materializados");
    }

    #[test]
    fn pattern_m_requires_copy_fields() {
        let proj = ProjectionConfig {
            name: "bad".into(),
            pattern: ProjectionPattern::M,
            source_stores: vec!["pedidos".into()],
            target_key: "k".into(),
            ttl_seconds: 300,
            copy_fields: None,
        };

        let reg = ProjectionRegistry::new(vec![proj]);
        assert!(reg.validate().is_err());
    }

    #[test]
    fn empty_source_stores_rejected() {
        let proj = ProjectionConfig {
            name: "bad".into(),
            pattern: ProjectionPattern::R,
            source_stores: vec![],
            target_key: "k".into(),
            ttl_seconds: 300,
            copy_fields: None,
        };

        let reg = ProjectionRegistry::new(vec![proj]);
        assert!(reg.validate().is_err());
    }

    #[test]
    fn valid_registry_passes_validation() {
        let reg = ProjectionRegistry::new(vec![make_r_projection(), make_m_projection()]);
        assert!(reg.validate().is_ok());
    }
}
