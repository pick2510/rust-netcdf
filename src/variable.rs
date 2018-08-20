use std::marker::Sized;
use std::ffi;
use std::collections::HashMap;
use netcdf_sys::*;
use dimension::Dimension;
use group::PutAttr;
use attribute::{init_attributes, Attribute};
use string_from_c_str;
use NC_ERRORS;
use std::error::Error;
use ndarray::{ArrayD};
use libc;

macro_rules! get_var_as_type {
    ( $me:ident, $nc_type:ident, $vec_type:ty, $nc_fn:ident , $cast:ident ) 
        => 
    {{
        if (!$cast) && ($me.vartype != $nc_type) {
            return Err("Types are not equivalent and cast==false".to_string());
        }
        let mut buf: Vec<$vec_type> = Vec::with_capacity($me.len as usize);
        let err: i32;
        unsafe {
            let _g = libnetcdf_lock.lock().unwrap();
            buf.set_len($me.len as usize);
            err = $nc_fn($me.grp_id, $me.id, buf.as_mut_ptr());
        }
        if err != NC_NOERR {
            return Err(NC_ERRORS.get(&err).unwrap().clone());
        }
        Ok(buf)
    }};
}

/// This trait allow an implicit cast when fetching 
/// a netCDF variable
pub trait Numeric {
    /// Returns the whole variable as Vec<Self>
    fn from_variable(variable: &Variable) -> Result<Vec<Self>, String>
        where Self: Sized;
    /// Read the variable into a buffer and update its length.
    fn read_variable_into_buffer(variable: &Variable, buffer: &mut Vec<Self>) -> Result<(), String>
        where Self: Sized;
    /// Read a slice of a variable into a buffer and update its length.
    fn read_slice_into_buffer(variable: &Variable, indices: &[usize], slice_len: &[usize], buffer: &mut Vec<Self>) -> Result<(), String>
        where Self: Sized;
    /// Returns a slice of the variable as Vec<Self>
    fn slice_from_variable(variable: &Variable, indices: &[usize], slice_len: &[usize]) -> Result<Vec<Self>, String>
        where Self: Sized;
    /// Returns a single indexed value of the variable as Self
    fn single_value_from_variable(variable: &Variable, indices: &[usize]) -> Result<Self, String>
        where Self: Sized;
    /// Put a single value into a netCDF variable
    fn put_value_at(variable: &mut Variable, indices: &[usize], value: Self) -> Result<(), String>
        where Self: Sized;
    /// put a SLICE of values into a netCDF variable at the given index
    fn put_values_at(variable: &mut Variable, indices: &[usize], slice_len: &[usize], values: &[Self]) -> Result<(), String>
        where Self: Sized;
    /// Returns `self` as a C (void *) pointer
    fn as_void_ptr(&self) -> *const libc::c_void;
}

