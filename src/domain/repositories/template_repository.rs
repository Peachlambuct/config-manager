use async_trait::async_trait;
use crate::{domain::entities::template::TemplateType, shared::error::TemplateError};

#[async_trait]
pub trait TemplateRepository {
    async fn get(&self, path: String) -> Result<TemplateType, TemplateError>;
    async fn get_all(&self) -> Result<Vec<TemplateType>, TemplateError>;
    async fn save(&self, template: TemplateType, path: String) -> Result<(), TemplateError>;
    async fn delete(&self, path: String) -> Result<(), TemplateError>;
    async fn update(&self, template: TemplateType, path: String) -> Result<(), TemplateError>;
    async fn get_default_template(&self, template: TemplateType) -> Result<TemplateType, TemplateError>;
}