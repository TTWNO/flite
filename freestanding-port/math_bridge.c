/* Floating-point ABI bridge (compiled by clang for x86_64-unknown-uefi).
 *
 * clang passes/returns `double` in XMM0 (MS x64 ABI). Rust's uefi target keeps
 * floats in INTEGER registers, so a direct clang->Rust `double` call exchanges
 * the value through the wrong register file. Integer args/returns DO match, so
 * here (clang side, native XMM0) we forward the f64 *bit pattern* as a u64 to
 * the Rust shim and reinterpret the u64 result. flite's <math.h>/<stdlib.h>
 * #define sqrt/exp/.../atof to these cstm_* entry points. */
typedef unsigned long long u64;

extern u64 rust_ceil_bits(u64);
extern u64 rust_exp_bits(u64);
extern u64 rust_fabs_bits(u64);
extern u64 rust_fmod_bits(u64, u64);
extern u64 rust_log_bits(u64);
extern u64 rust_pow_bits(u64, u64);
extern u64 rust_sin_bits(u64);
extern u64 rust_sqrt_bits(u64);
extern u64 rust_atof_bits(const char *);

static inline u64    to_bits(double d) { union { double d; u64 u; } t; t.d = d; return t.u; }
static inline double of_bits(u64 u)    { union { double d; u64 u; } t; t.u = u; return t.d; }

double cstm_ceil(double x)          { return of_bits(rust_ceil_bits(to_bits(x))); }
double cstm_exp(double x)           { return of_bits(rust_exp_bits(to_bits(x))); }
double cstm_fabs(double x)          { return of_bits(rust_fabs_bits(to_bits(x))); }
double cstm_fmod(double x, double y){ return of_bits(rust_fmod_bits(to_bits(x), to_bits(y))); }
double cstm_log(double x)           { return of_bits(rust_log_bits(to_bits(x))); }
double cstm_pow(double x, double y) { return of_bits(rust_pow_bits(to_bits(x), to_bits(y))); }
double cstm_sin(double x)           { return of_bits(rust_sin_bits(to_bits(x))); }
double cstm_sqrt(double x)          { return of_bits(rust_sqrt_bits(to_bits(x))); }
double cstm_atof(const char *s)     { return of_bits(rust_atof_bits(s)); }
