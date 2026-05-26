#ifndef _UEFI_SHIM_MATH_H_
#define _UEFI_SHIM_MATH_H_

#define M_PI   3.14159265358979323846
#define HUGE_VAL (__builtin_huge_val())

/* Double-precision math */
double exp(double x);
double log(double x);
double log10(double x);
double pow(double x, double y);
double sqrt(double x);
double sin(double x);
double cos(double x);
double tan(double x);
double asin(double x);
double acos(double x);
double atan(double x);
double atan2(double y, double x);
double floor(double x);
double ceil(double x);
double fabs(double x);
double fmod(double x, double y);
double frexp(double x, int *exp);
double ldexp(double x, int exp);
double rint(double x);
double round(double x);

/* Float variants */
float sqrtf(float x);
float fabsf(float x);
float floorf(float x);
float ceilf(float x);
float powf(float x, float y);
float expf(float x);
float logf(float x);
float sinf(float x);
float cosf(float x);

#endif /* _UEFI_SHIM_MATH_H_ */
