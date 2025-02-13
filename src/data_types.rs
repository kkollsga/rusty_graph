use chrono::{NaiveDateTime, NaiveDate, Utc, TimeZone};
use std::cmp::Ordering;
use pyo3::{prelude::*, exceptions::PyTypeError};
use pyo3::{PyResult, Python, FromPyObject, PyAny};
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum AttributeValue {
    Int(i32),
    Float(f64),
    DateTime(i64), // Timestamp, used for both dates and datetimes
    String(String),
}

impl AttributeValue {
    pub fn to_string(&self) -> String {
        match self {
            AttributeValue::Int(v) => v.to_string(),
            AttributeValue::Float(v) => v.to_string(),
            AttributeValue::DateTime(v) => v.to_string(),
            AttributeValue::String(v) => v.clone(),
        }
    }
    pub fn to_python_object(&self, py: Python, data_type: Option<&str>) -> PyResult<PyObject> {
        match self {
            AttributeValue::Int(v) => match data_type {
                Some("Int") | None => Ok(v.into_py(py)),
                _ => Err(PyTypeError::new_err("Type mismatch for Int value")),
            },
            AttributeValue::Float(v) => match data_type {
                Some("Float") | None => Ok(v.into_py(py)),
                _ => Err(PyTypeError::new_err("Type mismatch for Float value")),
            },
            AttributeValue::DateTime(v) => match data_type {
                Some("DateTime") => {
                    // Convert the timestamp to a Python datetime object and then to a string
                    let datetime_module = PyModule::import(py, "datetime")?;
                    let datetime_class = datetime_module.getattr("datetime")?;
                    let py_timestamp = (*v).into_py(py);
                    let datetime = datetime_class.call_method1("fromtimestamp", (py_timestamp,))?;
                    let datetime_str = datetime.call_method0("isoformat")?; // Convert datetime to ISO format string
                    Ok(datetime_str.into_py(py))
                },
                _ => Err(PyTypeError::new_err("Type mismatch for DateTime value")),
            }
            
            AttributeValue::String(v) => match data_type {
                Some("String") | None => Ok(v.into_py(py)),
                _ => Err(PyTypeError::new_err("Type mismatch for String value")),
            },
        }
    }

    // Convert a NaiveDateTime to AttributeValue::DateTime
    pub fn from_naive_datetime(dt: &NaiveDateTime) -> Self {
        AttributeValue::DateTime(Utc.from_utc_datetime(dt).timestamp())
    }

    // Convert a NaiveDate to AttributeValue::DateTime, setting the time to midnight
    pub fn from_naive_date(d: &NaiveDate) -> PyResult<Self> {
        let datetime_opt = d.and_hms_opt(0, 0, 0); // Returns Option<NaiveDateTime>
        let datetime = datetime_opt.ok_or_else(|| {
            PyTypeError::new_err("Invalid time for date") // Handle the None case by returning an error
        })?;
        Ok(AttributeValue::DateTime(Utc.from_utc_datetime(&datetime).timestamp()))
    }

    pub fn is_null(&self) -> bool {
        match self {
            AttributeValue::Int(v) => *v == 0,  // Consider 0 as null
            AttributeValue::Float(v) => *v == 0.0 || v.is_nan(),  // Consider 0.0 or NaN as null
            AttributeValue::DateTime(v) => *v == 0,  // Consider epoch (0 timestamp) as null
            AttributeValue::String(v) => v.is_empty(),  // Consider empty string as null
        }
    }
}

impl Clone for AttributeValue {
    fn clone(&self) -> Self {
        match self {
            AttributeValue::Int(v) => AttributeValue::Int(*v),
            AttributeValue::Float(v) => AttributeValue::Float(*v),
            AttributeValue::DateTime(v) => AttributeValue::DateTime(*v),
            AttributeValue::String(v) => AttributeValue::String(v.clone()),
        }
    }
}
impl PartialEq for AttributeValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (AttributeValue::Int(a), AttributeValue::Int(b)) => a == b,
            (AttributeValue::Float(a), AttributeValue::Float(b)) => a == b,
            (AttributeValue::DateTime(a), AttributeValue::DateTime(b)) => a == b,
            (AttributeValue::String(a), AttributeValue::String(b)) => a == b,
            _ => false, // Different types are always not equal
        }
    }
}
impl PartialOrd for AttributeValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (AttributeValue::Int(a), AttributeValue::Int(b)) => a.partial_cmp(b),
            (AttributeValue::Float(a), AttributeValue::Float(b)) => a.partial_cmp(b),
            (AttributeValue::DateTime(a), AttributeValue::DateTime(b)) => a.partial_cmp(b),
            // For strings, we'll default to a simple lexicographical comparison
            (AttributeValue::String(a), AttributeValue::String(b)) => a.partial_cmp(b),
            _ => None, // Comparison between different types is undefined
        }
    }
}

impl<'source> FromPyObject<'source> for AttributeValue {
    fn extract(ob: &'source PyAny) -> PyResult<Self> {
        // Try to extract the Python object as different types
        if let Ok(value) = ob.extract::<i32>() {
            return Ok(AttributeValue::Int(value));
        }
        if let Ok(value) = ob.extract::<f64>() {
            return Ok(AttributeValue::Float(value));
        }
        if let Ok(value) = ob.extract::<String>() {
            return Ok(AttributeValue::String(value));
        }
        if let Ok(value) = ob.extract::<i64>() { // Assuming DateTime is represented as a timestamp
            return Ok(AttributeValue::DateTime(value));
        }
        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "Could not extract AttributeValue",
        ))
    }
}