// This macro implements the trait Numeric for the type "sized_type".
// The use of this macro reduce code duplication for the implementation of Numeric
// for the common numeric types (i32, f32 ...): they only differs by the name of the
// C function used to fetch values from the NetCDF variable (eg: 'nc_get_var_ushort', ...).
//
macro_rules! impl_numeric {
    ( 
        $sized_type: ty,
        $nc_type: ident, 
        $nc_get_var: ident, 
        $nc_get_vara_type: ident,
        $nc_get_var1_type: ident, 
        $nc_put_var1_type: ident,
        $nc_put_vara_type: ident) => {

        impl Numeric for $sized_type {

            // fetch ALL values from variable using `$nc_get_var`
            fn from_variable(variable: &Variable) -> Result<Vec<$sized_type>, String> {
                let mut buf: Vec<$sized_type> = Vec::with_capacity(variable.len as usize);
                let err: i32;
                unsafe {
                    let _g = libnetcdf_lock.lock().unwrap();
                    buf.set_len(variable.len as usize);
                    err = $nc_get_var(variable.grp_id, variable.id, buf.as_mut_ptr());
                }
                if err != NC_NOERR {
                    return Err(NC_ERRORS.get(&err).unwrap().clone());
                }
                Ok(buf)
            }
            
            // Read all values from variable using `$nc_get_var` into a pre-allocated buffer
            fn read_variable_into_buffer(variable: &Variable, buffer: &mut Vec<$sized_type>) -> Result<(), String> {
                // check buffer capacity
                if buffer.capacity() < variable.len as usize {
                    return  Err(
                        format!("Buffer is not big enough. (size {} needed)", variable.len)
                    );
                }
                let err: i32;
                unsafe {
                    let _g = libnetcdf_lock.lock().unwrap();
                    // update the vector element count
                    buffer.set_len(variable.len as usize);
                    // fill the buffer
                    err = $nc_get_var(variable.grp_id, variable.id, buffer.as_mut_ptr());
                }
                if err != NC_NOERR {
                    return Err(NC_ERRORS.get(&err).unwrap().clone());
                }
                Ok(())
            }

            // fetch ONE value from variable using `$nc_get_var1`
            fn single_value_from_variable(variable: &Variable, indices: &[usize]) -> Result<$sized_type, String> {
                // Check the length of `indices`
                if indices.len() != variable.dimensions.len() {
                    return Err("`indices` must has the same length as the variable dimensions".into());
                }
                for i in 0..indices.len() {
                    if (indices[i] as u64) >= variable.dimensions[i].len {
                        return Err("requested index is bigger than the dimension length".into());
                    }
                }
                // initialize `buff` to 0
                let mut buff: $sized_type = 0 as $sized_type;
                let err: i32;
                // Get a pointer to an array [size_t]
                let indices: Vec<size_t> = indices.iter().map(|i| *i as size_t).collect();
                let indices_ptr = indices.as_slice().as_ptr();
                unsafe {
                    let _g = libnetcdf_lock.lock().unwrap();
                    //fn nc_get_var1(ncid: libc::c_int, varid: libc::c_int, indexp: *const size_t, ip: *mut libc::c_void)
                    err = $nc_get_var1_type(variable.grp_id, variable.id, indices_ptr, &mut buff);
                }
                if err != NC_NOERR {
                    return Err(NC_ERRORS.get(&err).unwrap().clone());
                }
                Ok(buff)
            }
            
            // fetch a SLICE of values from variable using `$nc_get_vara`
            fn slice_from_variable(variable: &Variable, indices: &[usize], slice_len: &[usize]) -> Result<Vec<$sized_type>, String> {
                // Check the length of `indices`
                if indices.len() != variable.dimensions.len() {
                    return Err("`indices` must has the same length as the variable dimensions".into());
                }
                if indices.len() != slice_len.len() {
                    return Err("`slice` must has the same length as the variable dimensions".into());
                }
                let mut values: Vec<$sized_type>;
                let mut values_len: usize = 1;
                for i in 0..indices.len() {
                    if (indices[i] as u64) >= variable.dimensions[i].len {
                        return Err("requested index is bigger than the dimension length".into());
                    }
                    if ((indices[i] + slice_len[i]) as u64) > variable.dimensions[i].len {
                        return Err("requested slice is bigger than the dimension length".into());
                    }
                    // Compute the full size of the request values
                    if slice_len[i] > 0 {
                        values_len *= slice_len[i];
                    } else {
                        return Err("Each slice element must be superior than 0".into());
                    }
                }

                let err: i32;
                // Get a pointer to an array [size_t]
                let indices: Vec<size_t> = indices.iter().map(|i| *i as size_t).collect();
                let slice: Vec<size_t> = slice_len.iter().map(|i| *i as size_t).collect();
                unsafe {
                    let _g = libnetcdf_lock.lock().unwrap();

                    values = Vec::with_capacity(values_len);
                    values.set_len(values_len);
                    //let buff_ptr = values.as_mut_ptr() as *mut _ as *mut libc::c_void;
                    //err = nc_get_vara(
                    err = $nc_get_vara_type(
                        variable.grp_id,
                        variable.id,
                        indices.as_slice().as_ptr(),
                        slice.as_slice().as_ptr(),
                        values.as_mut_ptr()
                    );
                }
                if err != NC_NOERR {
                    return Err(NC_ERRORS.get(&err).unwrap().clone());
                }
                Ok(values)
            }

            // read a SLICE of values from variable using `$nc_get_vara` into `buffer`
            fn read_slice_into_buffer(variable: &Variable, indices: &[usize], slice_len: &[usize], buffer: &mut Vec<$sized_type>) -> Result<(), String> {
                // Check the length of `indices`
                if indices.len() != variable.dimensions.len() {
                    return Err("`indices` must has the same length as the variable dimensions".into());
                }
                if indices.len() != slice_len.len() {
                    return Err("`slice` must has the same length as the variable dimensions".into());
                }
                let mut values_len: usize = 1;
                for i in 0..indices.len() {
                    if (indices[i] as u64) >= variable.dimensions[i].len {
                        return Err("requested index is bigger than the dimension length".into());
                    }
                    if ((indices[i] + slice_len[i]) as u64) > variable.dimensions[i].len {
                        return Err("requested slice is bigger than the dimension length".into());
                    }
                    // Compute the full size of the request values
                    if slice_len[i] > 0 {
                        values_len *= slice_len[i];
                    } else {
                        return Err("Each slice element must be superior than 0".into());
                    }
                }
                // check buffer capacity
                if buffer.capacity() < values_len {
                    return  Err(
                        format!("Buffer is not big enough. (size {} needed)", values_len)
                    );
                }

                let err: i32;
                // Get a pointer to an array [size_t]
                let indices: Vec<size_t> = indices.iter().map(|i| *i as size_t).collect();
                let slice: Vec<size_t> = slice_len.iter().map(|i| *i as size_t).collect();
                unsafe {
                    let _g = libnetcdf_lock.lock().unwrap();
                    // update the vector element count
                    buffer.set_len(values_len as usize);
                    // read values into the buffer
                    err = $nc_get_vara_type(
                        variable.grp_id,
                        variable.id,
                        indices.as_slice().as_ptr(),
                        slice.as_slice().as_ptr(),
                        buffer.as_mut_ptr()
                    );
                }
                if err != NC_NOERR {
                    return Err(NC_ERRORS.get(&err).unwrap().clone());
                }
                Ok(())
            }
            // put a SINGLE value into a netCDF variable at the given index
            fn put_value_at(variable: &mut Variable, indices: &[usize], value: Self) -> Result<(), String> {
                // Check the length of `indices`
                if indices.len() != variable.dimensions.len() {
                    return Err("`indices` must has the same length as the variable dimensions".into());
                }
                for i in 0..indices.len() {
                    if (indices[i] as u64) >= variable.dimensions[i].len {
                        return Err("requested index is bigger than the dimension length".into());
                    }
                }
                let err: i32;
                // Get a pointer to an array [size_t]
                let indices: Vec<size_t> = indices.iter().map(|i| *i as size_t).collect();
                let indices_ptr = indices.as_slice().as_ptr();
                unsafe {
                    let _g = libnetcdf_lock.lock().unwrap();
                    err = $nc_put_var1_type(variable.grp_id, variable.id, indices_ptr, &value);
                }
                if err != NC_NOERR {
                    return Err(NC_ERRORS.get(&err).unwrap().clone());
                }

                Ok(())
            }
            
            // put a SLICE of values into a netCDF variable at the given index
            fn put_values_at(variable: &mut Variable, indices: &[usize], slice_len: &[usize], values: &[Self]) -> Result<(), String> {
                if indices.len() != slice_len.len() {
                    return Err("`slice` must has the same length as the variable dimensions".into());
                }
                let mut values_len = 0;
                for i in 0..indices.len() {
                    if (indices[i] as u64) >= variable.dimensions[i].len {
                        return Err("requested index is bigger than the dimension length".into());
                    }
                    if ((indices[i] + slice_len[i]) as u64) > variable.dimensions[i].len {
                        return Err("requested slice is bigger than the dimension length".into());
                    }
                    // Check for empty slice
                    if slice_len[i] == 0 {
                        return Err("Each slice element must be superior than 0".into());
                    }
                    values_len += slice_len[i];
                }
                if values_len  != values.len() {
                    return Err("number of element in `values` doesn't match `slice_len`".into());
                }

                let err: i32;
                // Get a pointer to an array [size_t]
                let indices: Vec<size_t> = indices.iter().map(|i| *i as size_t).collect();
                let slice: Vec<size_t> = slice_len.iter().map(|i| *i as size_t).collect();
                unsafe {
                    let _g = libnetcdf_lock.lock().unwrap();
                    err = $nc_put_vara_type(
                        variable.grp_id,
                        variable.id,
                        indices.as_slice().as_ptr(),
                        slice.as_slice().as_ptr(),
                        values.as_ptr()
                    );
                }
                if err != NC_NOERR {
                    return Err(NC_ERRORS.get(&err).unwrap().clone());
                }

                Ok(())
            }

            fn as_void_ptr(&self) -> *const libc::c_void {
                self as *const _ as *const libc::c_void
            }
        }
    }
}
impl_numeric!(u8,
	 NC_CHAR,
	 nc_get_var_uchar,
	 nc_get_vara_uchar,
	 nc_get_var1_uchar,
	 nc_put_var1_uchar,
	 nc_put_vara_uchar
);

