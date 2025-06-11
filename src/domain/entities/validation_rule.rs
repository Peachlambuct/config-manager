use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
};

use tracing::{debug, info};

use crate::shared::error::ValidationError;

use super::configuration::Config;

// 将ValidationRule定义为类型别名，而不是trait
pub type ValidationRule = dyn Fn(&Config) -> Result<(), ValidationError> + Send + Sync + 'static;

#[derive(Default)]
pub struct Validation {
    pub required_fields: Vec<String>,
    pub field_types: HashMap<String, FieldType>, // 字段类型约束
    pub custom_rules: Vec<Box<ValidationRule>>,  // 自定义规则
}

impl std::fmt::Debug for Validation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Validation")
            .field("required_fields", &self.required_fields)
            .field("field_types", &self.field_types)
            .field(
                "custom_rules",
                &format!("[{} rules]", self.custom_rules.len()),
            )
            .finish()
    }
}

impl Validation {
    pub fn new() -> Self {
        Self {
            required_fields: vec![],
            field_types: HashMap::new(),
            custom_rules: vec![],
        }
    }

    pub fn require_field(mut self, fields: &str) -> Self {
        self.required_fields.push(fields.to_string());
        self
    }

    pub fn field_type(mut self, field: &str, field_type: FieldType) -> Self {
        self.field_types.insert(field.to_string(), field_type);
        self
    }

    pub fn custom_rule(mut self, rule: Box<ValidationRule>) -> Self {
        self.custom_rules.push(rule);
        self
    }
}

#[derive(Debug, Clone)]
pub enum FieldType {
    String {
        max_length: Option<usize>,
        min_length: Option<usize>,
    },
    Number {
        min: Option<f64>,
        max: Option<f64>,
    },
    Boolean,
}

impl Display for FieldType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldType::String {
                max_length,
                min_length,
            } => {
                write!(f, "String(max: {:?}, min: {:?})", max_length, min_length)
            }
            FieldType::Number { min, max } => {
                write!(f, "Number(min: {:?}, max: {:?})", min, max)
            }
            FieldType::Boolean => write!(f, "Boolean"),
        }
    }
}

#[derive(Debug)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
}

pub struct ValidationConfig {
    pub validation: Validation,
    pub config: Config,
}

impl ValidationConfig {
    pub fn new(validation: Validation, config: Config) -> Self {
        Self { validation, config }
    }

    pub fn validate(&self) -> ValidationResult {
        let mut errors: Vec<ValidationError> = Vec::new();

        info!("validation: {:?}", self.validation);

        for field in self.validation.required_fields.iter() {
            let value = self.config.get(field);
            debug!("field: {}, value: {:?}", field, value);
            if value.is_none() {
                errors.push(ValidationError::RequiredField {
                    field: field.clone(),
                });
            }
        }

        for (field, field_type) in self.validation.field_types.iter() {
            let value = self.config.get(field);
            debug!("field: {}, value: {:?}", field, value);
            if let Some(value) = value {
                info!("field_type: {:?}", field_type);
                match field_type {
                    FieldType::String {
                        max_length,
                        min_length,
                    } => {
                        if let Some(max_length) = max_length {
                            if let Some(len) = value.len() {
                                if len > *max_length {
                                    debug!("len: {}, max_length: {}", len, max_length);

                                    errors.push(ValidationError::TypeMismatch {
                                        field: field.clone(),
                                        expected: field_type.to_string(),
                                        actual: value.to_string(),
                                    });
                                }
                            }
                        }
                        if let Some(min_length) = min_length {
                            if let Some(len) = value.len() {
                                if len < *min_length {
                                    debug!("len: {}, min_length: {}", len, min_length);

                                    errors.push(ValidationError::TypeMismatch {
                                        field: field.clone(),
                                        expected: field_type.to_string(),
                                        actual: value.to_string(),
                                    });
                                }
                            }
                        }
                    }
                    FieldType::Number { min, max } => {
                        info!("value.as_number(): {:?}", value.as_number());

                        if let Some(num_value) = value.as_number() {
                            debug!("num_value: {}", num_value);

                            // 检查最小值
                            if let Some(min) = min {
                                debug!("num_value: {}, min: {}", num_value, min);
                                if num_value < *min {
                                    errors.push(ValidationError::TypeMismatch {
                                        field: field.clone(),
                                        expected: field_type.to_string(),
                                        actual: value.to_string(),
                                    });
                                }
                            }

                            // 检查最大值
                            if let Some(max) = max {
                                debug!("num_value: {}, max: {}", num_value, max);
                                if num_value > *max {
                                    errors.push(ValidationError::TypeMismatch {
                                        field: field.clone(),
                                        expected: field_type.to_string(),
                                        actual: value.to_string(),
                                    });
                                }
                            }
                        } else {
                            debug!("as_number() returned None for value: {:?}", value);
                            errors.push(ValidationError::TypeMismatch {
                                field: field.clone(),
                                expected: "number".to_string(),
                                actual: format!("{:?}", value),
                            });
                        }
                    }
                    FieldType::Boolean => {
                        if value != "true" && value != "false" {
                            errors.push(ValidationError::TypeMismatch {
                                field: field.clone(),
                                expected: field_type.to_string(),
                                actual: value.to_string(),
                            });
                        }
                    }
                }
            } else {
                errors.push(ValidationError::UndefinedField {
                    field: field.clone(),
                });
            }
        }

        for rule in self.validation.custom_rules.iter() {
            if let Err(e) = rule(&self.config) {
                errors.push(e);
            }
        }

        ValidationResult {
            is_valid: errors.is_empty(),
            errors,
        }
    }
}
