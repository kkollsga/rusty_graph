use pyo3::prelude::*;
use pyo3::types::{PyList, PyDict};
use petgraph::graph::DiGraph;
use std::collections::HashMap;
use chrono::NaiveDateTime;
use crate::graph::get_schema::update_or_retrieve_schema;
use crate::schema::{Node, Relation};
use crate::data_types::AttributeValue;

fn parse_value_to_i32(item: &PyAny) -> Option<i32> {
    if let Ok(int_val) = item.extract::<i64>() {
        return Some(int_val as i32);
    }
    if let Ok(float_val) = item.extract::<f64>() {
        return Some(float_val as i32);
    }
    if let Ok(int_val) = item.extract::<i32>() {
        return Some(int_val);
    }
    if let Ok(s) = item.extract::<String>() {
        if let Ok(num) = s.parse::<i32>() {
            return Some(num);
        }
    }
    None
}

fn update_or_create_node(
    graph: &mut DiGraph<Node, Relation>,
    node_type: &String,
    unique_id: i32,
    node_title: Option<String>,
    attributes: Option<HashMap<String, AttributeValue>>,
    conflict_handling: &String,
) -> usize {
    let existing_node_index = graph.node_indices().find(|&i| match &graph[i] {
        Node::StandardNode {
            node_type: nt,
            unique_id: uid,
            ..
        } => nt == node_type && *uid == unique_id,
        Node::DataTypeNode { .. } => false
    });

    match existing_node_index {
        Some(node_index) => {
            match conflict_handling.as_str() {
                "replace" => {
                    graph[node_index] = Node::new(node_type, unique_id, attributes, node_title.as_deref());
                },
                "update" => {
                    if let Some(attrs) = attributes {
                        if let Node::StandardNode {
                            attributes: node_attrs,
                            ..
                        } = &mut graph[node_index]
                        {
                            for (key, value) in attrs {
                                node_attrs.insert(key, value);
                            }
                        }
                    }
                },
                "skip" => (),
                _ => panic!("Invalid conflict_handling value"),
            }
            node_index.index()
        },
        None => {
            let node = Node::new(node_type, unique_id, attributes, node_title.as_deref());
            graph.add_node(node).index()
        },
    }
}

