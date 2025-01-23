use pyo3::prelude::*;
use pyo3::types::PyDict;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use petgraph::visit::EdgeRef;
use crate::schema::{Node, Relation};
use crate::data_types::AttributeValue;
use std::collections::HashMap;

pub fn filter_nodes(
    graph: &DiGraph<Node, Relation>,
    indices: Option<Vec<usize>>,
    filter_dict: &PyDict,
) -> PyResult<Vec<usize>> {
    let mut result = Vec::new();
    let nodes_to_check = match indices {
        Some(idx) => idx.into_iter().map(NodeIndex::new).collect::<Vec<_>>(),
        None => graph.node_indices().collect(),
    };

    let mut filters = HashMap::new();
    for (key, value) in filter_dict.iter() {
        let key = key.extract::<String>()?;
        let value = value.extract::<String>()?;
        filters.insert(key, value);
    }

    for idx in nodes_to_check {
        if let Some(Node::StandardNode { node_type, unique_id, attributes, title }) = graph.node_weight(idx) {
            let mut matches = true;

            for (key, value) in &filters {
                let matches_filter = match key.as_str() {
                    "type" | "node_type" => node_type == value,
                    "title" => title.as_ref().map_or(false, |t| t == value),
                    "unique_id" => unique_id.to_string() == *value,
                    _ => attributes.get(key).map_or(false, |attr| match attr {
                        AttributeValue::String(s) => s == value,
                        AttributeValue::Int(i) => i.to_string() == *value,
                        AttributeValue::Float(f) => f.to_string() == *value,
                        AttributeValue::DateTime(dt) => dt.to_string() == *value,
                    }),
                };

                if !matches_filter {
                    matches = false;
                    break;
                }
            }

            if matches {
                result.push(idx.index());
            }
        }
    }

    Ok(result)
}

pub fn traverse_relationships(
    graph: &DiGraph<Node, Relation>,
    indices: Vec<usize>,
    relationship_type: &str,
    incoming: bool,
    sort_attribute: Option<&str>,
    ascending: Option<bool>,
    max_relations: Option<usize>,
) -> Vec<usize> {
    let mut connected_nodes = Vec::new();
    let direction = if incoming { Direction::Incoming } else { Direction::Outgoing };

    for idx in indices {
        let node_idx = NodeIndex::new(idx);
        for edge in graph.edges_directed(node_idx, direction) {
            if edge.weight().relation_type == relationship_type {
                let connected_idx = if incoming { edge.source() } else { edge.target() };
                
                if let (Some(attr), Some(Node::StandardNode { attributes, .. })) = 
                    (sort_attribute, graph.node_weight(connected_idx)) {
                    connected_nodes.push((connected_idx.index(), attributes.get(attr).cloned()));
                } else {
                    connected_nodes.push((connected_idx.index(), None));
                }
            }
        }
    }

    if let Some(_) = sort_attribute {
        connected_nodes.sort_by(|a, b| match (&a.1, &b.1) {
            (Some(a_val), Some(b_val)) => a_val.partial_cmp(b_val).unwrap_or(std::cmp::Ordering::Equal),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });

        if !ascending.unwrap_or(true) {
            connected_nodes.reverse();
        }
    }

    let result: Vec<_> = connected_nodes.into_iter()
        .map(|(idx, _)| idx)
        .take(max_relations.unwrap_or(usize::MAX))
        .collect();

    result
}

pub fn get_node_data(
    graph: &DiGraph<Node, Relation>,
    indices: Vec<usize>,
    attributes: Option<Vec<String>>,
) -> PyResult<Vec<HashMap<String, PyObject>>> {
    let py = unsafe { Python::assume_gil_acquired() };
    let mut result = Vec::new();

    for idx in indices {
        if let Some(Node::StandardNode { node_type, unique_id, attributes: node_attrs, title }) = graph.node_weight(NodeIndex::new(idx)) {
            let mut node_data = HashMap::new();
            
            if attributes.is_none() || attributes.as_ref().unwrap().contains(&"node_type".to_string()) {
                node_data.insert("node_type".to_string(), node_type.clone().into_py(py));
            }
            if attributes.is_none() || attributes.as_ref().unwrap().contains(&"unique_id".to_string()) {
                node_data.insert("unique_id".to_string(), unique_id.to_string().into_py(py));
            }
            if let Some(title) = title {
                if attributes.is_none() || attributes.as_ref().unwrap().contains(&"title".to_string()) {
                    node_data.insert("title".to_string(), title.clone().into_py(py));
                }
            }

            match &attributes {
                Some(attr_list) => {
                    for attr in attr_list {
                        if let Some(value) = node_attrs.get(attr) {
                            node_data.insert(attr.clone(), value.to_python_object(py, None)?);
                        }
                    }
                }
                None => {
                    for (key, value) in node_attrs {
                        node_data.insert(key.clone(), value.to_python_object(py, None)?);
                    }
                }
            }

            result.push(node_data);
        }
    }

    Ok(result)
}