impl_numeric!(i8,
	 NC_BYTE,
	 nc_get_var_schar,
	 nc_get_vara_schar,
	 nc_get_var1_schar,
	 nc_put_var1_schar,
	 nc_put_vara_schar
);

impl_numeric!(i16,
	 NC_SHORT,
	 nc_get_var_short,
	 nc_get_vara_short,
	 nc_get_var1_short,
	 nc_put_var1_short,
	 nc_put_vara_short
);

impl_numeric!(u16,
	 NC_USHORT,
	 nc_get_var_ushort,
	 nc_get_vara_ushort,
	 nc_get_var1_ushort,
	 nc_put_var1_ushort,
	 nc_put_vara_ushort
);

impl_numeric!(i32,
	 NC_INT,
	 nc_get_var_int,
	 nc_get_vara_int,
	 nc_get_var1_int,
	 nc_put_var1_int,
	 nc_put_vara_int
);

impl_numeric!(u32,
	 NC_UINT,
	 nc_get_var_uint,
	 nc_get_vara_uint,
	 nc_get_var1_uint,
	 nc_put_var1_uint,
	 nc_put_vara_uint
);

impl_numeric!(i64,
	 NC_INT64,
	 nc_get_var_longlong,
	 nc_get_vara_longlong,
	 nc_get_var1_longlong,
	 nc_put_var1_longlong,
	 nc_put_vara_longlong
);

