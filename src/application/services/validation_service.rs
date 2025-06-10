use tracing::debug;

use crate::{
    domain::entities::{
        configuration::{Config, ConfigValue},
        validation_rule::{FieldType, Validation},
    },
    shared::error::ConfigError,
};

pub struct ValidationService;

impl ValidationService {
    pub fn get_validation_by_config(config: &Config) -> Result<Validation, ConfigError> {
        let mut validation = Validation::default();
        if let Some(field) = config.get("required_fields") {
            if let ConfigValue::Array(array) = field {
                validation.required_fields = array
                    .iter()
                    .map(|v| {
                        if let ConfigValue::String(s) = v {
                            s.to_string()
                        } else {
                            "".to_string()
                        }
                    })
                    .collect::<Vec<String>>();
            }
        }
        debug!("required_fields: {:?}", validation.required_fields);

        if let Some(field) = config.get("field_types") {
            if let ConfigValue::Object(object) = field {
                validation.field_types = object
                    .iter()
                    .map(|(k, v)| {
                        debug!("k: {k}, v: {:?}", v);
                        let mut field_type = FieldType::String {
                            max_length: None,
                            min_length: None,
                        };
                        if let ConfigValue::Object(object) = v {
                            let mut max_length_parse = None;
                            if let Some(max_length) = object.get("max") {
                                if let ConfigValue::Number(max_length) = max_length {
                                    max_length_parse =
                                        Some(max_length.to_string().parse::<usize>().unwrap());
                                }
                            }
                            let mut min_length_parse = None;
                            if let Some(min_length) = object.get("min") {
                                if let ConfigValue::Number(min_length) = min_length {
                                    min_length_parse =
                                        Some(min_length.to_string().parse::<usize>().unwrap());
                                }
                            }
                            let mut min_parse = None;
                            if let Some(min) = object.get("min") {
                                if let ConfigValue::Number(min) = min {
                                    min_parse = Some(min.to_string().parse::<f64>().unwrap());
                                }
                            }
                            let mut max_parse = None;
                            if let Some(max) = object.get("max") {
                                if let ConfigValue::Number(max) = max {
                                    max_parse = Some(max.to_string().parse::<f64>().unwrap());
                                }
                            }

                            field_type = match object.get("type").unwrap().to_string().as_str() {
                                "string" => FieldType::String {
                                    max_length: max_length_parse,
                                    min_length: min_length_parse,
                                },
                                "number" => FieldType::Number {
                                    min: min_parse,
                                    max: max_parse,
                                },
                                "boolean" => FieldType::Boolean,
                                _ => FieldType::String {
                                    max_length: max_length_parse,
                                    min_length: min_length_parse,
                                },
                            };
                        }
                        (k.to_string(), field_type)
                    })
                    .collect();
            }
        }

        Ok(validation)
    }
}
