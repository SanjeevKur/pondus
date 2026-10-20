//! [Pondus] The sparse Cholesky factorisation on an NVIDIA GPU with cuDSS, NVIDIA's sparse
//! direct solver, when one is there (a Pondus addition to the vendored engine, after the solver
//! of Tellus3D). The CUDA runtime and cuDSS are loaded at run time, so the same binary runs
//! without them: `available()` is false and the CPU Cholesky in `sparse_chol.rs` does the work.
//!
//! The engine keeps a symmetric matrix as its lower triangle by columns (CSC). Read by rows,
//! the same three arrays are the UPPER triangle in CSR, which cuDSS takes as it is
//! (`CUDSS_MVIEW_UPPER`), so nothing is converted. cuDSS reorders, factorises and keeps the
//! factor on the card; each solve copies the right-hand side over and the solution back.
//!
//! Used by the linear static solve (`prepare_static_3d`), the solve with constraints (rigid
//! diaphragms) and the shift-invert operator of the modal and buckling Lanczos. Its answers
//! agree with the CPU Cholesky to 1e-8 or better on the calibration frames and bridge (2e-13 on
//! a 56,680-element frame); on models with the wall piers' rigid arms (1000 x the wall's E) to
//! about 2e-5, and to 8e-4 in T2 on a 216,588-unknown supertall tower. That is the conditioning
//! of K, not the GPU: two orderings of the same Cholesky round differently there, and on the wall
//! calibration cases the GPU is closer to the closed-form reference in 23 of 33. cuDSS runs in
//! its deterministic mode, so a model gives the same bits every time.
//!
//! Switched off with `PONDUS_GPU=0`. Used for systems of at least `PONDUS_GPU_MIN` unknowns
//! (default 20,000), below which copying to the card costs more than it saves. The libraries
//! are found on the loader path (`LD_LIBRARY_PATH`, or `PATH` on Windows) or at
//! `PONDUS_CUDART` and `PONDUS_CUDSS`. The binding follows cuDSS 0.8 (`pip install
//! nvidia-cudss-cu12`); older libraries are refused because the phase values and the CSR
//! constructor changed.

use std::ffi::{c_char, c_void, CStr};
use std::ptr::{null_mut, NonNull};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use libloading::{Library, Symbol};

use super::sparse::CscMatrix;

// cudss_data_types.h, cuDSS 0.8
const CUDSS_PHASE_ANALYSIS: i32 = 1 | 2;
const CUDSS_PHASE_FACTORIZATION: i32 = 1 << 2;
const CUDSS_PHASE_SOLVE: i32 = (1 << 4) | (1 << 5) | (1 << 6) | (1 << 7) | (1 << 8) | (1 << 9);
const CUDSS_MTYPE_SPD: i32 = 3;
const CUDSS_MVIEW_UPPER: i32 = 2;
const CUDSS_BASE_ZERO: i32 = 0;
const CUDSS_LAYOUT_COL_MAJOR: i32 = 0;
const CUDSS_DATA_INFO: i32 = 0;
const CUDSS_CONFIG_DETERMINISTIC_MODE: i32 = 24;
// cuda_runtime_api.h / library_types.h
const CUDA_R_64F: i32 = 1;
const CUDA_R_32I: i32 = 10;
const MAJOR_VERSION: i32 = 0;
const MINOR_VERSION: i32 = 1;
const CUDSS_MIN_VERSION: (i32, i32) = (0, 8);
const CUDA_MEMCPY_HOST_TO_DEVICE: i32 = 1;
const CUDA_MEMCPY_DEVICE_TO_HOST: i32 = 2;

type P = *mut c_void;

