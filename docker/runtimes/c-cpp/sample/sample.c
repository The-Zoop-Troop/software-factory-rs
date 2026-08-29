#include <stdio.h>
#include "sample.h"
int greet(char *out, size_t n, const char *name) { return snprintf(out, n, "hello %s", name); }
