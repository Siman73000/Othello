#ifndef UTIL_H
#define UTIL_H
#include <stdint.h>

void memory_copy(char *source, char *dest, int32_t nbytes);
char *int_to_string(int32_t v, char *buff, int32_t radix_base);

#endif