#[allow(non_snake_case)]
struct Api {
    cudaGetDeviceCount: Symbol<'static, unsafe extern "C" fn(*mut i32) -> i32>,
    cudaMalloc: Symbol<'static, unsafe extern "C" fn(*mut P, usize) -> i32>,
    cudaFree: Symbol<'static, unsafe extern "C" fn(P) -> i32>,
    cudaMemcpy: Symbol<'static, unsafe extern "C" fn(P, *const c_void, usize, i32) -> i32>,
    cudaDeviceSynchronize: Symbol<'static, unsafe extern "C" fn() -> i32>,
    cudaGetErrorString: Symbol<'static, unsafe extern "C" fn(i32) -> *const c_char>,
    cudssGetProperty: Symbol<'static, unsafe extern "C" fn(i32, *mut i32) -> i32>,
    cudssCreate: Symbol<'static, unsafe extern "C" fn(*mut P) -> i32>,
    cudssConfigCreate: Symbol<'static, unsafe extern "C" fn(*mut P) -> i32>,
    cudssConfigDestroy: Symbol<'static, unsafe extern "C" fn(P) -> i32>,
    cudssConfigSet: Symbol<'static, unsafe extern "C" fn(P, i32, *const c_void, usize) -> i32>,
    cudssDataCreate: Symbol<'static, unsafe extern "C" fn(P, *mut P) -> i32>,
    cudssDataDestroy: Symbol<'static, unsafe extern "C" fn(P, P) -> i32>,
    cudssDataGet: Symbol<'static, unsafe extern "C" fn(P, P, i32, P, usize, *mut usize) -> i32>,
    #[allow(clippy::type_complexity)]
    cudssMatrixCreateCsr: Symbol<'static, unsafe extern "C" fn(*mut P, i64, i64, i64, P, P, P, P, i32, i32, i32, i32, i32, i32) -> i32>,
    cudssMatrixCreateDn: Symbol<'static, unsafe extern "C" fn(*mut P, i64, i64, i64, P, i32, i32) -> i32>,
    cudssMatrixDestroy: Symbol<'static, unsafe extern "C" fn(P) -> i32>,
    cudssExecute: Symbol<'static, unsafe extern "C" fn(P, i32, P, P, P, P, P) -> i32>,
    /// The cuDSS handle, made once; cuDSS is used from one thread at a time.
    handle: Mutex<NonNull<c_void>>,
    version: (i32, i32),
}

unsafe impl Send for Api {}
unsafe impl Sync for Api {}

static API: OnceLock<Result<Api, String>> = OnceLock::new();
static WARNED: OnceLock<()> = OnceLock::new();
/// Factorisations done on the card in this process.
static GPU_FACTORS: AtomicUsize = AtomicUsize::new(0);

#[cfg(target_os = "windows")]
const CUDART: &[&str] = &["cudart64_12.dll", "cudart64_13.dll", "cudart64_110.dll"];
#[cfg(target_os = "windows")]
const CUDSS: &[&str] = &["cudss64_0.dll", "cudss64.dll"];
#[cfg(not(target_os = "windows"))]
const CUDART: &[&str] = &["libcudart.so.12", "libcudart.so.13", "libcudart.so"];
#[cfg(not(target_os = "windows"))]
const CUDSS: &[&str] = &["libcudss.so.0", "libcudss.so"];

fn load(env: &str, names: &[&str]) -> Result<&'static Library, String> {
    let mut tried = vec![];
    if let Some(p) = std::env::var_os(env) {
        tried.push(p.to_string_lossy().into_owned());
        if let Ok(l) = unsafe { Library::new(&p) } {
            return Ok(Box::leak(Box::new(l)));
        }
    }
    for n in names {
        tried.push(n.to_string());
        if let Ok(l) = unsafe { Library::new(n) } {
            return Ok(Box::leak(Box::new(l)));
        }
    }
    Err(format!("not found ({})", tried.join(", ")))
}

