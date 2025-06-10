use async_trait::async_trait;

use crate::{
    domain::{
        entities::template::TemplateType, repositories::template_repository::TemplateRepository,
    },
    shared::error::TemplateError,
};

pub struct MemoryTemplateRepository;

impl MemoryTemplateRepository {
    pub fn new() -> Self {
        Self {}
    }

    pub fn get_supported_templates() -> Vec<TemplateType> {
        vec![
            TemplateType::Database,
            TemplateType::Redis,
            TemplateType::WebServer,
        ]
    }
}

#[async_trait]
impl TemplateRepository for MemoryTemplateRepository {
    async fn get(&self, _path: String) -> Result<TemplateType, TemplateError> {
        Err(TemplateError::NowRepositoryTemplateNotSupportFunction)
    }

    async fn get_all(&self) -> Result<Vec<TemplateType>, TemplateError> {
        Ok(Self::get_supported_templates())
    }

    async fn save(&self, _template: TemplateType, _path: String) -> Result<(), TemplateError> {
        Err(TemplateError::NowRepositoryTemplateNotSupportFunction)
    }

    async fn delete(&self, _path: String) -> Result<(), TemplateError> {
        Err(TemplateError::NowRepositoryTemplateNotSupportFunction)
    }

    async fn update(&self, _template: TemplateType, _path: String) -> Result<(), TemplateError> {
        Err(TemplateError::NowRepositoryTemplateNotSupportFunction)
    }

    async fn get_default_template(
        &self,
        template: TemplateType,
    ) -> Result<TemplateType, TemplateError> {
        Ok(template)
    }
}
