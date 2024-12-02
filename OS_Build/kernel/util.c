#include "util.h"
#include <stdint.h>

void memory_copy(char *source, char *dest, int32_t nbytes) {
    for (int i = 0; i < nbytes; i++) {
        dest[i] = source[i];
    }
}

char *int_to_string(int32_t v, char *buff, int32_t radix_base) {
    static char table[] = "0123456789abcdefghijklmnopqrstuvwxyz";
    char *p = buff;
    char *start = buff;
    unsigned int n = (v < 0 && radix_base == 10) ? -v : (unsigned int) v;
    
    while (n >= radix_base) {
        *p++ = table[n % radix_base];
        n /= radix_base;
    }
    *p++ = table[n];
    
    if (v < 0 && radix_base == 10) {
        *p++ = '-';
    }

    *p = '\0';

    for (char *end = p - 1; start < end; ++start, --end) {
        char tmp = *start;
        *start = *end;
        *end = tmp;
    }

    return buff;
}