fn api() -> Result<&'static Api, &'static String> {
    API.get_or_init(|| {
        if std::env::var("PONDUS_GPU").is_ok_and(|v| v == "0" || v.eq_ignore_ascii_case("off")) {
            return Err("switched off (PONDUS_GPU=0)".into());
        }
        if cfg!(target_os = "macos") {
            return Err("CUDA is not available on macOS".into());
        }
        let rt = load("PONDUS_CUDART", CUDART).map_err(|e| format!("CUDA runtime {e}"))?;
        let ds = load("PONDUS_CUDSS", CUDSS).map_err(|e| format!("cuDSS {e}"))?;
        macro_rules! sym {
            ($lib:expr, $name:literal) => {
                unsafe { $lib.get(concat!($name, "\0").as_bytes()) }.map_err(|e| format!("{}: {e}", $name))?
            };
        }
        let mut api = Api {
            cudaGetDeviceCount: sym!(rt, "cudaGetDeviceCount"),
            cudaMalloc: sym!(rt, "cudaMalloc"),
            cudaFree: sym!(rt, "cudaFree"),
            cudaMemcpy: sym!(rt, "cudaMemcpy"),
            cudaDeviceSynchronize: sym!(rt, "cudaDeviceSynchronize"),
            cudaGetErrorString: sym!(rt, "cudaGetErrorString"),
            cudssGetProperty: sym!(ds, "cudssGetProperty"),
            cudssCreate: sym!(ds, "cudssCreate"),
            cudssConfigCreate: sym!(ds, "cudssConfigCreate"),
            cudssConfigDestroy: sym!(ds, "cudssConfigDestroy"),
            cudssConfigSet: sym!(ds, "cudssConfigSet"),
            cudssDataCreate: sym!(ds, "cudssDataCreate"),
            cudssDataDestroy: sym!(ds, "cudssDataDestroy"),
            cudssDataGet: sym!(ds, "cudssDataGet"),
            cudssMatrixCreateCsr: sym!(ds, "cudssMatrixCreateCsr"),
            cudssMatrixCreateDn: sym!(ds, "cudssMatrixCreateDn"),
            cudssMatrixDestroy: sym!(ds, "cudssMatrixDestroy"),
            cudssExecute: sym!(ds, "cudssExecute"),
            handle: Mutex::new(NonNull::dangling()),
            version: (0, 0),
        };
        let mut n = 0i32;
        let st = unsafe { (api.cudaGetDeviceCount)(&mut n) };
        if st != 0 || n < 1 {
            return Err(format!("no CUDA device ({})", api.cuda_error(st)));
        }
        let (mut major, mut minor) = (0i32, 0i32);
        unsafe {
            (api.cudssGetProperty)(MAJOR_VERSION, &mut major);
            (api.cudssGetProperty)(MINOR_VERSION, &mut minor);
        }
        if (major, minor) < CUDSS_MIN_VERSION {
            return Err(format!("cuDSS {major}.{minor} is older than {}.{}", CUDSS_MIN_VERSION.0, CUDSS_MIN_VERSION.1));
        }
        api.version = (major, minor);
        let mut h: P = null_mut();
        let st = unsafe { (api.cudssCreate)(&mut h) };
        if st != 0 || h.is_null() {
            return Err(format!("cudssCreate failed (status {st})"));
        }
        *api.handle.lock().unwrap() = NonNull::new(h).unwrap();
        Ok(api)
    })
    .as_ref()
}

impl Api {
    fn cuda_error(&self, st: i32) -> String {
        let p = unsafe { (self.cudaGetErrorString)(st) };
        if p.is_null() { format!("CUDA error {st}") } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
    }

    fn check(&self, st: i32, what: &str) -> Result<(), String> {
        if st == 0 { Ok(()) } else { Err(format!("{what}: {}", self.cuda_error(st))) }
    }

    fn cudss(&self, st: i32, what: &str) -> Result<(), String> {
        if st == 0 { Ok(()) } else { Err(format!("{what}: cuDSS status {st}")) }
    }

    /// Device memory holding a copy of `v`.
    fn upload<T: Copy>(&self, v: &[T]) -> Result<Dev, String> {
        let bytes = std::mem::size_of_val(v).max(1);
        let mut p: P = null_mut();
        self.check(unsafe { (self.cudaMalloc)(&mut p, bytes) }, "cudaMalloc")?;
        let d = Dev(p);
        self.check(unsafe { (self.cudaMemcpy)(p, v.as_ptr() as *const c_void, std::mem::size_of_val(v), CUDA_MEMCPY_HOST_TO_DEVICE) }, "cudaMemcpy to the device")?;
        Ok(d)
    }
}

/// A device allocation, freed when dropped.
struct Dev(P);

impl Drop for Dev {
    fn drop(&mut self) {
        if let Ok(a) = api() {
            unsafe { (a.cudaFree)(self.0) };
        }
    }
}

/// Whether the GPU solver can be used: the libraries loaded and a device is there.
pub fn available() -> bool {
    api().is_ok()
}

/// The smallest system put on the card (`PONDUS_GPU_MIN`, default 20,000 unknowns).
pub fn min_size() -> usize {
    std::env::var("PONDUS_GPU_MIN").ok().and_then(|v| v.parse().ok()).unwrap_or(20_000)
}

/// Whether a system of `n` unknowns goes to the card.
pub fn use_for(n: usize) -> bool {
    n >= min_size() && available()
}

