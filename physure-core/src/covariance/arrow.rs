use sprs::CsMat;
use arrow::array::{UInt64Array, UInt32Array, ListBuilder, PrimitiveBuilder, ArrayRef};
use arrow::datatypes::{DataType, Field, Schema, UInt64Type, UInt32Type, Float64Type, Int32Type};
use arrow::record_batch::RecordBatch;
use arrow::ipc::writer::StreamWriter;
use arrow::ipc::reader::StreamReader;
use arrow::array::AsArray;
use std::sync::Arc;
use std::io::Cursor;

use crate::error::{PhysureError, PhysureResult};
use super::store::CovarianceStore;

impl CovarianceStore {
    pub fn to_arrow(&self) -> PhysureResult<Vec<u8>> {
        let mut keys: Vec<_> = self.blocks.keys().collect();
        keys.sort();

        let mut row_ids = Vec::with_capacity(keys.len());
        let mut col_ids = Vec::with_capacity(keys.len());
        let mut shapes_rows = Vec::with_capacity(keys.len());
        let mut shapes_cols = Vec::with_capacity(keys.len());
        
        let mut data_builder = ListBuilder::new(PrimitiveBuilder::<Float64Type>::new());
        let mut indices_builder = ListBuilder::new(PrimitiveBuilder::<Int32Type>::new());
        let mut indptr_builder = ListBuilder::new(PrimitiveBuilder::<Int32Type>::new());

        for &&(r, c) in &keys {
            let mat = &self.blocks[&(r, c)];
            row_ids.push(r);
            col_ids.push(c);
            shapes_rows.push(mat.rows() as u32);
            shapes_cols.push(mat.cols() as u32);
            
            let mat_owned = mat.clone();
            let (indptr, indices, data) = mat_owned.into_raw_storage();
            
            data_builder.values().append_slice(&data);
            data_builder.append(true);
            
            for idx in indices {
                indices_builder.values().append_value(idx as i32);
            }
            indices_builder.append(true);

            for ptr in indptr {
                indptr_builder.values().append_value(ptr as i32);
            }
            indptr_builder.append(true);
        }
        
        let row_id_array = UInt64Array::from(row_ids);
        let col_id_array = UInt64Array::from(col_ids);
        let rows_array = UInt32Array::from(shapes_rows);
        let cols_array = UInt32Array::from(shapes_cols);
        let data_array = data_builder.finish();
        let indices_array = indices_builder.finish();
        let indptr_array = indptr_builder.finish();

        let schema = Schema::new(vec![
            Field::new("row_id", DataType::UInt64, false),
            Field::new("col_id", DataType::UInt64, false),
            Field::new("rows", DataType::UInt32, false),
            Field::new("cols", DataType::UInt32, false),
            Field::new("data", DataType::List(Arc::new(Field::new("item", DataType::Float64, true))), false),
            Field::new("indices", DataType::List(Arc::new(Field::new("item", DataType::Int32, true))), false),
            Field::new("indptr", DataType::List(Arc::new(Field::new("item", DataType::Int32, true))), false),
        ]);

        let batch = RecordBatch::try_new(
            Arc::new(schema),
            vec![
                Arc::new(row_id_array) as ArrayRef,
                Arc::new(col_id_array) as ArrayRef,
                Arc::new(rows_array) as ArrayRef,
                Arc::new(cols_array) as ArrayRef,
                Arc::new(data_array) as ArrayRef,
                Arc::new(indices_array) as ArrayRef,
                Arc::new(indptr_array) as ArrayRef,
            ],
        ).map_err(|e| PhysureError::ArrowError(format!("Arrow error: {}", e)))?;

        let mut buffer = Vec::new();
        {
            let mut writer = StreamWriter::try_new(&mut buffer, &batch.schema())
                .map_err(|e| PhysureError::ArrowError(e.to_string()))?;
            writer.write(&batch).map_err(|e| PhysureError::ArrowError(e.to_string()))?;
            writer.finish().map_err(|e| PhysureError::ArrowError(e.to_string()))?;
        }
        Ok(buffer)
    }

    pub fn to_arrow_bytes(&self) -> PhysureResult<Vec<u8>> {
        self.to_arrow()
    }

    pub fn from_arrow_bytes(&mut self, state: Vec<u8>) -> PhysureResult<()> {
        let cursor = Cursor::new(state);
        let reader = StreamReader::try_new(cursor, None)
            .map_err(|e| PhysureError::ArrowError(format!("Arrow reader error: {}", e)))?;
        
        self.blocks.clear();

        for batch_result in reader {
             let batch = batch_result.map_err(|e| PhysureError::ArrowError(format!("Arrow batch error: {}", e)))?;

             let row_ids = batch.column(0).as_primitive::<UInt64Type>();
             let col_ids = batch.column(1).as_primitive::<UInt64Type>();
             let rows_arr = batch.column(2).as_primitive::<UInt32Type>();
             let cols_arr = batch.column(3).as_primitive::<UInt32Type>();
             
             let data_list = batch.column(4).as_list::<i32>();
             let indices_list = batch.column(5).as_list::<i32>();
             let indptr_list = batch.column(6).as_list::<i32>();

             for i in 0..batch.num_rows() {
                 let r_id = row_ids.value(i);
                 let c_id = col_ids.value(i);
                 let n_rows = rows_arr.value(i) as usize;
                 let n_cols = cols_arr.value(i) as usize;
                 
                 let data_vals: Vec<f64> = data_list.value(i).as_primitive::<Float64Type>().values().to_vec();
                 let indices_vals: Vec<usize> = indices_list.value(i).as_primitive::<Int32Type>().values().iter().map(|&x| x as usize).collect();
                 let indptr_vals: Vec<usize> = indptr_list.value(i).as_primitive::<Int32Type>().values().iter().map(|&x| x as usize).collect();

                 let mat = CsMat::new((n_rows, n_cols), indptr_vals, indices_vals, data_vals);
                 self.blocks.insert((r_id, c_id), mat);
             }
        }
        
        Ok(())
    }
}