pub fn add_nodes(
    graph: &mut DiGraph<Node, Relation>,
    data: &PyList,
    columns: Vec<String>,
    node_type: String,
    unique_id_field: String,
    node_title_field: Option<String>,
    conflict_handling: Option<String>,
    column_types: Option<&PyDict>,
    attribute_columns: Option<Vec<String>>,
) -> PyResult<()> {
    let conflict_handling = conflict_handling.unwrap_or_else(|| "update".to_string());
    let default_datetime_format = "%Y-%m-%d %H:%M:%S".to_string();

    // Convert available columns to lowercase for case-insensitive comparison
    let available_columns: Vec<String> = columns.iter()
        .map(|s| s.to_lowercase())
        .collect();

    // Validate unique_id_field
    if !available_columns.contains(&unique_id_field.to_lowercase()) {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            format!(
                "Unique ID column '{}' not found in data. Valid columns are: [{}]",
                unique_id_field,
                columns.join(", ")
            )
        ));
    }

    // Validate node_title_field if provided
    if let Some(title_field) = &node_title_field {
        if !available_columns.contains(&title_field.to_lowercase()) {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!(
                    "Title column '{}' not found in data. Valid columns are: [{}]",
                    title_field,
                    columns.join(", ")
                )
            ));
        }
    }

    // Validate attribute_columns if provided
    let attribute_columns = if let Some(attr_cols) = attribute_columns {
        // Find invalid columns
        let invalid_cols: Vec<String> = attr_cols.iter()
            .filter(|col| !available_columns.contains(&col.to_lowercase()))
            .cloned()
            .collect();
                
        if !invalid_cols.is_empty() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!(
                    "{} not found in data. Valid columns are: [{}]",
                    invalid_cols.join(", "),
                    columns.join(", ")
                )
            ));
        }
        
        Some(attr_cols)
    } else {
        None
    };

    // Infer types from first row if no column_types provided
    let mut inferred_types = HashMap::new();
    if column_types.is_none() {
        if let Ok(first_row) = data.get_item(0).and_then(|r| r.extract::<Vec<&PyAny>>()) {
            for (i, col) in columns.iter().enumerate() {
                if let Some(item) = first_row.get(i) {
                    let type_str = if item.extract::<i64>().is_ok() || item.extract::<i32>().is_ok() {
                        "Int"
                    } else if item.extract::<f64>().is_ok() {
                        "Float"
                    } else {
                        "String"
                    };
                    inferred_types.insert(col.clone(), type_str.to_string());
                }
            }
        }
    }

    let mut column_types_map = match column_types {
        Some(ct) => ct.extract().unwrap_or_default(),
        None => inferred_types,
    };

    let datetime_formats = if !column_types_map.is_empty() {
        extract_datetime_formats(&mut column_types_map, &default_datetime_format)
    } else {
        HashMap::new()
    };

    let schema = update_or_retrieve_schema(
        graph,
        "Node",
        &node_type,
        Some(column_types_map.clone())
    )?;

    'row_loop: for row in data.iter() {
        let row: Vec<&PyAny> = match row.extract() {
            Ok(r) => r,
            Err(_) => {
                continue 'row_loop;
            }
        };
        let mut attributes: HashMap<String, AttributeValue> = HashMap::new();
        let mut unique_id: Option<i32> = None;
        let mut node_title: Option<String> = None;

        for (col_index, column_name) in columns.iter().enumerate() {
            let item = match row.get(col_index) {
                Some(i) => i,
                None => {
                    continue 'row_loop;
                }
            };

            if column_name == &unique_id_field {
                unique_id = parse_value_to_i32(item);
                if unique_id.is_none() {
                    continue 'row_loop;
                }
                continue;
            }

            if let Some(ref title_field) = node_title_field {
                if column_name == title_field {
                    node_title = match item.extract() {
                        Ok(title) => Some(title),
                        Err(_) => {
                            None
                        }
                    };
                    continue;
                }
            }

            // Check if column should be processed as attribute
            if let Some(ref attr_cols) = attribute_columns {
                if !attr_cols.iter().any(|col| col.eq_ignore_ascii_case(column_name)) {
                    continue;
                }
            }

            let data_type = schema.get(column_name).map_or("String", String::as_str);
            let attribute_value = match data_type {
                "Int" => {
                    if let Ok(value) = item.extract::<i64>() {
                        Some(AttributeValue::Int(value as i32))
                    } else if let Ok(value) = item.extract::<i32>() {
                        Some(AttributeValue::Int(value))
                    } else if let Ok(value) = item.extract::<f64>() {
                        Some(AttributeValue::Int(value as i32))
                    } else {
                        None
                    }
                },
                "Float" => {
                    if let Ok(value) = item.extract::<f64>() {
                        Some(AttributeValue::Float(value))
                    } else if let Ok(value) = item.extract::<i64>() {
                        Some(AttributeValue::Float(value as f64))
                    } else {
                        None
                    }
                },
                "DateTime" => {
                    let format = datetime_formats.get(column_name).unwrap_or(&default_datetime_format);
                    if let Ok(ts) = item.extract::<i64>() {
                        Some(AttributeValue::DateTime(ts))
                    } else if let Ok(datetime_str) = item.extract::<String>() {
                        match NaiveDateTime::parse_from_str(&datetime_str, format) {
                            Ok(dt) => Some(AttributeValue::DateTime(dt.and_utc().timestamp())),
                            Err(_) => {
                                None
                            }
                        }
                    } else {
                        None
                    }
                },
                _ => Some(AttributeValue::String(item.extract::<String>().unwrap_or_default())),
            };

            if let Some(value) = attribute_value {
                attributes.insert(column_name.clone(), value);
            }
        }

        let unique_id = match unique_id {
            Some(id) => id,
            None => {
                continue;
            }
        };

        update_or_create_node(
            graph,
            &node_type,
            unique_id,
            node_title,
            Some(attributes),
            &conflict_handling,
        );
    }

    Ok(())
}

fn extract_datetime_formats(column_types_map: &mut HashMap<String, String>, default_datetime_format: &str) -> HashMap<String, String> {
    let mut datetime_formats: HashMap<String, String> = HashMap::new();

    for (column, data_type) in column_types_map.iter() {
        let parts: Vec<&str> = data_type.splitn(2, ' ').collect();

        if parts[0] == "DateTime" {
            let format = parts.get(1).unwrap_or(&default_datetime_format);
            datetime_formats.insert(column.clone(), format.to_string());
        }
    }

    for (_column, data_type) in column_types_map.iter_mut() {
        if data_type.starts_with("DateTime") {
            *data_type = "DateTime".to_string();
        }
    }

    datetime_formats
}