impl_numeric!(u64,
	 NC_UINT64,
	 nc_get_var_ulonglong,
	 nc_get_vara_ulonglong,
	 nc_get_var1_ulonglong,
	 nc_put_var1_ulonglong,
	 nc_put_vara_ulonglong
);

impl_numeric!(f32,
	 NC_FLOAT,
	 nc_get_var_float,
	 nc_get_vara_float,
	 nc_get_var1_float,
	 nc_put_var1_float,
	 nc_put_vara_float
);

impl_numeric!(f64,
	 NC_DOUBLE,
	 nc_get_var_double,
	 nc_get_vara_double,
	 nc_get_var1_double,
	 nc_put_var1_double,
	 nc_put_vara_double
);


/// This struct defines a netCDF variable.
pub struct Variable {
    /// The variable name
    pub name : String,
    pub attributes : HashMap<String, Attribute>,
    pub dimensions : Vec<Dimension>,
    /// the netcdf variable type identifier (from netcdf-sys)
    pub vartype : i32,
    pub id: i32,
    /// total length; the product of all dim lengths
    pub len: u64, 
    pub grp_id: i32,
}

impl Variable {
    pub fn get_char(&self, cast: bool) -> Result<Vec<u8>, String> {
        get_var_as_type!(self, NC_CHAR, u8, nc_get_var_uchar, cast)
    }
    pub fn get_char_str(&self, cast:bool) -> Result<Vec<i8>, String> {
       get_var_as_type!(self, NC_CHAR, i8, nc_get_var_text, cast)
   }
    pub fn get_byte(&self, cast: bool) -> Result<Vec<i8>, String> {
        get_var_as_type!(self, NC_BYTE, i8, nc_get_var_schar, cast)
    }
    pub fn get_short(&self, cast: bool) -> Result<Vec<i16>, String> {
        get_var_as_type!(self, NC_SHORT, i16, nc_get_var_short, cast)
    }
    pub fn get_ushort(&self, cast: bool) -> Result<Vec<u16>, String> {
        get_var_as_type!(self, NC_USHORT, u16, nc_get_var_ushort, cast)
    }
    pub fn get_int(&self, cast: bool) -> Result<Vec<i32>, String> {
        get_var_as_type!(self, NC_INT, i32, nc_get_var_int, cast)
    }
    pub fn get_uint(&self, cast: bool) -> Result<Vec<u32>, String> {
        get_var_as_type!(self, NC_UINT, u32, nc_get_var_uint, cast)
    }
    pub fn get_int64(&self, cast: bool) -> Result<Vec<i64>, String> {
        get_var_as_type!(self, NC_INT64, i64, nc_get_var_longlong, cast)
    }
    pub fn get_uint64(&self, cast: bool) -> Result<Vec<u64>, String> {
        get_var_as_type!(self, NC_UINT64, u64, nc_get_var_ulonglong, cast)
    }
    pub fn get_float(&self, cast: bool) -> Result<Vec<f32>, String> {
        get_var_as_type!(self, NC_FLOAT, f32, nc_get_var_float, cast)
    }
    pub fn get_double(&self, cast: bool) -> Result<Vec<f64>, String> {
        get_var_as_type!(self, NC_DOUBLE, f64, nc_get_var_double, cast)
    }

