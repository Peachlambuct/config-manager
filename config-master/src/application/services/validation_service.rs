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
                        
                        if let ConfigValue::Object(field_config) = v {
                            // 先获取类型
                            let field_type_str = field_config
                                .get("type")
                                .and_then(|t| t.as_string())
                                .map(|s| s.as_str())
                                .unwrap_or("string");
                            
                            debug!("Field {} type: {}", k, field_type_str);
                            
                            // 根据类型解析对应的约束
                            let field_type = match field_type_str {
                                "string" => {
                                    let max_length = field_config
                                        .get("max")
                                        .and_then(|v| v.as_number())
                                        .map(|n| n as usize);
                                        
                                    let min_length = field_config
                                        .get("min")
                                        .and_then(|v| v.as_number())
                                        .map(|n| n as usize);
                                        
                                    debug!("String constraints - min: {:?}, max: {:?}", min_length, max_length);
                                    
                                    FieldType::String {
                                        max_length,
                                        min_length,
                                    }
                                }
                                "number" => {
                                    let min = field_config
                                        .get("min")
                                        .and_then(|v| v.as_number());
                                        
                                    let max = field_config
                                        .get("max")
                                        .and_then(|v| v.as_number());
                                        
                                    debug!("Number constraints - min: {:?}, max: {:?}", min, max);
                                    
                                    FieldType::Number { min, max }
                                }
                                "boolean" => FieldType::Boolean,
                                _ => {
                                    debug!("Unknown field type: {}, defaulting to String", field_type_str);
                                    FieldType::String {
                                        max_length: None,
                                        min_length: None,
                                    }
                                }
                            };
                            
                            debug!("Final field_type for {}: {:?}", k, field_type);
                            (k.to_string(), field_type)
                        } else {
                            debug!("Invalid field config for {}: not an object", k);
                            (k.to_string(), FieldType::String {
                                max_length: None,
                                min_length: None,
                            })
                        }
                    })
                    .collect();
            }
        }

        Ok(validation)
    }
}
