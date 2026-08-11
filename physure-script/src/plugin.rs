//! Native `.rs` extensions: compiled `cdylib` plugins dropped in `<script_dir>/ext/`,
//! discovered and dlopen'd the same way the Python bindings load `ext/*.py`.
//!
//! Rust has no interpreter, so a `.rs` file can't be loaded like a `.py` one —
//! plugin authors compile their extension crate to a `cdylib` and place the
//! resulting `.so`/`.dylib`/`.dll` under `ext/`. The plugin exports a single
//! `extern "C" fn phs_plugin_entry() -> PluginRegistry` symbol describing its
//! functions; each function takes and returns a [`PluginValue`], a tagged union
//! covering every [`PhsValue`] except `Function`/`Sigma`/`SigmaBound`/`Plot`
//! (those don't have a stable, useful C representation and aren't supported
//! across this boundary).
//!
//! Plugins can also be hot-reloaded: [`PhsInterpreter::reload_native_ext`]
//! re-scans `ext/` and swaps in any plugin whose file changed, without
//! restarting the process.

use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::SystemTime;

use physure_core::error::PhysureResult;
#[cfg(not(target_arch = "wasm32"))]
use physure_core::error::PhysureError;
#[cfg(not(target_arch = "wasm32"))]
use physure_core::quantity::Quantity;

use crate::interpreter::ExternalFn;
#[cfg(not(target_arch = "wasm32"))]
use crate::value::PhsValue;

/// Bumped whenever `PluginValue`/`PluginFnEntry`/`PluginRegistry`'s layout changes.
pub const PHS_PLUGIN_ABI_VERSION: u32 = 2;

/// Discriminant for [`PluginValue`]. `#[repr(u8)]` so it's FFI-safe.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginValueTag {
    None = 0,
    Number = 1,
    Bool = 2,
    Quantity = 3,
    String = 4,
    Vector = 5,
}

/// A tagged union carrying one [`PhsValue`] across the plugin ABI boundary.
///
/// - `Number`/`Bool`: value in `number` (`Bool` as 0.0/1.0).
/// - `Quantity`: magnitude in `number`, unit expression string (e.g. `"m/s"`)
///   in `text`.
/// - `String`: nul-terminated string in `text`.
/// - `Vector`: `item_count` elements at `items`.
///
/// Unused fields are zero/null. All pointers are borrows valid only for the
/// duration of the call they appear in — a plugin must not retain them.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PluginValue {
    pub tag: PluginValueTag,
    pub number: f64,
    pub text: *const c_char,
    pub items: *const PluginValue,
    pub item_count: usize,
}

impl PluginValue {
    pub const NONE: PluginValue = PluginValue {
        tag: PluginValueTag::None,
        number: 0.0,
        text: std::ptr::null(),
        items: std::ptr::null(),
        item_count: 0,
    };
}

/// A native plugin function: takes a borrowed argument array, returns one value.
pub type PluginFn = extern "C" fn(*const PluginValue, usize) -> PluginValue;

#[repr(C)]
pub struct PluginFnEntry {
    pub name: *const c_char,
    pub func: PluginFn,
}

#[repr(C)]
pub struct PluginRegistry {
    pub abi_version: u32,
    pub entries: *const PluginFnEntry,
    pub entry_count: usize,
}

/// Signature every plugin cdylib must export under the symbol `phs_plugin_entry`.
pub type PluginEntryPoint = unsafe extern "C" fn() -> PluginRegistry;