    pub fn add_attribute<T: PutAttr>(&mut self, name: &str, val: T) 
            -> Result<(), String> {
        try!(val.put(self.grp_id, self.id, name));
        self.attributes.insert(
                name.to_string().clone(),
                Attribute {
                    name: name.to_string().clone(),
                    attrtype: val.get_nc_type(),
                    id: 0, // XXX Should Attribute even keep track of an id?
                    var_id: self.id,
                    file_id: self.grp_id
                }
            );
        Ok(())
    }

    /// Fetchs variable values, and cast them if needed.
    ///
    /// ```
    /// // let values: Vec<f64> = some_variable.values().unwrap();
    /// ```
    ///
    pub fn values<T: Numeric>(&self) -> Result<Vec<T>, String> {
        T::from_variable(self)
    }
    
    /// Read a slice of a variable into a buffer,
    /// the buffer must have a capacity at least equal as the number of elements of the slice.
    /// The buffer length (not its capacity) will be updated.
    pub fn read_values_into_buffer<T: Numeric>(&self, buffer: &mut Vec<T>) -> Result<(), String> {
        T::read_variable_into_buffer(self, buffer)
    }

    ///  Fetchs one specific value at specific indices
    ///  indices must has the same length as self.dimensions.
    pub fn value_at<T: Numeric>(&self, indices: &[usize]) -> Result<T, String> {
        T::single_value_from_variable(self, indices)
    }

    /// Read a slice of a variable into a buffer:
    ///
    /// * the 'buffer' must have a capacity at least equal as the product of all elements in 'slice'.
    /// * 'indices' must has the same length as self.dimensions.
    /// * all 'slice' elements must be > 0.
    ///
    /// The buffer length (not its capacity) will be updated.
    pub fn read_slice_into_buffer<T: Numeric>(&self, indices: &[usize], slice_len: &[usize], buffer: &mut Vec<T>) -> Result<(), String> {
        T::read_slice_into_buffer(self, indices, slice_len, buffer)
    }

    /// Fetchs a slice of values
    /// indices must has the same length as self.dimensions.
    /// All slice elements must be > 0.
    pub fn values_at<T: Numeric>(&self, indices: &[usize], slice_len: &[usize]) -> Result<Vec<T>, String> {
        T::slice_from_variable(self, indices, slice_len)
    }

    /// Fetchs variable values as a ndarray.
    ///
    /// ```
    /// // Each values will be implicitly casted to a f64 if needed
    /// // let values: ArrayD<f64> = some_variable.as_array().unwrap();
    /// ```
    ///
    pub fn as_array<T: Numeric>(&self) -> Result<ArrayD<T>, Box<Error>> {
        let mut dims: Vec<usize> = Vec::new();
        for dim in &self.dimensions {
            dims.push(dim.len as usize);
        }
        let values = self.values()?;
        Ok(ArrayD::<T>::from_shape_vec(dims, values)?)
    }
    
    /// Fetchs variable slice as a ndarray.
    pub fn array_at<T: Numeric>(&self, indices: &[usize], slice_len: &[usize]) -> Result<ArrayD<T>, Box<Error>> {
        let values = self.values_at(indices, slice_len)?;
        Ok(ArrayD::<T>::from_shape_vec(slice_len, values)?)
    }

