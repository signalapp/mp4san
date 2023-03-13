#include <stdio.h>
#include <string.h>
#include <gpac/tools.h>
#include "log.h"

void mp4san_test_gpac_log(GF_LOG_Level, GF_LOG_Tool, const char*);

void mp4san_test_gpac_log_callback(void* ptr, GF_LOG_Level level, GF_LOG_Tool tool, const char* fmt, va_list vl) {
    (void) ptr;
  
    char buffer[4096];
    memset(buffer, 0, sizeof(buffer));
    vsnprintf(buffer, sizeof(buffer), fmt, vl);
    mp4san_test_gpac_log(level, tool, buffer);
}