/// One line for the user: where the factorisations run.
pub fn summary() -> String {
    match api() {
        Ok(a) => format!("GPU available (CUDA, cuDSS {}.{}; systems of {} unknowns or more; {} factorised on the GPU so far)", a.version.0, a.version.1, min_size(), GPU_FACTORS.load(Ordering::Relaxed)),
        Err(e) => format!("CPU only: GPU not used ({e})"),
    }
}

/// How many factorisations ran on the card in this process.
pub fn factorisations() -> usize {
    GPU_FACTORS.load(Ordering::Relaxed)
}

fn warn_once(msg: &str) {
    if WARNED.set(()).is_ok() {
        eprintln!("pondus: {msg}");
    }
}

/// A factorised symmetric positive definite matrix on the card.
pub struct Factor {
    n: usize,
    config: P,
    data: P,
    mat_a: P,
    mat_x: P,
    mat_b: P,
    _dev: Vec<Dev>,
    d_x: P,
    d_b: P,
}

unsafe impl Send for Factor {}
unsafe impl Sync for Factor {}

impl Factor {
    /// Factorises the SPD matrix `a` (lower triangle by columns). `Err(None)` when it is not
    /// positive definite (the CPU path then reports the mechanism as it always has),
    /// `Err(Some(msg))` when the GPU fails (the caller then factorises on the CPU).
    pub fn new(a: &CscMatrix) -> Result<Factor, Option<String>> {
        let api = api().map_err(|e| Some(e.clone()))?;
        match Self::build(api, a) {
            Ok(f) => {
                GPU_FACTORS.fetch_add(1, Ordering::Relaxed);
                Ok(f)
            }
            Err(e) if e.starts_with("pivot") => Err(None),
            Err(e) => {
                warn_once(&format!("GPU factorisation failed, using the CPU: {e}"));
                Err(Some(e))
            }
        }
    }

    fn build(a: &'static Api, m: &CscMatrix) -> Result<Factor, String> {
        let n = m.n;
        let nnz = m.col_ptr[n];
        if n > i32::MAX as usize || nnz > i32::MAX as usize {
            return Err("the matrix is too large for 32-bit indices".into());
        }
        // lower triangle by columns = upper triangle by rows
        let rp: Vec<i32> = m.col_ptr.iter().map(|&v| v as i32).collect();
        let ci: Vec<i32> = m.row_idx[..nnz].iter().map(|&v| v as i32).collect();
        let d_rp = a.upload(&rp)?;
        let d_ci = a.upload(&ci)?;
        let d_ax = a.upload(&m.values[..nnz])?;
        let d_x = a.upload(&vec![0.0f64; n])?;
        let d_b = a.upload(&vec![0.0f64; n])?;
        let mut f = Factor { n, config: null_mut(), data: null_mut(), mat_a: null_mut(), mat_x: null_mut(), mat_b: null_mut(), d_x: d_x.0, d_b: d_b.0, _dev: vec![d_rp, d_ci, d_ax, d_x, d_b] };
        // the handle's lock is released before `f` can be dropped on an error, since its Drop
        // takes the same lock
        let guard = a.handle.lock().unwrap();
        let h = guard.as_ptr();
        let built = (|| -> Result<(), String> {
            unsafe {
                a.cudss((a.cudssConfigCreate)(&mut f.config), "cudssConfigCreate")?;
                // the same model gives the same answer to the last bit, run after run (by default
                // cuDSS sums in an order that changes between runs, by about 1e-12)
                let on = 1i32;
                a.cudss((a.cudssConfigSet)(f.config, CUDSS_CONFIG_DETERMINISTIC_MODE, &on as *const i32 as *const c_void, std::mem::size_of::<i32>()), "cudssConfigSet deterministic")?;
                a.cudss((a.cudssDataCreate)(h, &mut f.data), "cudssDataCreate")?;
                a.cudss((a.cudssMatrixCreateCsr)(&mut f.mat_a, n as i64, n as i64, nnz as i64, f._dev[0].0, null_mut(), f._dev[1].0, f._dev[2].0, CUDA_R_32I, CUDA_R_32I, CUDA_R_64F, CUDSS_MTYPE_SPD, CUDSS_MVIEW_UPPER, CUDSS_BASE_ZERO), "cudssMatrixCreateCsr")?;
                a.cudss((a.cudssMatrixCreateDn)(&mut f.mat_x, n as i64, 1, n as i64, f.d_x, CUDA_R_64F, CUDSS_LAYOUT_COL_MAJOR), "cudssMatrixCreateDn x")?;
                a.cudss((a.cudssMatrixCreateDn)(&mut f.mat_b, n as i64, 1, n as i64, f.d_b, CUDA_R_64F, CUDSS_LAYOUT_COL_MAJOR), "cudssMatrixCreateDn b")?;
                a.cudss((a.cudssExecute)(h, CUDSS_PHASE_ANALYSIS, f.config, f.data, f.mat_a, f.mat_x, f.mat_b), "cuDSS analysis")?;
                a.cudss((a.cudssExecute)(h, CUDSS_PHASE_FACTORIZATION, f.config, f.data, f.mat_a, f.mat_x, f.mat_b), "cuDSS factorisation")?;
                a.check((a.cudaDeviceSynchronize)(), "factorisation")?;
                // a non-positive pivot: INFO is its (1-based) index
                let mut info = 0i32;
                let mut written = 0usize;
                a.cudss((a.cudssDataGet)(h, f.data, CUDSS_DATA_INFO, &mut info as *mut i32 as P, std::mem::size_of::<i32>(), &mut written), "cudssDataGet info")?;
                if info != 0 {
                    return Err(format!("pivot {}", info.unsigned_abs()));
                }
                Ok(())
            }
        })();
        drop(guard);
        built?;
        Ok(f)
    }