    /// Put a single value at `indices`
    pub fn put_value_at<T: Numeric>(&mut self, value: T, indices: &[usize]) -> Result<(), String> {
        T::put_value_at(self, indices, value)
    }

    /// Put a slice of values at `indices`
    pub fn put_values_at<T: Numeric>(&mut self, values: &[T], indices: &[usize], slice_len: &[usize]) -> Result<(), String> {
        T::put_values_at(self, indices, slice_len, values)
    }

    /// Set a Fill Value
    pub fn set_fill_value<T: Numeric>(&mut self, fill_value: T) -> Result<(), String> {
        let err: i32;
        unsafe {
            let _g = libnetcdf_lock.lock().unwrap();
            err = nc_def_var_fill(self.grp_id, self.id, 0 as libc::c_int, fill_value.as_void_ptr());
        }
        if err != NC_NOERR {
            return Err(NC_ERRORS.get(&err).unwrap().clone());
        }
        self.update_attributes()?;
        Ok(())
    }

    /// update self.attributes, (sync cached attribute and the file)
    fn update_attributes(&mut self) -> Result<(), String> {
        let mut natts: i32 = 0;
        let err: i32;
        unsafe {
            let _g = libnetcdf_lock.lock().unwrap();
            err = nc_inq_varnatts(self.grp_id, self.id, &mut natts);
        }
        if err != NC_NOERR {
            return Err(NC_ERRORS.get(&err).unwrap().clone());
        }
        let (grp_id, var_id) = (self.grp_id, self.id);
        self.attributes.clear();
        init_attributes(&mut self.attributes, grp_id, var_id, natts);
        Ok(())
    }
}

pub fn init_variables(vars: &mut HashMap<String, Variable>, grp_id: i32, grp_dims: &HashMap<String, Dimension>) {
    // determine number of vars
    let mut nvars = 0i32;
    unsafe {
        let _g = libnetcdf_lock.lock().unwrap();
        let err = nc_inq_nvars(grp_id, &mut nvars);
        assert_eq!(err, NC_NOERR);
    }
    for i_var in 0..nvars {
        init_variable(vars, grp_id, grp_dims, i_var);
    }
}

/// Creates and add a `Variable` Objects, from the dataset
pub fn init_variable(vars: &mut HashMap<String, Variable>, grp_id: i32, grp_dims: &HashMap<String, Dimension>, varid: i32) {
    // read each dim name and length
    let mut buf_vec = vec![0i8; (NC_MAX_NAME + 1) as usize];
    let c_str: &ffi::CStr;
    let mut var_type : i32 = 0;
    let mut ndims : i32 = 0;
    let mut dimids : Vec<i32> = Vec::with_capacity(NC_MAX_DIMS as usize);
    let mut natts : i32 = 0;
    unsafe {
        let _g = libnetcdf_lock.lock().unwrap();
        let buf_ptr : *mut i8 = buf_vec.as_mut_ptr();
        let err = nc_inq_var(grp_id, varid, buf_ptr,
                                &mut var_type, &mut ndims,
                                dimids.as_mut_ptr(), &mut natts);
        dimids.set_len(ndims as usize);
        assert_eq!(err, NC_NOERR);
        c_str = ffi::CStr::from_ptr(buf_ptr);
    }
    let str_buf: String = string_from_c_str(c_str);
    let mut attr_map : HashMap<String, Attribute> = HashMap::new();
    init_attributes(&mut attr_map, grp_id, varid, natts);
    // var dims should always be a subset of the group dims:
    let mut dim_vec : Vec<Dimension> = Vec::new();
    let mut len : u64 = 1;
    for dimid in dimids {
        // maintaining dim order is crucial here so we can maintain
        // rule that "last dim varies fastest" in our 1D return Vec
        for (_, grp_dim) in grp_dims {
            if dimid == grp_dim.id {
                len *= grp_dim.len;
                dim_vec.push(grp_dim.clone());
                break
            }
        }
    }
    vars.insert(
        str_buf.clone(),
        Variable{
            name: str_buf.clone(),
            attributes: attr_map,
            dimensions: dim_vec,
            vartype: var_type,
            len: len,
            id: varid,
            grp_id: grp_id
        }
   );
}

