use std::collections::HashMap;
use arrow::array::{Float64Array, StringArray, ArrayRef, Array};
use arrow::record_batch::RecordBatch;
use arrow::datatypes::{DataType, Field, Schema};
use std::sync::Arc;

use crate::quantity::Quantity;
use crate::units::RationalUnit;

pub fn quantities_to_arrow(quantities: &[Quantity]) -> Result<Vec<u8>, String> {
    let len = quantities.len();
    let mut means = Vec::with_capacity(len);
    let mut std_devs = Vec::with_capacity(len);
    
    let mut unit_values = Vec::new();
    let mut unit_indices = Vec::with_capacity(len);
    let mut unit_repr_cache: HashMap<RationalUnit, u32> = HashMap::new();

    for q in quantities {
        means.push(q.value.mean());
        std_devs.push(q.value.std_dev());
        
        let idx = *unit_repr_cache.entry(q.unit.clone()).or_insert_with(|| {
            let i = unit_values.len() as u32;
            unit_values.push(q.unit.__repr__());
            i
        });
        unit_indices.push(Some(idx));
    }

    let mean_array = Float64Array::from_iter_values(means);
    let std_dev_array = Float64Array::from_iter_values(std_devs);
    
    let keys = arrow::array::UInt32Array::from(unit_indices);
    let values = StringArray::from(unit_values);
    let unit_array = arrow::array::DictionaryArray::<arrow::datatypes::UInt32Type>::try_new(
        keys,
        Arc::new(values) as ArrayRef
    ).map_err(|e| format!("Arrow dict error: {}", e))?;

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
    ).map_err(|e| format!("Arrow error: {}", e))?;

    let mut buffer = Vec::new();
    {
        let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut buffer, &batch.schema())
            .map_err(|e| e.to_string())?;
        writer.write(&batch).map_err(|e| e.to_string())?;
        writer.finish().map_err(|e| e.to_string())?;
    }
    Ok(buffer)
}
