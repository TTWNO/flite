#ifndef _UEFI_SHIM_STDLIB_H_
#define _UEFI_SHIM_STDLIB_H_

#include <stddef.h>

#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1
#define RAND_MAX     32767

#ifndef NULL
#define NULL ((void *)0)
#endif

/* Memory management */
void  *malloc(size_t size);
void  *calloc(size_t nmemb, size_t size);
void  *realloc(void *ptr, size_t size);
void   free(void *ptr);

/* Program control */
void   abort(void);
void   exit(int status);

/* String/number conversion */
int    atoi(const char *s);
double atof(const char *s);
double strtod(const char *s, char **endptr);
long   strtol(const char *s, char **endptr, int base);

/* Utilities */
void   qsort(void *base, size_t nmemb, size_t size,
             int (*compar)(const void *, const void *));
int    rand(void);
void   srand(unsigned int seed);
char  *getenv(const char *name);
int    abs(int j);

#endif /* _UEFI_SHIM_STDLIB_H_ */
