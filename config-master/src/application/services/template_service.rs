use crate::{
    domain::{
        entities::template::TemplateType, repositories::template_repository::TemplateRepository,
    },
    shared::error::TemplateError,
};

pub struct TemplateService {
    pub template_repository: Box<dyn TemplateRepository>,
}

impl TemplateService {
    pub fn new(template_repository: Box<dyn TemplateRepository>) -> Self {
        Self {
            template_repository,
        }
    }

    pub async fn write_template(
        &self,
        template: TemplateType,
        format: String,
    ) -> Result<(), TemplateError> {
        self.template_repository
            .write_template_by_type_and_format(template, format)
            .await
    }
}
