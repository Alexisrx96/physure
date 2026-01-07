use pyo3::prelude::*;
use pyo3::types::PyList;
use pyo3::{Bound, PyResult};
use std::collections::HashMap;
use arrow::array::{Float64Array, StringArray, ArrayRef, Array};
use arrow::record_batch::RecordBatch;
use arrow::datatypes::{DataType, Field, Schema};
use std::sync::Arc;

use crate::quantity::QuantityInner;
use crate::units::RationalUnit;

#[pyfunction]
pub fn to_arrow_record_batch(quantities: Bound<'_, PyList>) -> PyResult<Vec<u8>> {
    let len = quantities.len();
    let mut means = Vec::with_capacity(len);
    let mut std_devs = Vec::with_capacity(len);
    
    // Use Dictionary encoding for units (very efficient for repeated units)
    let mut unit_values = Vec::new();
    let mut unit_indices = Vec::with_capacity(len);
    let mut unit_repr_cache: HashMap<RationalUnit, u32> = HashMap::new();

    for q_any in quantities.iter() {
        // Optimization: downcast and borrow instead of extract (zero allocation)
        let q_bound = q_any.downcast::<QuantityInner>()?;
        let q = q_bound.borrow();
        
        means.push(q.mean());
        std_devs.push(q.std_dev());
        
        // Use the RationalUnit itself as the cache key (efficient Hash/Eq)
        let idx = *unit_repr_cache.entry(q.unit.clone()).or_insert_with(|| {
            let i = unit_values.len() as u32;
            unit_values.push(q.unit.__repr__());
            i
        });
        unit_indices.push(Some(idx));
    }

    let mean_array = Float64Array::from(means);
    let std_dev_array = Float64Array::from(std_devs);
    
    // Build DictionaryArray for units (reduces memory and overhead)
    let keys = arrow::array::UInt32Array::from(unit_indices);
    let values = StringArray::from(unit_values);
    let unit_array = arrow::array::DictionaryArray::<arrow::datatypes::UInt32Type>::try_new(
        keys,
        Arc::new(values) as ArrayRef
    ).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Arrow dict error: {}", e)))?;

    let schema = Schema::new(vec![
        Field::new("mean", DataType::Float64, false),
        Field::new("std_dev", DataType::Float64, false),
        Field::new("unit", unit_array.data_type().clone(), false),
    ]);

    let batch = RecordBatch::try_new(
        Arc::new(schema),
        vec![
            Arc::new(mean_array) as ArrayRef,
            Arc::new(std_dev_array) as ArrayRef,
            Arc::new(unit_array) as ArrayRef,
        ],
    ).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Arrow error: {}", e)))?;

    let mut buffer = Vec::new();
    {
        let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut buffer, &batch.schema()).unwrap();
        writer.write(&batch).unwrap();
        writer.finish().unwrap();
    }
    Ok(buffer)
}
