use crate::backend::CacheBackend;

#[cfg(feature = "flashdb")]
mod ffi {
    #![allow(non_camel_case_types, non_snake_case, unused_imports)]
    include!(concat!(env!("OUT_DIR"), "/flashdb_bindings.rs"));
}

#[cfg(feature = "flashdb")]
use std::ffi::CString;
#[cfg(feature = "flashdb")]
use std::os::raw::c_char;
#[cfg(feature = "flashdb")]
use std::path::Path;
#[cfg(feature = "flashdb")]
use std::ptr;

#[cfg(feature = "flashdb")]
pub struct FlashDb {
    kvdb: *mut ffi::fdb_kvdb,
    data_dir: String,
}

#[cfg(not(feature = "flashdb"))]
pub struct FlashDb {
    _ph: (),
}

#[cfg(feature = "flashdb")]
unsafe impl Send for FlashDb {}
#[cfg(feature = "flashdb")]
unsafe impl Sync for FlashDb {}

impl FlashDb {
    #[cfg(feature = "flashdb")]
    pub fn new(data_dir: &str) -> Result<Self, String> {
        let dir = Path::new(data_dir);
        if !dir.exists() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create FlashDB data dir: {}", e))?;
        }

        let kvdb = Box::into_raw(Box::new(unsafe { std::mem::zeroed::<ffi::fdb_kvdb>() }));

        unsafe {
            (*kvdb).parent.sec_size = 4096;
            (*kvdb).parent.max_size = 128 * 1024 * 1024;
        }

        let name = CString::new("kvdb").map_err(|e| format!("invalid name: {}", e))?;
        let part = CString::new(data_dir).map_err(|e| format!("invalid partition: {}", e))?;

        let result = unsafe {
            ffi::fdb_kvdb_init(
                kvdb,
                name.as_ptr(),
                part.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };

        if result != ffi::fdb_err_t_FDB_NO_ERR {
            unsafe { drop(Box::from_raw(kvdb)) };
            return Err(format!("FlashDB init failed: {}", result));
        }

        tracing::info!(data_dir = %data_dir, "FlashDB initialized");
        Ok(Self {
            kvdb,
            data_dir: data_dir.to_string(),
        })
    }

    #[cfg(not(feature = "flashdb"))]
    pub fn new(_data_dir: &str) -> Result<Self, String> {
        tracing::warn!("FlashDB feature not enabled; using stub");
        Ok(Self { _ph: () })
    }
}

#[cfg(feature = "flashdb")]
impl Drop for FlashDb {
    fn drop(&mut self) {
        unsafe {
            ffi::fdb_kvdb_deinit(self.kvdb);
            drop(Box::from_raw(self.kvdb));
        }
        tracing::debug!(data_dir = %self.data_dir, "FlashDB deinitialized");
    }
}

#[cfg(feature = "flashdb")]
impl CacheBackend for FlashDb {
    fn get(&self, store: &str, entity: &str, key: &str) -> Option<Vec<u8>> {
        let kv_key = internal_key(store, entity, key);
        let c_key = CString::new(kv_key).ok()?;
        let mut buf = vec![0u8; 8192];

        let mut blob = ffi::fdb_blob {
            buf: buf.as_mut_ptr() as *mut std::ffi::c_void,
            size: buf.len(),
            saved: unsafe { std::mem::zeroed() },
        };

        let blob_ptr = unsafe { ffi::fdb_blob_make(&mut blob, ptr::null_mut(), 0) };
        let result = unsafe { ffi::fdb_kv_get_blob(self.kvdb, c_key.as_ptr(), blob_ptr) };

        if result as u32 != ffi::fdb_err_t_FDB_NO_ERR || blob.saved.len == 0 {
            return None;
        }

        buf.truncate(blob.saved.len);
        Some(buf)
    }

    fn set(&self, store: &str, entity: &str, key: &str, value: &[u8]) {
        let kv_key = internal_key(store, entity, key);
        let c_key = match CString::new(kv_key) {
            Ok(k) => k,
            Err(_) => return,
        };

        let mut blob = unsafe { std::mem::zeroed::<ffi::fdb_blob>() };
        let blob_ptr = unsafe {
            ffi::fdb_blob_make(
                &mut blob,
                value.as_ptr() as *mut std::ffi::c_void,
                value.len(),
            )
        };

        let result = unsafe {
            ffi::fdb_kv_set_blob(self.kvdb, c_key.as_ptr(), blob_ptr)
        };
        if result != ffi::fdb_err_t_FDB_NO_ERR {
            tracing::error!(key = %std::str::from_utf8(unsafe { std::ffi::CStr::from_ptr(c_key.as_ptr()) }.to_bytes()).unwrap_or("?"), len = value.len(), err = result, "FlashDB set_blob failed");
        }
    }

    fn del(&self, store: &str, entity: &str, key: &str) {
        let kv_key = internal_key(store, entity, key);
        let c_key = match CString::new(kv_key) {
            Ok(k) => k,
            Err(_) => return,
        };

        unsafe {
            ffi::fdb_kv_del(self.kvdb, c_key.as_ptr());
        }
    }

    fn delete_by_prefix(&self, store: &str, prefix: &str) {
        let full_prefix = format!("{}:{}:", store, prefix);

        let mut itr = unsafe { std::mem::zeroed::<ffi::fdb_kv_iterator>() };

        unsafe {
            ffi::fdb_kv_iterator_init(self.kvdb, &mut itr);
        }

        loop {
            let has_next = unsafe { ffi::fdb_kv_iterate(self.kvdb, &mut itr) };
            if !has_next {
                break;
            }

            let kv = &itr.curr_kv;
            let name_bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(kv.name.as_ptr() as *const u8, kv.name_len as usize)
            };

            if name_bytes.starts_with(full_prefix.as_bytes()) {
                let key_ptr = unsafe {
                    std::ffi::CStr::from_bytes_with_nul_unchecked(std::slice::from_raw_parts(
                        kv.name.as_ptr() as *const u8,
                        64,
                    ))
                };
                unsafe {
                    ffi::fdb_kv_del(self.kvdb, key_ptr.as_ptr());
                }
            }
        }
    }

    fn flush(&self, store: &str) {
        self.delete_by_prefix(store, "");
    }
}

#[cfg(not(feature = "flashdb"))]
impl CacheBackend for FlashDb {
    fn get(&self, _store: &str, _entity: &str, _key: &str) -> Option<Vec<u8>> {
        None
    }

    fn set(&self, _store: &str, _entity: &str, _key: &str, _value: &[u8]) {}

    fn del(&self, _store: &str, _entity: &str, _key: &str) {}

    fn delete_by_prefix(&self, _store: &str, _prefix: &str) {}

    fn flush(&self, _store: &str) {}
}

#[cfg(feature = "flashdb")]
fn internal_key(store: &str, entity: &str, key: &str) -> String {
    format!("{}:{}:{}", store, entity, key)
}

#[cfg(not(feature = "flashdb"))]
#[allow(dead_code)]
fn internal_key(_store: &str, _entity: &str, _key: &str) -> String {
    String::new()
}

#[allow(clippy::derivable_impls)]
impl Default for FlashDb {
    fn default() -> Self {
        #[cfg(feature = "flashdb")]
        {
            Self::new("/tmp/ixmati-flashdb").unwrap_or_else(|e| {
                tracing::warn!(error = %e, "FlashDB init failed, falling back");
                Self {
                    kvdb: ptr::null_mut(),
                    data_dir: String::new(),
                }
            })
        }
        #[cfg(not(feature = "flashdb"))]
        {
            Self { _ph: () }
        }
    }
}
