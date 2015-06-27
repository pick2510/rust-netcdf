use std::ffi;
use std::collections::HashMap;
use netcdf_sys::*;
use dimension::Dimension;
use group::PutAttr;
use attribute::{init_attributes, Attribute};
use string_from_c_str;
use NC_ERRORS;

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
            err = $nc_fn($me.file_id, $me.id, buf.as_mut_ptr());
        }
        if err != nc_noerr {
            return Err(NC_ERRORS.get(&err).unwrap().clone());
        }
        Ok(buf)
    }};
}

pub struct Variable {
    pub name : String,
    pub attributes : HashMap<String, Attribute>,
    pub dimensions : Vec<Dimension>,
    pub vartype : i32,
    pub id: i32,
    pub len: u64, // total length; the product of all dim lengths
    pub file_id: i32,
}

impl Variable {
    pub fn get_char(&self, cast: bool) -> Result<Vec<u8>, String> {
        get_var_as_type!(self, nc_char, u8, nc_get_var_uchar, cast)
    }
    pub fn get_byte(&self, cast: bool) -> Result<Vec<i8>, String> {
        get_var_as_type!(self, nc_byte, i8, nc_get_var_schar, cast)
    }
    pub fn get_short(&self, cast: bool) -> Result<Vec<i16>, String> {
        get_var_as_type!(self, nc_short, i16, nc_get_var_short, cast)
    }
    pub fn get_ushort(&self, cast: bool) -> Result<Vec<u16>, String> {
        get_var_as_type!(self, nc_ushort, u16, nc_get_var_ushort, cast)
    }
    pub fn get_int(&self, cast: bool) -> Result<Vec<i32>, String> {
        get_var_as_type!(self, nc_int, i32, nc_get_var_int, cast)
    }
    pub fn get_uint(&self, cast: bool) -> Result<Vec<u32>, String> {
        get_var_as_type!(self, nc_uint, u32, nc_get_var_uint, cast)
    }
    pub fn get_int64(&self, cast: bool) -> Result<Vec<i64>, String> {
        get_var_as_type!(self, nc_int64, i64, nc_get_var_longlong, cast)
    }
    pub fn get_uint64(&self, cast: bool) -> Result<Vec<u64>, String> {
        get_var_as_type!(self, nc_uint64, u64, nc_get_var_ulonglong, cast)
    }
    pub fn get_float(&self, cast: bool) -> Result<Vec<f32>, String> {
        get_var_as_type!(self, nc_float, f32, nc_get_var_float, cast)
    }
    pub fn get_double(&self, cast: bool) -> Result<Vec<f64>, String> {
        get_var_as_type!(self, nc_double, f64, nc_get_var_double, cast)
    }

    pub fn add_attribute<T: PutAttr>(&mut self, name: &str, val: T) 
            -> Result<(), String> {
        try!(val.put(self.file_id, self.id, name));
        self.attributes.insert(
                name.to_string().clone(),
                Attribute {
                    name: name.to_string().clone(),
                    attrtype: val.get_nc_type(),
                    id: 0, // XXX Should Attribute even keep track of an id?
                    var_id: self.id,
                    file_id: self.file_id
                }
            );
        Ok(())
    }
}

pub fn init_variables(vars: &mut HashMap<String, Variable>, grp_id: i32,
                  grp_dims: &HashMap<String, Dimension>) {
    // determine number of vars
    let mut nvars = 0i32;
    unsafe {
        let _g = libnetcdf_lock.lock().unwrap();
        let err = nc_inq_nvars(grp_id, &mut nvars);
        assert_eq!(err, nc_noerr);
    }
    // read each dim name and length
    for i_var in 0..nvars {
        let mut buf_vec = vec![0i8; (nc_max_name + 1) as usize];
        let c_str: &ffi::CStr;
        let mut var_type : i32 = 0;
        let mut ndims : i32 = 0;
        let mut dimids : Vec<i32> = Vec::with_capacity(nc_max_dims as usize);
        let mut natts : i32 = 0;
        unsafe {
            let _g = libnetcdf_lock.lock().unwrap();
            let buf_ptr : *mut i8 = buf_vec.as_mut_ptr();
            let err = nc_inq_var(grp_id, i_var, buf_ptr,
                                    &mut var_type, &mut ndims,
                                    dimids.as_mut_ptr(), &mut natts);
            dimids.set_len(ndims as usize);
            assert_eq!(err, nc_noerr);
            c_str = ffi::CStr::from_ptr(buf_ptr);
        }
        let str_buf: String = string_from_c_str(c_str);
        let mut attr_map : HashMap<String, Attribute> = HashMap::new();
        init_attributes(&mut attr_map, grp_id, i_var, natts);
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
        vars.insert(str_buf.clone(),
                      Variable{name: str_buf.clone(),
                          attributes: attr_map,
                          dimensions: dim_vec,
                          vartype: var_type,
                          len: len,
                          id: i_var,
                          file_id: grp_id});
    }
}