    pub fn n(&self) -> usize {
        self.n
    }

    /// Solves A x = b, returning x.
    pub fn solve(&self, b: &[f64]) -> Result<Vec<f64>, String> {
        let a = api().map_err(|e| e.clone())?;
        assert_eq!(b.len(), self.n);
        let mut x = vec![0.0; self.n];
        let h = a.handle.lock().unwrap();
        unsafe {
            a.check((a.cudaMemcpy)(self.d_b, b.as_ptr() as *const c_void, 8 * self.n, CUDA_MEMCPY_HOST_TO_DEVICE), "cudaMemcpy b")?;
            a.cudss((a.cudssExecute)(h.as_ptr(), CUDSS_PHASE_SOLVE, self.config, self.data, self.mat_a, self.mat_x, self.mat_b), "cuDSS solve")?;
            a.check((a.cudaDeviceSynchronize)(), "solve")?;
            a.check((a.cudaMemcpy)(x.as_mut_ptr() as P, self.d_x, 8 * self.n, CUDA_MEMCPY_DEVICE_TO_HOST), "cudaMemcpy x")?;
        }
        Ok(x)
    }
}

impl Drop for Factor {
    fn drop(&mut self) {
        if let Ok(a) = api() {
            let h = a.handle.lock().unwrap();
            unsafe {
                for m in [self.mat_a, self.mat_x, self.mat_b] {
                    if !m.is_null() {
                        (a.cudssMatrixDestroy)(m);
                    }
                }
                if !self.data.is_null() {
                    (a.cudssDataDestroy)(h.as_ptr(), self.data);
                }
                if !self.config.is_null() {
                    (a.cudssConfigDestroy)(self.config);
                }
            }
        }
    }
}

/// A factor that is on the card or on the CPU, with one solve.
pub enum SpdFactor {
    Cpu(super::sparse_chol::NumericCholesky),
    Gpu(Factor),
}

impl SpdFactor {
    /// The GPU for a large enough system when it is there, else the CPU Cholesky (None when K is
    /// not positive definite). A matrix the GPU finds indefinite is factorised on the CPU too,
    /// so that the engine's own handling of a mechanism stays exactly as it was.
    pub fn new(a: &CscMatrix) -> Option<SpdFactor> {
        if use_for(a.n) {
            if let Ok(f) = Factor::new(a) {
                return Some(SpdFactor::Gpu(f));
            }
        }
        let sym = std::rc::Rc::new(super::sparse_chol::symbolic_cholesky(a));
        super::sparse_chol::numeric_cholesky(&sym, a).map(SpdFactor::Cpu)
    }

    pub fn solve(&self, b: &[f64]) -> Vec<f64> {
        match self {
            SpdFactor::Cpu(f) => super::sparse_chol::sparse_cholesky_solve(f, b),
            SpdFactor::Gpu(f) => f.solve(b).unwrap_or_else(|e| panic!("GPU solve failed after a good factorisation: {e}")),
        }
    }

    pub fn on_gpu(&self) -> bool {
        matches!(self, SpdFactor::Gpu(_))
    }
}