/// Owns the conversions' `CString`/`Vec` allocations, keeping them alive for
/// the duration of a single plugin call (the `PluginValue`s built from them
/// borrow, they don't own).
///
/// Only used by the real (non-wasm32) plugin-loading path below.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct ValueArena {
    strings: Vec<CString>,
    vectors: Vec<Vec<PluginValue>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ValueArena {
    fn push(&mut self, value: &PhsValue) -> PhysureResult<PluginValue> {
        Ok(match value {
            PhsValue::None => PluginValue::NONE,
            PhsValue::Number(n) => PluginValue {
                tag: PluginValueTag::Number,
                number: *n,
                ..PluginValue::NONE
            },
            PhsValue::Bool(b) => PluginValue {
                tag: PluginValueTag::Bool,
                number: if *b { 1.0 } else { 0.0 },
                ..PluginValue::NONE
            },
            PhsValue::Quantity(q) => PluginValue {
                tag: PluginValueTag::Quantity,
                number: q.value.mean(),
                text: self.intern(q.unit.__repr__())?,
                ..PluginValue::NONE
            },
            PhsValue::String(s) => PluginValue {
                tag: PluginValueTag::String,
                text: self.intern(s.clone())?,
                ..PluginValue::NONE
            },
            PhsValue::Vector(items) => {
                let converted = items
                    .iter()
                    .map(|v| self.push(v))
                    .collect::<PhysureResult<Vec<_>>>()?;
                let (ptr, len) = (converted.as_ptr(), converted.len());
                self.vectors.push(converted);
                PluginValue {
                    tag: PluginValueTag::Vector,
                    items: ptr,
                    item_count: len,
                    ..PluginValue::NONE
                }
            }
            PhsValue::Function(_)
            | PhsValue::Sigma(_)
            | PhsValue::SigmaBound(_, _)
            | PhsValue::Plot(_)
            | PhsValue::Equation(_, _)
            | PhsValue::Matrix(_)
            | PhsValue::Range(_, _) => {
                return Err(PhysureError::Generic(
                    "native plugin functions don't support function, sigma, plot, equation, or range values".into(),
                ));
            }
        })
    }

    fn intern(&mut self, s: String) -> PhysureResult<*const c_char> {
        let c = CString::new(s).map_err(|e| PhysureError::Generic(e.to_string()))?;
        let ptr = c.as_ptr();
        self.strings.push(c);
        Ok(ptr)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn from_plugin_value(value: &PluginValue) -> PhysureResult<PhsValue> {
    match value.tag {
        PluginValueTag::None => Ok(PhsValue::None),
        PluginValueTag::Number => Ok(PhsValue::Number(value.number)),
        PluginValueTag::Bool => Ok(PhsValue::Bool(value.number != 0.0)),
        PluginValueTag::Quantity => {
            // SAFETY: plugin contract requires `text` to be a valid nul-terminated
            // C string for the `Quantity` tag.
            let unit = unsafe { CStr::from_ptr(value.text) }.to_string_lossy();
            Ok(PhsValue::Quantity(Quantity::new(value.number, &unit)?))
        }
        PluginValueTag::String => {
            let s = unsafe { CStr::from_ptr(value.text) }
                .to_string_lossy()
                .into_owned();
            Ok(PhsValue::String(s))
        }
        PluginValueTag::Vector => {
            // SAFETY: plugin contract requires `items`/`item_count` to describe
            // a valid slice for the `Vector` tag.
            let items = unsafe { std::slice::from_raw_parts(value.items, value.item_count) };
            let converted = items
                .iter()
                .map(from_plugin_value)
                .collect::<PhysureResult<Vec<_>>>()?;
            Ok(PhsValue::Vector(converted))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct LoadedPlugin {
    mtime: SystemTime,
    functions: HashMap<String, ExternalFn>,
    // Kept alive because closures above hold raw function pointers into it.
    _lib: libloading::Library,
}

/// Tracks which plugin files have been loaded (and when), keyed by their full
/// path, so [`PluginState::reload_loaded_into`] can tell which ones changed on
/// disk. Only stems some `use` statement has actually requested end up here —
/// there is no eager directory scan.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
pub struct PluginState {
    loaded: HashMap<PathBuf, LoadedPlugin>,
}

/// `wasm32-unknown-unknown` has no dlopen equivalent, so native plugin
/// loading is simply unavailable there: every lookup reports "no plugin
/// found" and reload is a no-op. Kept API-identical to the real
/// [`PluginState`] above so callers (e.g. `interpreter.rs`) need no
/// target-specific code of their own.
#[cfg(target_arch = "wasm32")]
#[derive(Default)]
pub struct PluginState;

#[cfg(target_arch = "wasm32")]
impl PluginState {
    pub fn ensure_stem_loaded(
        &mut self,
        _base_dir: &Path,
        _stem: &str,
    ) -> PhysureResult<Option<HashMap<String, ExternalFn>>> {
        Ok(None)
    }

    pub fn reload_loaded_into(&mut self, _externals: &mut HashMap<String, ExternalFn>) -> Vec<String> {
        Vec::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl PluginState {
    /// Loads `<base_dir>/ext/<stem>.<DLL_EXTENSION>` on demand, returning its
    /// exported `{name: function}` map. Returns `Ok(None)` if no such file
    /// exists. Reuses the cached load if the file hasn't changed since the
    /// last call for this stem.
    pub fn ensure_stem_loaded(
        &mut self,
        base_dir: &Path,
        stem: &str,
    ) -> PhysureResult<Option<HashMap<String, ExternalFn>>> {
        let path = base_dir
            .join("ext")
            .join(format!("{stem}.{}", std::env::consts::DLL_EXTENSION));
        if !path.is_file() {
            return Ok(None);
        }
        let mtime = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .map_err(|e| PhysureError::Generic(e.to_string()))?;
        if let Some(loaded) = self.loaded.get(&path) {
            if loaded.mtime == mtime {
                return Ok(Some(loaded.functions.clone()));
            }
        }
        let (lib, functions) = load_plugin_file(&path, mtime)?;
        let functions: HashMap<String, ExternalFn> = functions.into_iter().collect();
        self.loaded.insert(
            path,
            LoadedPlugin { mtime, functions: functions.clone(), _lib: lib },
        );
        Ok(Some(functions))
    }

    /// Re-checks only the plugin files some `use` statement has already
    /// caused to be loaded (via [`Self::ensure_stem_loaded`]) and reloads any
    /// whose file changed since. For each reloaded function name that's
    /// already a key in `externals` (i.e. some `use` actually unlocked it),
    /// installs the fresh closure there. Functions a plugin exports but that
    /// were never `use`d are not injected. Returns the names refreshed.
    ///
    /// ponytail: reload is caller-triggered, not automatic — nothing watches
    /// the filesystem. Callers that want auto-reload can poll this on an
    /// interval or wire it to a file-watcher themselves.
    pub fn reload_loaded_into(&mut self, externals: &mut HashMap<String, ExternalFn>) -> Vec<String> {
        let mut reloaded = Vec::new();
        let paths: Vec<PathBuf> = self.loaded.keys().cloned().collect();
        for path in paths {
            let Ok(mtime) = std::fs::metadata(&path).and_then(|m| m.modified()) else {
                continue;
            };
            if self.loaded.get(&path).is_some_and(|p| p.mtime == mtime) {
                continue; // unchanged since last load
            }
            match load_plugin_file(&path, mtime) {
                Ok((lib, functions)) => {
                    let functions: HashMap<String, ExternalFn> = functions.into_iter().collect();
                    for (name, func) in &functions {
                        if externals.contains_key(name) {
                            externals.insert(name.clone(), func.clone());
                            reloaded.push(name.clone());
                        }
                    }
                    // Old library for this path (if any) is dropped here, now
                    // that `externals` no longer points into it.
                    self.loaded.insert(path, LoadedPlugin { mtime, functions, _lib: lib });
                }
                Err(e) => eprintln!(
                    "warning: failed to reload native extension {}: {}",
                    path.display(),
                    e
                ),
            }
        }
        reloaded
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn load_plugin_file(
    path: &Path,
    mtime: SystemTime,
) -> PhysureResult<(libloading::Library, Vec<(String, ExternalFn)>)> {
    // dlopen caches by path/inode: reopening the same path after a rebuild
    // would silently hand back the *old* mapping instead of the new bytes. So
    // each (re)load dlopen's a uniquely-named temp copy instead of `path`
    // directly. ponytail: copies are left in the temp dir rather than cleaned
    // up (deleting a mapped library mid-run isn't portable); fine unless a
    // process reloads plugins so often disk usage becomes a concern.
    static TEMP_COPY_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let count = TEMP_COPY_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = mtime
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("plugin");
    let temp_copy = std::env::temp_dir().join(format!(
        "phs_plugin_{}_{}_{}_{}.{}",
        stem,
        nanos,
        std::process::id(),
        count,
        std::env::consts::DLL_EXTENSION
    ));
    let mut copy_result = std::fs::copy(path, &temp_copy);
    for _ in 0..20 {
        if copy_result.is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
        copy_result = std::fs::copy(path, &temp_copy);
    }
    copy_result.map_err(|e| PhysureError::Generic(e.to_string()))?;

    // SAFETY: dlopen'ing arbitrary code is inherently unsafe; we trust plugins
    // placed by the script author under `ext/`, the same trust boundary as the
    // `ext/*.py` loader on the Python side.
    let mut lib_result = unsafe { libloading::Library::new(&temp_copy) };
    for _ in 0..20 {
        if lib_result.is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
        lib_result = unsafe { libloading::Library::new(&temp_copy) };
    }
    let lib = lib_result.map_err(|e| PhysureError::Generic(e.to_string()))?;
    let entry_point: libloading::Symbol<PluginEntryPoint> =
        unsafe { lib.get(b"phs_plugin_entry\0") }
            .map_err(|e| PhysureError::Generic(format!("missing phs_plugin_entry: {}", e)))?;
    let registry = unsafe { entry_point() };

    if registry.abi_version != PHS_PLUGIN_ABI_VERSION {
        return Err(PhysureError::Generic(format!(
            "ABI version mismatch: plugin is v{}, interpreter expects v{}",
            registry.abi_version, PHS_PLUGIN_ABI_VERSION
        )));
    }

    let entries = unsafe { std::slice::from_raw_parts(registry.entries, registry.entry_count) };
    let mut functions = Vec::with_capacity(entries.len());
    for entry in entries {
        let name = unsafe { CStr::from_ptr(entry.name) }
            .to_string_lossy()
            .into_owned();
        let func = entry.func;
        let external: ExternalFn = Arc::new(move |args: &[PhsValue]| {
            let mut arena = ValueArena::default();
            let plugin_args = args
                .iter()
                .map(|v| arena.push(v))
                .collect::<PhysureResult<Vec<_>>>()?;
            let result = func(plugin_args.as_ptr(), plugin_args.len());
            from_plugin_value(&result)
        });
        functions.push((name, external));
    }

    Ok((lib, functions))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::Duration;

    const PLUGIN_SRC_V1: &str = r#"
        #[repr(u8)]
        #[derive(Clone, Copy)]
        pub enum PluginValueTag { None = 0, Number = 1, Bool = 2, Quantity = 3, String = 4, Vector = 5 }

        #[repr(C)]
        #[derive(Clone, Copy)]
        pub struct PluginValue {
            pub tag: PluginValueTag,
            pub number: f64,
            pub text: *const std::os::raw::c_char,
            pub items: *const PluginValue,
            pub item_count: usize,
        }

        const NONE: PluginValue = PluginValue { tag: PluginValueTag::None, number: 0.0, text: std::ptr::null(), items: std::ptr::null(), item_count: 0 };

        #[repr(C)]
        pub struct PluginFnEntry {
            pub name: *const std::os::raw::c_char,
            pub func: extern "C" fn(*const PluginValue, usize) -> PluginValue,
        }

        #[repr(C)]
        pub struct PluginRegistry {
            pub abi_version: u32,
            pub entries: *const PluginFnEntry,
            pub entry_count: usize,
        }

        extern "C" fn triple(args: *const PluginValue, len: usize) -> PluginValue {
            let args = unsafe { std::slice::from_raw_parts(args, len) };
            PluginValue { tag: PluginValueTag::Number, number: args[0].number * 3.0, ..NONE }
        }

        extern "C" fn shout(args: *const PluginValue, len: usize) -> PluginValue {
            let args = unsafe { std::slice::from_raw_parts(args, len) };
            let s = unsafe { std::ffi::CStr::from_ptr(args[0].text) }.to_str().unwrap();
            let upper = std::ffi::CString::new(s.to_uppercase()).unwrap();
            PluginValue { tag: PluginValueTag::String, text: upper.into_raw(), ..NONE }
        }

        #[no_mangle]
        pub extern "C" fn phs_plugin_entry() -> PluginRegistry {
            let entries = vec![
                PluginFnEntry { name: std::ffi::CString::new("triple").unwrap().into_raw(), func: triple },
                PluginFnEntry { name: std::ffi::CString::new("shout").unwrap().into_raw(), func: shout },
            ];
            let entries: &'static [PluginFnEntry] = Box::leak(entries.into_boxed_slice());
            PluginRegistry { abi_version: 2, entries: entries.as_ptr(), entry_count: entries.len() }
        }
    "#;

    /// Same as `PLUGIN_SRC_V1` but `triple` multiplies by 4 instead of 3 — used
    /// to exercise hot-reload picking up a changed plugin body.
    const PLUGIN_SRC_V2: &str = r#"
        #[repr(u8)]
        #[derive(Clone, Copy)]
        pub enum PluginValueTag { None = 0, Number = 1, Bool = 2, Quantity = 3, String = 4, Vector = 5 }

        #[repr(C)]
        #[derive(Clone, Copy)]
        pub struct PluginValue {
            pub tag: PluginValueTag,
            pub number: f64,
            pub text: *const std::os::raw::c_char,
            pub items: *const PluginValue,
            pub item_count: usize,
        }

        const NONE: PluginValue = PluginValue { tag: PluginValueTag::None, number: 0.0, text: std::ptr::null(), items: std::ptr::null(), item_count: 0 };

        #[repr(C)]
        pub struct PluginFnEntry {
            pub name: *const std::os::raw::c_char,
            pub func: extern "C" fn(*const PluginValue, usize) -> PluginValue,
        }

        #[repr(C)]
        pub struct PluginRegistry {
            pub abi_version: u32,
            pub entries: *const PluginFnEntry,
            pub entry_count: usize,
        }

        extern "C" fn triple(args: *const PluginValue, len: usize) -> PluginValue {
            let args = unsafe { std::slice::from_raw_parts(args, len) };
            PluginValue { tag: PluginValueTag::Number, number: args[0].number * 4.0, ..NONE }
        }

        #[no_mangle]
        pub extern "C" fn phs_plugin_entry() -> PluginRegistry {
            let entries = vec![
                PluginFnEntry { name: std::ffi::CString::new("triple").unwrap().into_raw(), func: triple },
            ];
            let entries: &'static [PluginFnEntry] = Box::leak(entries.into_boxed_slice());
            PluginRegistry { abi_version: 2, entries: entries.as_ptr(), entry_count: entries.len() }
        }
    "#;

    /// Builds `src` into a real cdylib at `<dir>/fixture_plugin.<DLL_EXTENSION>`
    /// with `rustc` (always on PATH wherever `cargo test` runs), so tests
    /// exercise the actual dlopen path, not just the in-process struct layout.
    static BUILD_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn build_fixture_plugin(dir: &Path, stem: &str, src: &str) -> PathBuf {
        let _guard = BUILD_MUTEX.lock().unwrap();
        let src_path = dir.join(format!("{stem}.rs"));
        std::fs::write(&src_path, src).unwrap();
        let out_path = dir.join(format!(
            "{stem}.{}",
            std::env::consts::DLL_EXTENSION
        ));

        let status = Command::new("rustc")
            .args(["--edition", "2021", "--crate-type", "cdylib", "-o"])
            .arg(&out_path)
            .arg(&src_path)
            .status()
            .expect("rustc must be on PATH to build the test fixture plugin");
        assert!(status.success(), "failed to compile fixture plugin");
        std::thread::sleep(std::time::Duration::from_millis(50));
        out_path
    }

    #[test]
    fn test_load_native_ext_end_to_end() {
        let base_dir = std::env::temp_dir().join(format!("phs_plugin_test_e2e_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let ext_dir = base_dir.join("ext");
        std::fs::create_dir_all(&ext_dir).unwrap();
        build_fixture_plugin(&ext_dir, "fixture_plugin_e2e", PLUGIN_SRC_V1);

        let mut state = PluginState::default();
        let functions = state.ensure_stem_loaded(&base_dir, "fixture_plugin_e2e").unwrap().unwrap();
        assert_eq!(functions.len(), 2);
        let result = functions["triple"](&[PhsValue::Number(14.0)]).unwrap();
        assert_eq!(result, PhsValue::Number(42.0));

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn test_load_native_ext_no_ext_dir() {
        let mut state = PluginState::default();
        let result = state
            .ensure_stem_loaded(Path::new("/nonexistent/phs/plugin/dir"), "fixture_plugin")
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_plugin_string_round_trip() {
        let base_dir =
            std::env::temp_dir().join(format!("phs_plugin_string_test_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let ext_dir = base_dir.join("ext");
        std::fs::create_dir_all(&ext_dir).unwrap();
        build_fixture_plugin(&ext_dir, "fixture_plugin_str", PLUGIN_SRC_V1);

        let mut state = PluginState::default();
        let functions = state.ensure_stem_loaded(&base_dir, "fixture_plugin_str").unwrap().unwrap();
        let result = functions["shout"](&[PhsValue::String("hi".into())]).unwrap();
        assert_eq!(result, PhsValue::String("HI".into()));

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn test_quantity_round_trip_through_value_arena() {
        let mut arena = ValueArena::default();
        let q = Quantity::new(10.0, "m/s").unwrap();
        let plugin_value = arena.push(&PhsValue::Quantity(q)).unwrap();
        let back = from_plugin_value(&plugin_value).unwrap();
        assert_eq!(
            back,
            PhsValue::Quantity(Quantity::new(10.0, "m/s").unwrap())
        );
    }

    #[test]
    fn test_vector_round_trip_through_value_arena() {
        let mut arena = ValueArena::default();
        let v = PhsValue::Vector(vec![PhsValue::Number(1.0), PhsValue::Bool(true)]);
        let plugin_value = arena.push(&v).unwrap();
        let back = from_plugin_value(&plugin_value).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_hot_reload_picks_up_changed_plugin() {
        let base_dir =
            std::env::temp_dir().join(format!("phs_plugin_reload_test_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let ext_dir = base_dir.join("ext");
        std::fs::create_dir_all(&ext_dir).unwrap();
        let plugin_path = build_fixture_plugin(&ext_dir, "fixture_plugin_reload", PLUGIN_SRC_V1);

        let mut state = PluginState::default();
        let mut externals = state.ensure_stem_loaded(&base_dir, "fixture_plugin_reload").unwrap().unwrap();
        assert_eq!(
            externals["triple"](&[PhsValue::Number(14.0)]).unwrap(),
            PhsValue::Number(42.0)
        );

        // No change: reload should be a no-op.
        assert!(state.reload_loaded_into(&mut externals).is_empty());

        // Rebuild with different behavior, forcing a later mtime so the reload
        // notices it regardless of filesystem timestamp granularity.
        build_fixture_plugin(&ext_dir, "fixture_plugin_reload", PLUGIN_SRC_V2);
        std::fs::File::open(&plugin_path)
            .unwrap()
            .set_modified(SystemTime::now() + Duration::from_secs(2))
            .unwrap();

        let mut reloaded = state.reload_loaded_into(&mut externals);
        reloaded.sort();
        assert_eq!(reloaded, vec!["triple".to_string()]);
        assert_eq!(
            externals["triple"](&[PhsValue::Number(14.0)]).unwrap(),
            PhsValue::Number(56.0)
        );

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn test_use_statement_native_plugin_round_trip() {
        use crate::interpreter::PhsInterpreter;

        let base_dir =
            std::env::temp_dir().join(format!("phs_plugin_use_test_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let ext_dir = base_dir.join("ext");
        std::fs::create_dir_all(&ext_dir).unwrap();
        build_fixture_plugin(&ext_dir, "fixture_plugin_use", PLUGIN_SRC_V1);

        let mut interp = PhsInterpreter::with_base_dir(&base_dir);
        interp
            .eval_str("use triple, shout from fixture_plugin_use")
            .unwrap();
        let results = interp.eval_str("triple(14)").unwrap();
        assert_eq!(results[0], PhsValue::Number(42.0));
        let results = interp.eval_str("shout(\"hi\")").unwrap();
        assert_eq!(results[0], PhsValue::String("HI".into()));

        let _ = std::fs::remove_dir_all(&base_dir);
    }
}
