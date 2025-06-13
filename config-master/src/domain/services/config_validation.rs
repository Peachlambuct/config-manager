use crate::domain::entities::{configuration::Config, validation_rule::{Validation, ValidationConfig, ValidationResult}};

pub struct ConfigValidationService;

impl ConfigValidationService {
    pub fn validate_with_rules(
        validation: Validation,
        config: Config,
    ) -> ValidationResult {
        let validation_config = ValidationConfig::new(validation, config);
        validation_config.validate()
    }
}

