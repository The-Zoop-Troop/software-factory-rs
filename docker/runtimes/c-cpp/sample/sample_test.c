#include <string.h>
#include "sample.h"
int main(void) { char b[32]; greet(b, sizeof b, "rig"); return strcmp(b, "hello rig") == 0 ? 0 : 1; }
